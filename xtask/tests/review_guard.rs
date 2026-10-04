use dotfiles_xtask::review_guard::{
    DAY_MS, HOUR_MS, Invocation, PROBE_GAP_MS, check_budget, check_usage, classify, execute_probe,
    execute_reserved, install, reserve, review_directory, usage_query, validate_policy,
};
use dotfiles_xtask::review_rules::{capacity_budget, included_allowance};
use std::fs;
use std::io::{Read, Seek};

fn invocation(args: &[&str]) -> Invocation {
    classify(&args.iter().map(|arg| (*arg).into()).collect::<Vec<_>>())
}

#[test]
fn layered_capacity_uses_exact_single_rounding_for_small_and_maximum_limits() {
    for (capacity, expected) in [(0, 0), (1, 0), (2, 1), (3, 2), (10, 7), (49, 35), (50, 36)] {
        assert_eq!(capacity_budget(capacity), expected);
    }
    assert_eq!(
        u128::from(capacity_budget(u64::MAX)),
        u128::from(u64::MAX) * 72 / 100
    );
}

#[test]
fn admission_matches_the_final_decimal_budget_at_capacity_boundaries() {
    for limit in 0..=128 {
        let budget = u128::from(limit) * 72 / 100;
        for remaining in 0..=limit + 1 {
            let expected = remaining <= limit && u128::from(limit - remaining) < budget;
            assert_eq!(included_allowance(limit, remaining), expected);
        }
    }
    let limit = u64::MAX;
    let budget = (u128::from(limit) * 72 / 100) as u64;
    assert!(included_allowance(limit, limit - budget + 1));
    assert!(!included_allowance(limit, limit - budget));
}

#[test]
fn every_analysis_route_is_limited_and_paid_or_cloud_routes_are_refused() {
    for args in [
        vec![],
        vec!["--agent"],
        vec!["review"],
        vec!["review", "--deep"],
        vec!["config"],
    ] {
        assert_eq!(invocation(&args), Invocation::Review);
    }
    for args in [
        vec!["--version"],
        vec!["--help"],
        vec!["auth", "status"],
        vec!["usage", "--agent"],
        vec!["review", "findings"],
        vec!["review", "--show-prompts"],
    ] {
        assert_eq!(invocation(&args), Invocation::ReadOnly, "{args:?}");
    }
    for args in [
        vec!["review", "--use-credits"],
        vec!["review", "--use-credits=true"],
        vec!["review", "--api-key=x"],
        vec!["code", "handoff"],
        vec!["handoff"],
        vec!["update"],
        vec!["--agent", "code", "handoff"],
        vec!["--agent", "update"],
        vec!["review", "--remote", "other/repo"],
    ] {
        assert_eq!(invocation(&args), Invocation::Forbidden, "{args:?}");
    }
    assert_eq!(
        invocation(&["review", "--config", "--help"]),
        Invocation::Review
    );
}

#[test]
fn command_words_are_valid_option_values_and_cannot_hide_cloud_commands() {
    for args in [
        vec!["review", "--dir", "code"],
        vec!["review", "--base", "update"],
        vec!["review", "--config", "handoff"],
        vec!["--dir", "code", "--agent"],
        vec!["--base=update", "--agent"],
        vec!["--deep", "code"],
        vec!["--config", "code", "update"],
        vec!["--config=code", "handoff"],
    ] {
        assert_eq!(invocation(&args), Invocation::Review, "{args:?}");
    }
    assert_eq!(
        invocation(&["auth", "login", "--organization", "code"]),
        Invocation::ReadOnly
    );
    for args in [
        vec!["--agent", "code", "handoff"],
        vec!["--base", "main", "code", "handoff"],
        vec!["--dir", "code", "update"],
        vec!["--config", "code", "--agent", "update"],
        vec!["--", "code", "handoff"],
    ] {
        assert_eq!(invocation(&args), Invocation::Forbidden, "{args:?}");
    }
}

#[test]
fn rolling_hour_daily_budget_and_clock_rollback_fail_closed() {
    assert!(check_budget(&[100], 101).is_ok());
    assert!(check_budget(&[100], HOUR_MS + 100).is_ok());
    assert!(check_budget(&[1, 2, 3, 4, 5, 6, 7], HOUR_MS).is_err());
    assert!(check_budget(&[1, 2, 3, 4, 5, 6, 7], HOUR_MS + 1).is_ok());
    let records = vec![100; 168];
    assert!(check_budget(&records, DAY_MS - 1).is_err());
    assert!(check_budget(&records, DAY_MS + 100).is_ok());
    assert!(check_budget(&[100], 99).is_err());
    assert!(check_budget(&[100, 1], 99).is_err());
}

const USAGE: &str = r#"{"type":"status","phase":"usage","status":"complete","organization":"P4suta","includedReviews":{"state":"available","repository":"P4suta/dotfiles","fullCapacityInMs":0,"limit":10,"remaining":10,"rollingWindowMs":3600000},"billingPeriod":{"state":"available","scope":"user","usageBillingStatus":"inactive","user":"P4suta","reviewsCount":0}}"#;

#[test]
fn only_known_included_allowance_with_inactive_paid_usage_is_accepted() {
    assert!(check_usage(USAGE, "P4suta").is_ok());
    assert!(check_usage(USAGE, "other").is_err());
    assert!(
        check_usage(
            &USAGE.replace("\"user\":\"P4suta\"", "\"user\":\"other\""),
            "P4suta"
        )
        .is_err()
    );
    for (from, to) in [
        ("\"remaining\":10", "\"remaining\":0"),
        ("\"remaining\":10", "\"remaining\":1"),
        ("\"remaining\":10", "\"remaining\":2"),
        ("\"remaining\":10", "\"remaining\":3"),
        ("\"remaining\":10", "\"remaining\":11"),
        ("\"limit\":10", "\"limit\":2"),
        ("\"inactive\"", "\"active\""),
        ("\"state\":\"available\"", "\"state\":\"unavailable\""),
        (
            "\"rollingWindowMs\":3600000",
            "\"rollingWindowMs\":86400000",
        ),
        ("\"status\":\"complete\"", "\"status\":\"partial\""),
    ] {
        assert!(
            check_usage(&USAGE.replace(from, to), "P4suta").is_err(),
            "{to}"
        );
    }
    assert!(check_usage("not JSON", "P4suta").is_err());
    assert!(check_usage("{}", "P4suta").is_err());
    assert!(check_usage(&format!("{USAGE}\n{USAGE}"), "P4suta").is_err());
    assert!(
        check_usage(
            &USAGE.replace("\"remaining\":10", "\"remaining\":4"),
            "P4suta"
        )
        .is_ok()
    );
}

#[test]
fn allowance_queries_follow_the_review_directory_and_reject_ambiguous_targets() {
    let args = |values: &[&str]| {
        values
            .iter()
            .map(|value| (*value).into())
            .collect::<Vec<_>>()
    };
    assert_eq!(review_directory(&args(&["review"])).unwrap(), None);
    for values in [
        vec!["review", "--dir", "../repo"],
        vec!["review", "--dir=../repo"],
    ] {
        assert_eq!(
            review_directory(&args(&values)).unwrap(),
            Some("../repo".into())
        );
    }
    for values in [
        vec!["review", "--dir"],
        vec!["review", "--dir="],
        vec!["review", "--dir", "--agent"],
        vec!["review", "--dir=a", "--dir=b"],
    ] {
        assert!(review_directory(&args(&values)).is_err());
    }
}

#[test]
fn reservation_is_durable_before_work_and_serializes_other_processes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviews.log");
    fs::write(&path, "").unwrap();
    let mut first = reserve(&path, 100).unwrap();
    first.rewind().unwrap();
    let mut recorded = String::new();
    first.read_to_string(&mut recorded).unwrap();
    assert_eq!(recorded, "100\n");
    assert!(reserve(&path, 101).is_err());
    drop(first);
    for now in 101..107 {
        drop(reserve(&path, now).unwrap());
    }
    assert!(reserve(&path, 107).is_err());
    assert!(reserve(&path, HOUR_MS + 99).is_err());
    drop(reserve(&path, HOUR_MS + 100).unwrap());
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        format!("100\n101\n102\n103\n104\n105\n106\n{}\n", HOUR_MS + 100)
    );
}

#[cfg(unix)]
#[test]
fn inherited_ledger_handle_child() {
    use std::io::Write;
    if std::env::var("DOTFILES_GUARD_LOCK_CHILD").as_deref() == Ok("inherited-ledger-handle-v1") {
        println!("ledger-lock-ready");
        std::io::stdout().flush().unwrap();
        std::io::stdin().read_exact(&mut [0]).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn reservation_release_is_not_delayed_by_an_inherited_child_handle() {
    use std::io::{BufRead, Write};
    use std::process::{Command, Stdio};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviews.log");
    fs::write(&path, "").unwrap();
    let first = reserve(&path, 100).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "inherited_ledger_handle_child", "--nocapture"])
        .env("DOTFILES_GUARD_LOCK_CHILD", "inherited-ledger-handle-v1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(first.try_clone().unwrap()))
        .spawn()
        .unwrap();
    let mut output = std::io::BufReader::new(child.stdout.take().unwrap());
    loop {
        let mut line = String::new();
        assert!(output.read_line(&mut line).unwrap() > 0);
        if line.contains("ledger-lock-ready") {
            break;
        }
    }
    assert!(reserve(&path, 101).is_err());
    drop(first);
    let next = reserve(&path, 101);
    child.stdin.take().unwrap().write_all(&[0]).unwrap();
    assert!(child.wait().unwrap().success());
    assert!(next.is_ok(), "{next:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), "100\n101\n");
}

#[test]
fn missing_truncated_corrupt_and_unsorted_ledgers_never_reset_allowance() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviews.log");
    assert!(reserve(&path, 100).is_err());
    for text in ["1", "one\n", "2\n1\n", "1\n\n"] {
        fs::write(&path, text).unwrap();
        assert!(reserve(&path, 100).is_err(), "{text:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }
}

#[test]
fn oversized_audit_history_requires_maintenance_without_discarding_entries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviews.log");
    let history = "1\n".repeat(524_289);
    fs::write(&path, &history).unwrap();
    let error = reserve(&path, DAY_MS).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("operator maintenance is required")
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), history);
}

#[test]
fn failed_preflight_consumes_allowance_and_never_launches_a_review() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("reviews.log");
    let probes = dir.path().join("usage.log");
    fs::write(&ledger, "").unwrap();
    fs::write(&probes, "").unwrap();
    let forbidden_review = || panic!("a review must not start");
    assert!(
        execute_reserved(
            &ledger,
            &probes,
            100,
            "P4suta",
            || Ok("{}".into()),
            forbidden_review
        )
        .is_err()
    );
    assert_eq!(fs::read_to_string(&ledger).unwrap(), "100\n");
    assert_eq!(
        execute_reserved(
            &ledger,
            &probes,
            HOUR_MS + 100,
            "P4suta",
            || Ok(USAGE.into()),
            || Ok(23)
        )
        .unwrap(),
        23
    );
    assert!(
        execute_reserved(
            &ledger,
            &probes,
            HOUR_MS + 101,
            "P4suta",
            || panic!("preflight must not start"),
            forbidden_review
        )
        .is_err()
    );
    assert_eq!(
        fs::read_to_string(&ledger).unwrap(),
        format!("100\n{}\n{}\n", HOUR_MS + 100, HOUR_MS + 101)
    );
}

#[test]
fn quota_queries_are_durable_bounded_and_never_retried_after_failure() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("usage.log");
    fs::write(&ledger, "").unwrap();
    assert!(execute_probe::<()>(&ledger, 100, || anyhow::bail!("service unavailable")).is_err());
    assert_eq!(fs::read_to_string(&ledger).unwrap(), "100\n");
    for now in [99, 100, PROBE_GAP_MS + 99] {
        assert!(
            execute_probe(&ledger, now, || -> anyhow::Result<()> {
                panic!("query must not run")
            })
            .is_err()
        );
    }
    for index in 1..7 {
        let now = 100 + index * PROBE_GAP_MS;
        execute_probe(&ledger, now, || {
            assert!(execute_probe::<()>(&ledger, now, || panic!("concurrent query")).is_err());
            Ok(())
        })
        .unwrap();
    }
    assert!(
        execute_probe::<()>(&ledger, 100 + 7 * PROBE_GAP_MS, || panic!(
            "hourly budget exhausted"
        ))
        .is_err()
    );
    for hour in 1..24 {
        for index in 0..7 {
            execute_probe(&ledger, 100 + hour * HOUR_MS + index * PROBE_GAP_MS, || {
                Ok(())
            })
            .unwrap();
        }
    }
    assert_eq!(fs::read_to_string(&ledger).unwrap().lines().count(), 168);
    assert!(
        execute_probe::<()>(&ledger, DAY_MS + 99, || panic!(
            "budget must not expire early"
        ))
        .is_err()
    );
    execute_probe(&ledger, DAY_MS + 100, || Ok(())).unwrap();
}

#[test]
fn every_supported_usage_spelling_uses_the_probe_gate() {
    for args in [
        vec!["usage"],
        vec!["usage", "--agent"],
        vec!["--agent", "usage"],
        vec!["--usage"],
        vec!["--usage", "--agent"],
        vec!["review", "--agent", "--usage"],
    ] {
        let args: Vec<String> = args.iter().map(|arg| (*arg).into()).collect();
        assert!(usage_query(&args));
        assert_eq!(classify(&args), Invocation::ReadOnly);
    }
    for args in [
        vec!["review"],
        vec!["review", "--dir", "usage"],
        vec!["review", "--base", "usage"],
        vec!["review", "findings"],
    ] {
        let args: Vec<String> = args.iter().map(|arg| (*arg).into()).collect();
        assert!(!usage_query(&args));
    }
    assert_eq!(
        invocation(&["--usage", "--use-credits"]),
        Invocation::Forbidden
    );
}

#[test]
fn managed_hosts_share_one_review_executor_and_probe_history_survives_installation() {
    assert!(
        validate_policy(
            r#"{"organization":"P4suta","vendor_version":"0.8.2","review_os":"macos"}"#
        )
        .is_ok()
    );
    for policy in [
        r#"{"organization":"P4suta","vendor_version":"0.8.2"}"#,
        r#"{"organization":"P4suta","vendor_version":"0.8.2","review_os":"windows"}"#,
    ] {
        assert!(validate_policy(policy).is_err());
    }
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("guard");
    fs::write(&binary, "guard").unwrap();
    let home = dir.path().join("user");
    install(&binary, &home).unwrap();
    let probes = home.join(".local/state/coderabbit-guard/usage.log");
    execute_probe(&probes, 100, || Ok(())).unwrap();
    install(&binary, &home).unwrap();
    assert_eq!(fs::read_to_string(&probes).unwrap(), "100\n");
    fs::remove_file(&probes).unwrap();
    assert!(install(&binary, &home).is_err());
    assert!(!probes.exists());
}

#[test]
fn owner_pause_blocks_the_real_entry_point_before_vendor_lookup_and_survives_install() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("user");
    let state = home.join(".local/state/coderabbit-guard");
    let policy = home.join(".config/coderabbit-guard");
    fs::create_dir_all(&state).unwrap();
    fs::create_dir_all(&policy).unwrap();
    fs::write(state.join("paused"), "owner pause").unwrap();
    fs::write(
        policy.join("policy.json"),
        r#"{"organization":"P4suta","vendor_version":"0.8.2","review_os":"macos"}"#,
    )
    .unwrap();
    let binary = std::path::Path::new(env!("CARGO_BIN_EXE_coderabbit"));
    install(binary, &home).unwrap();
    for args in [
        vec!["review"],
        vec!["usage", "--agent"],
        vec!["--usage", "--agent"],
        vec!["auth", "status"],
        vec!["doctor"],
        vec!["pullrequest", "18"],
        vec!["review", "findings"],
    ] {
        let output = std::process::Command::new(binary)
            .args(args)
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(75));
        assert!(String::from_utf8_lossy(&output.stderr).contains("paused by the owner"));
    }
    let output = std::process::Command::new(binary)
        .arg("--guard-status")
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("paused true"));
    assert_eq!(fs::read_to_string(state.join("reviews.log")).unwrap(), "");
    assert_eq!(fs::read_to_string(state.join("usage.log")).unwrap(), "");
    install(binary, &home).unwrap();
    assert_eq!(
        fs::read_to_string(state.join("paused")).unwrap(),
        "owner pause"
    );
}

#[test]
fn reinstall_preserves_history_and_a_deleted_ledger_is_not_recreated() {
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("built-guard");
    let home = dir.path().join("user");
    fs::write(&binary, "guard-v1").unwrap();
    install(&binary, &home).unwrap();
    let ledger = home.join(".local/state/coderabbit-guard/reviews.log");
    drop(reserve(&ledger, 100).unwrap());
    fs::write(&binary, "guard-v2").unwrap();
    install(&binary, &home).unwrap();
    assert_eq!(fs::read_to_string(&ledger).unwrap(), "100\n");
    for name in ["coderabbit", "cr"] {
        let destination = home
            .join(".local/bin")
            .join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        assert_eq!(fs::read_to_string(destination).unwrap(), "guard-v2");
    }
    fs::remove_file(&ledger).unwrap();
    assert!(install(&binary, &home).is_err());
    assert!(!ledger.exists());
}

#[test]
fn failed_first_initialization_can_retry_without_an_initialized_marker() {
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("built-guard");
    let home = dir.path().join("user");
    let state = home.join(".local/state/coderabbit-guard");
    let ledger = state.join("reviews.log");
    fs::write(&binary, "guard").unwrap();
    fs::create_dir_all(&ledger).unwrap();
    assert!(install(&binary, &home).is_err());
    assert!(!state.join("initialized").exists());
    fs::remove_dir(&ledger).unwrap();
    install(&binary, &home).unwrap();
    assert!(state.join("initialized").is_file());
    assert_eq!(fs::read_to_string(&ledger).unwrap(), "");
}

#[test]
fn interrupted_initialization_keeps_the_existing_ledger_history() {
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("built-guard");
    let home = dir.path().join("user");
    let state = home.join(".local/state/coderabbit-guard");
    let ledger = state.join("reviews.log");
    fs::write(&binary, "guard").unwrap();
    fs::create_dir_all(&state).unwrap();
    fs::write(&ledger, "100\n").unwrap();
    install(&binary, &home).unwrap();
    assert!(state.join("initialized").is_file());
    assert_eq!(fs::read_to_string(&ledger).unwrap(), "100\n");
}
