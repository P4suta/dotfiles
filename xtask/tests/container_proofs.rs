use anyhow::Context as _;
use serde_json::json;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use xtask::alloy::{AssetControlResult, CORRUPT_ASSETS, CorruptAsset};
use xtask::skill_proofs::{
    FUZZ_VERSION, KANI_VERSION, MUTANTS_VERSION, container_image, container_run_arguments,
    container_target_volume, validate_results,
};

#[path = "support/incident_fixture.rs"]
mod incident_fixture;

const KANI_DRIVER_FIXTURE: &str = "DOTFILES_KANI_DRIVER_FIXTURE";
const IMAGE_PLATFORM_FIXTURE: &str = "DOTFILES_IMAGE_PLATFORM_FIXTURE";

fn run_kani_driver_fixture(test: &str) -> anyhow::Result<bool> {
    if std::env::var(KANI_DRIVER_FIXTURE).is_ok_and(|value| value == test) {
        anyhow::ensure!(
            platform::incidents::enabled(),
            "the owned Kani driver fixture requires explicit experimental consent"
        );
        return Ok(false);
    }
    let scope = incident_fixture::scope()?;
    let home = scope.path().join("home");
    let consent = home.join(platform::incidents::EXPERIMENTAL);
    std::fs::create_dir_all(consent.parent().unwrap())?;
    std::fs::write(consent, rules::incident_rules::EXPERIMENTAL_OPT_IN)?;
    anyhow::ensure!(
        platform::incidents::enabled_at(&home),
        "the owned Kani driver fixture consent is invalid"
    );
    let native_home = platform::home::home().context("native tool home")?;
    let driver = xtask::skill_proofs::kani_environment_driver()?;
    let kani_home = driver
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .context("existing Kani distribution root")?;
    let mut command = platform::process::command(std::env::current_exe()?);
    command
        .args(["--ignored", "--exact", test, "--nocapture"])
        .env(KANI_DRIVER_FIXTURE, test)
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("KANI_HOME", kani_home)
        .env("SKILL_OPS_STATE", scope.path().join("state"))
        .env("DOTFILES_EXPECTED_FAILURE", "");
    for (name, directory) in [("CARGO_HOME", ".cargo"), ("RUSTUP_HOME", ".rustup")] {
        command.env(
            name,
            std::env::var_os(name)
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| native_home.join(directory).into_os_string()),
        );
    }
    let output = command.output()?;
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    incident_fixture::preserve(scope);
    anyhow::ensure!(output.status.success(), "owned Kani driver fixture failed");
    Ok(true)
}

/// Every `*_rules.rs` file under `directory`, as a checkout-relative path with forward slashes.
fn rule_modules(root: &Path, directory: &Path, found: &mut BTreeSet<String>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path: PathBuf = entry.unwrap().path();
        if path.is_dir() {
            rule_modules(root, &path, found);
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("_rules.rs"))
        {
            found.insert(
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}

#[test]
fn every_rule_module_and_only_those_are_registered_for_proof() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut found = BTreeSet::new();
    rule_modules(root, &root.join("xtask/src"), &mut found);
    for entry in std::fs::read_dir(root.join("crates")).unwrap() {
        let source = entry.unwrap().path().join("src");
        if source.is_dir() {
            rule_modules(root, &source, &mut found);
        }
    }
    let registered: BTreeSet<String> = xtask::obligations::manifest(root)
        .unwrap()
        .obligations
        .iter()
        .filter_map(|obligation| match obligation {
            xtask::obligations::Obligation::Kani { source, .. } => Some(source.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(found, registered);
}

#[test]
fn miri_reports_each_test_binary_duration_in_milliseconds() {
    let report = "running 60 tests\ntest result: ok. 60 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 14.74s\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n";
    assert_eq!(
        xtask::skill_proofs::test_durations(report),
        [Some(14_740), Some(0)]
    );
    assert_eq!(
        xtask::skill_proofs::test_durations("test result: ok. finished in soon"),
        [None]
    );
    assert!(xtask::skill_proofs::MIRI_CRATES.contains(&"rules"));
}

fn miri_events(events: &[serde_json::Value]) -> String {
    events
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn miri_batches_keep_every_discovered_test_and_its_ignore_state() {
    const TESTS: usize = 75;
    let mut events = vec![json!({"type":"suite", "event":"discovery"})];
    let expected: BTreeSet<_> = (0..TESTS)
        .map(|index| (format!("unit::case_{index:03}"), index % 2 == 0))
        .collect();
    events.extend(expected.iter().map(|(name, ignored)| json!({"type":"test", "event":"discovered", "name":name, "ignore":ignored})));
    events.push(json!({"type":"suite", "event":"completed", "tests":TESTS, "benchmarks":0}));
    let batches = xtask::skill_proofs::miri_batches(&miri_events(&events)).unwrap();
    assert!(batches.len() > 1);
    let count: usize = batches.iter().map(Vec::len).sum();
    let actual: BTreeSet<_> = batches
        .into_iter()
        .flatten()
        .map(|test| (test.name, test.ignored))
        .collect();
    assert_eq!(count, TESTS);
    assert_eq!(actual, expected);
}

#[test]
fn miri_exhaustive_shards_run_separately_without_losing_other_tests_or_ignores() {
    let mut expected: BTreeSet<_> = (0..75)
        .map(|index| (format!("unit::case_{index:03}"), index % 2 == 0))
        .collect();
    let families = [
        "artifact_rules::tests::sha256_rejects_every_noncanonical_byte_at_every_position_",
        "artifact_rules::tests::device_prefix_membership_matches_the_reference_for_every_byte_at_every_position_",
        "artifact_rules::tests::portable_components_match_the_split_reference_for_all_device_spellings_",
    ];
    let mut isolated = BTreeSet::new();
    for family in families {
        for shard in 0..8 {
            let name = format!("{family}shard_{shard}");
            isolated.insert(name.clone());
            expected.insert((name, shard == 0));
        }
    }
    let mut events: Vec<_> = expected.iter().map(|(name, ignored)| json!({"type":"test", "event":"discovered", "name":name, "ignore":ignored})).collect();
    events
        .push(json!({"type":"suite", "event":"completed", "tests":expected.len(), "benchmarks":0}));
    let batches = xtask::skill_proofs::miri_batches(&miri_events(&events)).unwrap();
    for batch in &batches {
        assert!(!batch.is_empty() && batch.len() <= 32);
        if batch.iter().any(|test| isolated.contains(&test.name)) {
            assert_eq!(batch.len(), 1);
        }
    }
    let count: usize = batches.iter().map(Vec::len).sum();
    let actual: BTreeSet<_> = batches
        .into_iter()
        .flatten()
        .map(|test| (test.name, test.ignored))
        .collect();
    assert_eq!(count, expected.len());
    assert_eq!(actual, expected);
}

#[test]
fn miri_discovery_refuses_empty_incomplete_and_duplicate_inventories() {
    let test = json!({"type":"test", "event":"discovered", "name":"unit::case", "ignore":false});
    let completed = json!({"type":"suite", "event":"completed", "tests":1, "benchmarks":0});
    for events in [
        vec![],
        vec![test.clone()],
        vec![completed.clone()],
        vec![test.clone(), test.clone(), completed.clone()],
        vec![
            test.clone(),
            json!({"type":"suite", "event":"completed", "tests":2, "benchmarks":0}),
        ],
        vec![
            test.clone(),
            json!({"type":"suite", "event":"completed", "tests":1, "benchmarks":1}),
        ],
        vec![
            json!({"type":"test", "event":"discovered", "name":"unit::case"}),
            completed,
        ],
    ] {
        assert!(xtask::skill_proofs::miri_batches(&miri_events(&events)).is_err());
    }
    assert!(xtask::skill_proofs::miri_batches("not JSON").is_err());
}

#[test]
fn miri_batch_results_refuse_missing_changed_duplicate_and_failed_tests() {
    use xtask::skill_proofs::{MiriTest, validate_miri_batch};
    let expected = [MiriTest {
        name: "unit::case".to_owned(),
        ignored: false,
    }];
    let test = json!({"type":"test", "event":"ok", "name":"unit::case"});
    let suite = json!({"type":"suite", "event":"ok", "failed":0, "exec_time":30.0});
    validate_miri_batch(
        &miri_events(&[test.clone(), suite.clone()]),
        &expected,
        30_000,
    )
    .unwrap();
    for events in [
        vec![suite.clone()],
        vec![test.clone()],
        vec![test.clone(), test.clone(), suite.clone()],
        vec![
            json!({"type":"test", "event":"ignored", "name":"unit::case"}),
            suite.clone(),
        ],
        vec![
            json!({"type":"test", "event":"ok", "name":"unexpected"}),
            suite.clone(),
        ],
        vec![
            test.clone(),
            json!({"type":"suite", "event":"ok", "failed":1, "exec_time":1.0}),
        ],
        vec![
            test.clone(),
            json!({"type":"suite", "event":"ok", "failed":0}),
        ],
    ] {
        assert!(validate_miri_batch(&miri_events(&events), &expected, 1).is_err());
    }
    assert!(validate_miri_batch(&miri_events(&[test.clone(), suite.clone()]), &[], 1).is_err());
    assert!(
        validate_miri_batch(
            &miri_events(&[test, suite]),
            &[expected[0].clone(), expected[0].clone()],
            1,
        )
        .is_err()
    );
}

#[test]
fn miri_batch_durations_are_advisory_and_well_formed() {
    use xtask::skill_proofs::{MiriTest, validate_miri_batch};
    let expected = [MiriTest {
        name: "unit::case".to_owned(),
        ignored: false,
    }];
    let test = json!({"type":"test", "event":"ok", "name":"unit::case"});
    let report = |seconds: &str| {
        format!(
            "{test}\n{{\"type\":\"suite\",\"event\":\"ok\",\"passed\":1,\"failed\":0,\"ignored\":0,\"measured\":0,\"filtered_out\":0,\"exec_time\":{seconds}}}"
        )
    };
    for seconds in [
        "9.75e-6",
        "0",
        "0.0",
        "1e-9",
        "29.999999999",
        "30",
        "30.0",
        "3e1",
        "30000e-3",
        "399",
        "1e17",
        "18446744073709551615",
        "30.000000000000004",
    ] {
        validate_miri_batch(&report(seconds), &expected, 30_000)
            .unwrap_or_else(|error| panic!("duration {seconds}: {error}"));
    }
    for seconds in ["-1", "-9.75e-6", "-0.0", "null", "\"9.75e-6\"", "1e309"] {
        assert!(
            validate_miri_batch(&report(seconds), &expected, 1).is_err(),
            "duration {seconds} must be refused"
        );
    }
    let missing = format!("{test}\n{{\"type\":\"suite\",\"event\":\"ok\",\"failed\":0}}");
    assert!(validate_miri_batch(&missing, &expected, 1).is_err());
    validate_miri_batch(&report("399"), &expected, 29_999).unwrap();
    for elapsed_ms in [30_001, u64::MAX] {
        validate_miri_batch(&report("1e-9"), &expected, elapsed_ms).unwrap();
    }
}

#[test]
fn completed_proof_verdicts_ignore_elapsed_time_and_incomplete_proofs_lack_evidence() {
    let expected = ["bounded-contract"];
    let valid = json!({"verification_results":{"summary":{"status":"completed","executed":1,"failed":0,"successful":1},"results":[{"harness_id":expected[0],"status":"Success","checks":[{"category":"assertion","status":"Success"},{"category":"cover","status":"Satisfied"},{"category":"cover","status":"Satisfied"}]}]}});
    validate_results(&valid, &expected, false).unwrap();
    for elapsed_ms in [0, 30_000, 30_001, 60_000, u64::MAX] {
        let mut measured = valid.clone();
        measured["verification_results"]["results"][0]["duration_ms"] = json!(elapsed_ms);
        validate_results(&measured, &expected, false).unwrap();
    }
    for status in ["Failure", "Timeout", "Undetermined", "Skipped", "Unknown"] {
        let mut incomplete = valid.clone();
        incomplete["verification_results"]["results"][0]["status"] = json!(status);
        incomplete["verification_results"]["results"][0]["checks"] = json!([]);
        incomplete["verification_results"]["summary"]["failed"] = json!(1);
        incomplete["verification_results"]["summary"]["successful"] = json!(0);
        for counterexample in [false, true] {
            let error = validate_results(&incomplete, &expected, counterexample)
                .unwrap_err()
                .to_string();
            assert!(error.contains("proof did not complete"), "{error}");
            assert!(error.contains("inconclusive"), "{error}");
            assert!(!error.contains("required proof failed"), "{error}");
        }
    }
}

#[test]
fn a_counterexample_with_unfinished_properties_is_missing_evidence() {
    let expected = ["incomplete-counterexample"];
    let report = json!({"verification_results":{"summary":{"status":"completed","executed":1,"failed":1,"successful":0},"results":[{"harness_id":expected[0],"status":"Failure","checks":[{"category":"assertion","status":"Failure"},{"category":"unwinding assertion","status":"Unknown"}]}]}});
    let error = validate_results(&report, &expected, true)
        .unwrap_err()
        .to_string();
    assert!(error.contains("proof did not complete"), "{error}");
}

#[test]
fn completed_miri_verdicts_ignore_host_speed() {
    use xtask::skill_proofs::{MiriTest, validate_miri_batch};
    let expected = [MiriTest {
        name: "bounded-test".into(),
        ignored: false,
    }];
    let report = miri_events(&[
        json!({"type":"test","event":"timeout","name":"bounded-test"}),
        json!({"type":"test","event":"ok","name":"bounded-test"}),
        json!({"type":"suite","event":"ok","failed":0,"exec_time":0.001}),
    ]);
    for elapsed_ms in [0, 30_000, 30_001, 60_000, u64::MAX] {
        validate_miri_batch(&report, &expected, elapsed_ms).unwrap();
    }
    let incomplete = miri_events(&[
        json!({"type":"test","event":"timeout","name":"bounded-test"}),
        json!({"type":"suite","event":"ok","failed":0,"exec_time":0.001}),
    ]);
    assert!(validate_miri_batch(&incomplete, &expected, 60_000).is_err());
}

#[test]
fn proof_gate_rejects_empty_missing_vacuous_and_failed_results() -> anyhow::Result<()> {
    if incident_fixture::run("proof_gate_rejects_empty_missing_vacuous_and_failed_results")? {
        return Ok(());
    }
    let expected = ["actual-production-contract"];
    let valid = json!({"verification_results":{"summary":{"status":"completed","executed":1,"failed":0,"successful":1},"results":[{"harness_id":expected[0],"status":"Success","duration_ms":1200,"checks":[{"category":"assertion","status":"Success"},{"category":"cover","status":"Satisfied"},{"category":"cover","status":"Satisfied"}]}]}});
    let mut slow = valid.clone();
    slow["verification_results"]["results"][0]["duration_ms"] = json!(30_001);
    validate_results(&slow, &expected, false)?;
    let mut unmeasured = valid.clone();
    unmeasured["verification_results"]["results"][0]
        .as_object_mut()
        .unwrap()
        .remove("duration_ms");
    validate_results(&unmeasured, &expected, false)?;
    validate_results(&valid, &expected, false)?;
    let mut impossible = valid.clone();
    impossible["verification_results"]["results"][0]["checks"]
        .as_array_mut()
        .unwrap()
        .push(json!({"category":"overflow","status":"Unreachable"}));
    validate_results(&impossible, &expected, false)?;
    for status in ["Failure", "Undetermined", "Unknown", "Skipped"] {
        impossible["verification_results"]["results"][0]["checks"][3]["status"] = json!(status);
        assert!(validate_results(&impossible, &expected, false).is_err());
    }
    let mut unreachable = valid.clone();
    unreachable["verification_results"]["results"][0]["checks"][0]["status"] = json!("Unreachable");
    assert!(validate_results(&unreachable, &expected, false).is_err());
    unreachable = valid.clone();
    unreachable["verification_results"]["results"][0]["checks"][2]["status"] = json!("Unreachable");
    assert!(validate_results(&unreachable, &expected, false).is_err());
    assert!(validate_results(&valid, &[], false).is_err());
    let mut missing = valid.clone();
    missing["verification_results"]["results"] = json!([]);
    assert!(validate_results(&missing, &expected, false).is_err());
    let mut vacuous = valid.clone();
    vacuous["verification_results"]["results"][0]["checks"][2]["status"] = json!("Unsatisfied");
    assert!(validate_results(&vacuous, &expected, false).is_err());
    let mut failed = valid.clone();
    failed["verification_results"]["results"][0]["status"] = json!("Failure");
    assert!(validate_results(&failed, &expected, false).is_err());
    assert!(validate_results(&valid, &expected, true).is_err());
    Ok(())
}

#[test]
fn proof_results_collect_completed_harness_failures_independent_of_duration() -> anyhow::Result<()>
{
    if incident_fixture::run(
        "proof_results_collect_completed_harness_failures_independent_of_duration",
    )? {
        return Ok(());
    }
    let expected = ["slow", "broken"];
    let mut receipt = json!({"verification_results":{"summary":{"status":"completed","executed":2,"failed":0,"successful":2},"results":[]}});
    for name in expected {
        receipt["verification_results"]["results"]
            .as_array_mut()
            .unwrap()
            .push(json!({"harness_id":name,"status":"Success","duration_ms":1200,"checks":[{"category":"assertion","status":"Success"},{"category":"cover","status":"Satisfied"},{"category":"cover","status":"Satisfied"}]}));
    }
    validate_results(&receipt, &expected, false)?;
    let mut failed = receipt.clone();
    failed["verification_results"]["results"][0]["duration_ms"] = json!(30_001);
    failed["verification_results"]["summary"]["failed"] = json!(1);
    failed["verification_results"]["summary"]["successful"] = json!(1);
    failed["verification_results"]["results"][1]["status"] = json!("Failure");
    failed["verification_results"]["results"][1]["checks"][0]["status"] = json!("Failure");
    let failure = validate_results(&failed, &expected, false).unwrap_err();
    let diagnostic = format!("{failure:#}");
    assert!(!diagnostic.contains("slow took"), "{diagnostic}");
    assert!(diagnostic.contains("broken"), "{diagnostic}");
    assert!(diagnostic.contains("required proof failed"), "{diagnostic}");
    platform::report(&failure);
    validate_results(&receipt, &expected, false)?;
    Ok(())
}

#[test]
#[ignore = "the proofs gate runs this test with its pinned Kani installation"]
fn a_kani_batch_verifies_every_harness_and_rejects_every_counterexample() -> anyhow::Result<()> {
    if run_kani_driver_fixture(
        "a_kani_batch_verifies_every_harness_and_rejects_every_counterexample",
    )? {
        return Ok(());
    }
    let scope = tempfile::tempdir()?;
    let source = scope.path().join("proof.rs");
    let flags = BTreeSet::from(["stubbing"]);
    let names: Vec<_> = (0..=xtask::skill_proofs::KANI_BATCH_SIZE)
        .map(|index| format!("case_{index}"))
        .collect();
    let expected: Vec<_> = names.iter().map(String::as_str).collect();
    let mut duplicate = expected.clone();
    *duplicate.last_mut().unwrap() = expected[0];
    assert!(
        xtask::skill_proofs::verify_file(&source, scope.path(), &duplicate, false, &flags)
            .unwrap_err()
            .to_string()
            .contains("duplicate proof harness")
    );
    let mut valid = String::new();
    for name in &names {
        writeln!(
            valid,
            "#[kani::proof] fn {name}() {{ let value: bool = kani::any(); assert_eq!(value, !(!value)); kani::cover!(value); kani::cover!(!value); }}"
        )?;
    }
    verify_failed_kani_batches(&source, scope.path(), &expected, &flags, &valid)?;
    let mut missing = expected.clone();
    *missing.last_mut().unwrap() = "missing";
    assert!(
        xtask::skill_proofs::verify_file(&source, scope.path(), &missing, false, &flags).is_err()
    );
    std::fs::write(
        &source,
        valid.replace("assert_eq!(value, !(!value))", "assert!(value)"),
    )?;
    xtask::skill_proofs::verify_file(&source, scope.path(), &expected, true, &flags)?;
    std::fs::write(&source, "this source cannot compile")?;
    assert!(
        xtask::skill_proofs::verify_file(&source, scope.path(), &expected, true, &flags).is_err()
    );
    assert_eq!(
        std::fs::read_to_string(scope.path().join("results.json"))?,
        ""
    );
    let last = valid.lines().last().unwrap();
    std::fs::write(
        &source,
        format!(
            "{}\n{last}\n",
            valid
                .lines()
                .take(names.len() - 1)
                .collect::<Vec<_>>()
                .join("\n")
                .replace("assert_eq!(value, !(!value))", "assert!(value)")
        ),
    )?;
    assert!(
        xtask::skill_proofs::verify_file(&source, scope.path(), &expected, true, &flags).is_err()
    );
    assert!(
        xtask::skill_proofs::verify_file(&source, scope.path(), &expected, false, &flags).is_err()
    );
    Ok(())
}

fn verify_failed_kani_batches(
    source: &Path,
    directory: &Path,
    expected: &[&str],
    flags: &BTreeSet<&'static str>,
    valid: &str,
) -> anyhow::Result<()> {
    let last = expected.last().unwrap();
    let failed = valid
        .lines()
        .enumerate()
        .map(|(index, line)| {
            if index == 0 || index + 1 == expected.len() {
                line.replace("assert_eq!(value, !(!value))", "assert!(value)")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(source, failed)?;
    let failure =
        xtask::skill_proofs::verify_file(source, directory, expected, false, flags).unwrap_err();
    let diagnostic = format!("{failure:#}");
    for name in [expected[0], last] {
        assert!(
            diagnostic.contains(&format!("harness {name}:")),
            "{diagnostic}"
        );
    }
    let report: serde_json::Value = skill_ops::read_json(&directory.join("results.json"))?;
    assert_eq!(report["verification_results"]["summary"]["executed"], 1);
    assert_eq!(
        report["verification_results"]["results"][0]["harness_id"],
        *last
    );
    assert_eq!(
        report["verification_results"]["results"][0]["status"],
        "Failure"
    );
    platform::report(&failure);
    let state = std::env::var_os("SKILL_OPS_STATE").unwrap();
    let incidents = skill_ops::incidents(Path::new(&state))?;
    assert_eq!(incidents.len(), 1);
    assert_eq!(incidents[0].rule, "tool.failure");
    assert!(incidents[0].evidence.iter().any(|evidence| {
        evidence.contains(&format!("harness {}:", expected[0]))
            && evidence.contains(&format!("harness {last}:"))
    }));
    std::fs::write(source, valid)?;
    xtask::skill_proofs::verify_file(source, directory, expected, false, flags)?;
    assert_eq!(skill_ops::incidents(Path::new(&state))?, incidents);
    Ok(())
}

#[test]
fn the_overflow_fixture_requires_the_actual_checked_add_failure() -> anyhow::Result<()> {
    let name = "reject_an_overflow_under_checked_mir";
    let mut receipt = json!({"verification_results":{"summary":{"status":"completed","executed":1,"failed":1,"successful":0},"results":[{"harness_id":name,"status":"Failure","duration_ms":30,"checks":[{"category":"assertion","description":"attempt to add with overflow","status":"Failure"}]}]}});
    validate_results(&receipt, &[name], true)?;
    receipt["verification_results"]["results"][0]["checks"][0]["description"] =
        json!("an unrelated assertion failed");
    assert!(validate_results(&receipt, &[name], true).is_err());
    Ok(())
}

#[test]
fn the_image_is_named_by_its_definition_and_changes_with_it() {
    let image = container_image(b"FROM base\n");
    assert!(image.starts_with("dotfiles-proofs:"));
    assert_eq!(image, container_image(b"FROM base\n"));
    assert_ne!(image, container_image(b"FROM other\n"));
}

#[test]
fn the_image_identity_covers_pins_and_rejects_missing_or_drifting_pins() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let scope = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(scope.path().join("xtask/proofs")).unwrap();
    std::fs::create_dir_all(scope.path().join("home/.chezmoidata")).unwrap();
    for path in [
        "mise.toml",
        "mise.lock",
        "xtask/proofs/Dockerfile",
        "home/.chezmoidata/versions.toml",
    ] {
        std::fs::write(
            scope.path().join(path),
            std::fs::read(root.join(path)).unwrap(),
        )
        .unwrap();
    }
    let first = xtask::skill_proofs::proof_image(scope.path()).unwrap();
    let lock = std::fs::read_to_string(scope.path().join("mise.lock")).unwrap();
    let table: toml::Value = toml::from_str(&lock).unwrap();
    let digest =
        table["tools"]["powershell"].as_array().unwrap()[0]["platforms.linux-x64"]["checksum"]
            .as_str()
            .unwrap();
    let altered = lock.replace(digest, &digest.replace('d', "a"));
    assert_ne!(lock, altered);
    std::fs::write(scope.path().join("mise.lock"), altered).unwrap();
    assert_ne!(
        first,
        xtask::skill_proofs::proof_image(scope.path()).unwrap()
    );
    std::fs::write(scope.path().join("mise.lock"), lock).unwrap();
    std::fs::write(
        scope.path().join("mise.toml"),
        "[tools]\npowershell = '0'\n",
    )
    .unwrap();
    assert!(xtask::skill_proofs::proof_image(scope.path()).is_err());
    std::fs::write(scope.path().join("mise.lock"), "[tools]\n").unwrap();
    assert!(xtask::skill_proofs::proof_image(scope.path()).is_err());
}

#[test]
fn a_cached_image_is_reused_only_for_the_pinned_linux_x86_64_semantics() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    if let Some(directory) = std::env::var_os(IMAGE_PLATFORM_FIXTURE) {
        let executable =
            PathBuf::from(directory).join(format!("docker{}", std::env::consts::EXE_SUFFIX));
        let platform = executable.with_extension("platform");
        let built = executable.with_extension("built");
        for installed in [
            None,
            Some("linux/arm64"),
            Some("windows/amd64"),
            Some("linux/amd64"),
        ] {
            if let Some(installed) = installed {
                std::fs::write(&platform, installed)?;
            } else if platform.exists() {
                std::fs::remove_file(&platform)?;
            }
            if built.exists() {
                std::fs::remove_file(&built)?;
            }
            xtask::skill_proofs::ensure_image(
                root,
                &root.join(xtask::skill_proofs::PROOF_IMAGE),
                "dotfiles-proofs:fixture",
            )?;
            assert_eq!(std::fs::read_to_string(&platform)?, "linux/amd64");
            assert_eq!(built.exists(), installed != Some("linux/amd64"));
        }
        return Ok(());
    }
    let scope = incident_fixture::scope()?;
    let executable = scope
        .path()
        .join(format!("docker{}", std::env::consts::EXE_SUFFIX));
    let status = platform::process::command("rustc")
        .args(["--edition", "2024"])
        .arg(root.join("xtask/tests/fixtures/docker_image.rs"))
        .arg("-o")
        .arg(executable)
        .status()?;
    anyhow::ensure!(status.success(), "compile the Docker platform fixture");
    let mut path = vec![scope.path().to_path_buf()];
    path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let output = platform::process::command(std::env::current_exe()?)
        .args([
            "--exact",
            "a_cached_image_is_reused_only_for_the_pinned_linux_x86_64_semantics",
            "--nocapture",
        ])
        .env(IMAGE_PLATFORM_FIXTURE, scope.path())
        .env("PATH", std::env::join_paths(path)?)
        .env("SKILL_OPS_STATE", scope.path().join("state"))
        .env("DOTFILES_EXPECTED_FAILURE", "")
        .output()?;
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    anyhow::ensure!(
        output.status.success(),
        "owned Docker platform fixture failed"
    );
    incident_fixture::preserve(scope);
    Ok(())
}

#[test]
fn checkouts_never_share_a_build_cache() {
    let first = container_target_volume(Path::new(r"C:\Users\fixture\dotfiles"));
    assert_eq!(
        first,
        container_target_volume(Path::new(r"C:\Users\fixture\dotfiles"))
    );
    assert_ne!(
        first,
        container_target_volume(Path::new(r"C:\Users\fixture\worktree"))
    );
}

#[test]
fn the_container_reads_the_checkout_and_runs_the_unchanged_proof_gate() {
    let arguments = container_run_arguments(
        Path::new(r"C:\Users\fixture\dotfiles"),
        "dotfiles-proofs:fixture",
        "dotfiles-proofs-target-fixture",
    );
    let text: Vec<_> = arguments
        .iter()
        .map(|value| value.to_string_lossy())
        .collect();
    let mount = text
        .iter()
        .position(|value| value == "--mount")
        .expect("checkout mount");
    assert_eq!(
        text[mount + 1],
        r#"type=bind,"source=C:\Users\fixture\dotfiles",target=/source,readonly"#
    );
    let separated = container_run_arguments(
        Path::new(r"C:\Users\Jane, Doe\dotfiles"),
        "dotfiles-proofs:fixture",
        "dotfiles-proofs-target-fixture",
    );
    assert_eq!(
        separated[mount + 1],
        OsString::from(r#"type=bind,"source=C:\Users\Jane, Doe\dotfiles",target=/source,readonly"#)
    );
    assert!(text.contains(&"--rm".into()));
    let platform = text
        .iter()
        .position(|value| value == "--platform")
        .expect("platform of the pinned x86_64 tools");
    assert_eq!(text[platform + 1], "linux/amd64");
    assert!(text.contains(&"dotfiles-proofs-target-fixture:/cache".into()));
    assert!(
        text.windows(2)
            .any(|pair| pair[0] == "--env" && pair[1] == "DOTFILES_HOST_CACHE=/cache")
    );
    let image = text
        .iter()
        .position(|value| value == "dotfiles-proofs:fixture")
        .expect("pinned image");
    assert_eq!(
        arguments[image + 1..image + 3],
        ["bash", "-ec"].map(OsString::from)
    );
    let gate = text[image + 3].as_ref();
    assert!(gate.starts_with("set -o pipefail; tar -C /source "));
    assert!(gate.ends_with(
        "| tar -C /work -xf -; exec cargo run --locked --manifest-path xtask/Cargo.toml -- proofs"
    ));
    assert_eq!(arguments.len(), image + 4);
}

#[test]
fn the_container_verifier_matches_the_native_pin() {
    let definition = include_str!("../proofs/Dockerfile");
    let mise = include_str!("../../mise.toml");
    assert!(definition.contains(&format!("kani-verifier --version {KANI_VERSION}")));
    assert!(mise.contains(&format!(
        "\"cargo:kani-verifier\" = {{ version = \"{KANI_VERSION}\""
    )));
    assert!(definition.contains(&format!("cargo-mutants --version {MUTANTS_VERSION}")));
    assert!(mise.contains(&format!(
        "\"cargo:cargo-mutants\" = {{ version = \"{MUTANTS_VERSION}\""
    )));
    assert!(definition.contains(&format!("cargo-fuzz --version {FUZZ_VERSION}")));
    assert!(mise.contains(&format!(
        "\"cargo:cargo-fuzz\" = {{ version = \"{FUZZ_VERSION}\""
    )));
    let rust = mise
        .lines()
        .find_map(|line| line.strip_prefix("rust = { version = \""))
        .and_then(|rest| rest.split('"').next())
        .expect("pinned rust");
    assert!(definition.contains(&format!("rust:{rust}-")));
}

#[test]
fn the_quint_image_pins_quint_its_apalache_and_every_base_by_digest() {
    use xtask::quint::{APALACHE_VERSION, QUINT_VERSION};
    let definition = include_str!("../proofs/quint/Dockerfile");
    let package = include_str!("../proofs/quint/package.json");
    let lock: serde_json::Value =
        serde_json::from_str(include_str!("../proofs/quint/package-lock.json")).unwrap();
    assert!(package.contains(&format!("\"@informalsystems/quint\": \"{QUINT_VERSION}\"")));
    assert_eq!(
        lock["packages"]["node_modules/@informalsystems/quint"]["version"],
        QUINT_VERSION
    );
    assert!(lock["packages"]["node_modules/@informalsystems/quint"]["integrity"].is_string());
    assert!(definition.contains(&format!("ARG APALACHE_VERSION={APALACHE_VERSION}\n")));
    assert!(definition.contains("ADD --checksum=sha256:${APALACHE_SHA256}"));
    assert!(definition.contains("sha256sum.txt"));
    assert!(definition.contains(&format!("ENV QUINT_HOME={}\n", xtask::quint::QUINT_HOME)));
    assert!(definition.contains("${QUINT_HOME}/apalache-dist-${APALACHE_VERSION}/apalache"));
    for line in definition.lines().filter(|line| line.starts_with("FROM ")) {
        assert!(line.contains("@sha256:"), "{line}");
    }
}

#[test]
fn quint_runs_offline_with_the_checkout_read_only() {
    let arguments = xtask::quint::run_arguments(
        Path::new(r"C:\Users\fixture\dotfiles"),
        Path::new(r"C:\Temp\out"),
        "dotfiles-quint:fixture",
        &["typecheck", "/source/specs/pr_ready.qnt"],
    );
    let text: Vec<_> = arguments
        .iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect();
    let network = text.iter().position(|value| value == "--network").unwrap();
    assert_eq!(text[network + 1], "none");
    assert!(text.contains(
        &r#"type=bind,"source=C:\Users\fixture\dotfiles",target=/source,readonly"#.to_owned()
    ));
    assert!(text.contains(&r#"type=bind,"source=C:\Temp\out",target=/out"#.to_owned()));
    assert_eq!(
        text[text.len() - 3..],
        ["quint", "typecheck", "/source/specs/pr_ready.qnt"].map(str::to_owned)
    );
}

#[test]
fn the_alloy_image_pins_alloy_and_its_base_by_digest() {
    let definition = include_str!("../proofs/alloy/Dockerfile");
    assert!(definition.contains(&format!(
        "ARG ALLOY_VERSION={}\n",
        xtask::alloy::ALLOY_VERSION
    )));
    assert!(definition.contains("ADD --checksum=sha256:${ALLOY_SHA256}"));
    for line in definition.lines().filter(|line| line.starts_with("FROM ")) {
        assert!(line.contains("@sha256:"), "{line}");
    }
}

const CORRUPTED_ASSET: &str = "corrupt-verifier-asset";
const MODEL_PLATFORM: &str = "linux/amd64";

fn corrupted_image_context(root: &Path, asset: CorruptAsset) -> anyhow::Result<tempfile::TempDir> {
    let scope = incident_fixture::scope()?;
    for entry in std::fs::read_dir(root.join(asset.directory))? {
        let entry = entry?;
        anyhow::ensure!(
            entry.file_type()?.is_file(),
            "a model image context contains a non-file: {}",
            entry.path().display()
        );
        std::fs::write(
            scope.path().join(entry.file_name()),
            std::fs::read(entry.path())?,
        )?;
    }
    anyhow::ensure!(
        !scope.path().join(CORRUPTED_ASSET).exists(),
        "the model image context collides with its corruption fixture"
    );
    std::fs::write(
        scope.path().join(CORRUPTED_ASSET),
        b"corrupted verifier input\n",
    )?;
    let path = scope.path().join("Dockerfile");
    let definition = std::fs::read_to_string(&path)?;
    let target = format!(" {}", asset.target);
    let mut replaced = 0;
    let definition = definition
        .lines()
        .map(|line| {
            if line.starts_with("ADD --checksum=sha256:") && line.ends_with(&target) {
                replaced += 1;
                format!("COPY {CORRUPTED_ASSET} {}", asset.target)
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    anyhow::ensure!(
        replaced == 1,
        "the model image has {replaced} pinned inputs for {}",
        asset.target
    );
    std::fs::write(path, format!("{definition}\n"))?;
    Ok(scope)
}

#[test]
fn corruption_probes_preserve_other_inputs_and_cover_every_model_asset() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut covered = BTreeSet::new();
    for asset in CORRUPT_ASSETS {
        assert!(covered.insert((asset.directory, asset.target)));
        let scope = corrupted_image_context(root, asset)?;
        let original = std::fs::read_to_string(root.join(asset.directory).join("Dockerfile"))?;
        let altered = std::fs::read_to_string(scope.path().join("Dockerfile"))?;
        let differences: Vec<_> = original
            .lines()
            .zip(altered.lines())
            .filter(|(original, altered)| original != altered)
            .collect();
        assert_eq!(original.lines().count(), altered.lines().count());
        assert_eq!(differences.len(), 1);
        assert!(differences[0].0.starts_with("ADD --checksum=sha256:"));
        assert_eq!(
            differences[0].1,
            format!("COPY {CORRUPTED_ASSET} {}", asset.target)
        );
        for entry in std::fs::read_dir(root.join(asset.directory))? {
            let entry = entry?;
            if entry.file_name() != "Dockerfile" {
                assert_eq!(
                    std::fs::read(entry.path())?,
                    std::fs::read(scope.path().join(entry.file_name()))?
                );
            }
        }
        platform::cleanup::report(scope.close(), "remove the model corruption fixture");
    }
    let mut declared = BTreeSet::new();
    for directory in [
        xtask::alloy::IMAGE_DIRECTORY,
        xtask::quint::IMAGE_DIRECTORY,
        xtask::lean::IMAGE_DIRECTORY,
    ] {
        let definition = std::fs::read_to_string(root.join(directory).join("Dockerfile"))?;
        for line in definition
            .lines()
            .filter(|line| line.starts_with("ADD --checksum=sha256:"))
        {
            let target = line.split_whitespace().last().unwrap();
            declared.insert((directory.to_owned(), target.to_owned()));
        }
    }
    assert_eq!(
        covered
            .into_iter()
            .map(|(directory, target)| (directory.to_owned(), target.to_owned()))
            .collect::<BTreeSet<_>>(),
        declared
    );
    Ok(())
}

#[test]
#[ignore = "runs through the authoritative model gate with Docker"]
fn proof_images_reject_corrupt_verifier_assets_before_use() -> anyhow::Result<()> {
    corruption_probes_preserve_other_inputs_and_cover_every_model_asset()?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut checks = xtask::checks::Checks::default();
    let mut reports = Vec::new();
    for asset in CORRUPT_ASSETS {
        checks.run(asset.target, || {
            let scope = corrupted_image_context(root, asset)?;
            let result = (|| {
                let mut command = platform::tool::Tool::Docker.command();
                command.arg("build");
                if asset.directory == xtask::lean::IMAGE_DIRECTORY {
                    command.args(["--platform", MODEL_PLATFORM]);
                }
                let output = command.arg(scope.path()).output()?;
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                print!("{stdout}");
                eprint!("{stderr}");
                let captured = AssetControlResult::capture(asset, output)?;
                let result = captured.require_rejection();
                reports.push(captured);
                result
            })();
            platform::cleanup::report(scope.close(), "remove the model corruption fixture");
            result
        });
    }
    checks.run("retain original asset control results", || {
        xtask::alloy::write_asset_results(&reports)
    });
    checks.finish()
}

#[test]
fn alloy_runs_one_command_offline_with_the_checkout_read_only() {
    let arguments = xtask::alloy::run_arguments(
        Path::new(r"C:\Users\fixture\dotfiles"),
        Path::new(r"C:\Temp\out"),
        "dotfiles-alloy:fixture",
        "specs/transaction.als",
        "sat4j",
        "Durable",
    );
    let text: Vec<_> = arguments
        .iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect();
    let network = text.iter().position(|value| value == "--network").unwrap();
    assert_eq!(text[network + 1], "none");
    assert!(text.contains(
        &r#"type=bind,"source=C:\Users\fixture\dotfiles",target=/source,readonly"#.to_owned()
    ));
    let solver = text.iter().position(|value| value == "--solver").unwrap();
    assert_eq!(text[solver + 1], "sat4j");
    let command = text.iter().position(|value| value == "--command").unwrap();
    assert_eq!(text[command + 1], "Durable");
    assert_eq!(
        text.last().map(String::as_str),
        Some("/source/specs/transaction.als")
    );
}

#[test]
#[ignore = "requires Docker on a native host"]
fn proof_containers_leave_native_owned_removable_outputs() -> anyhow::Result<()> {
    const MODEL: &str = "/source/specs/pr_ready.qnt";
    const TRACE: &str = "/out/owner.{seq}.itf.json";
    const SEED: &str = "0x2a";
    const ONE: &str = "1";
    let root = xtask::canonical(Path::new(env!("CARGO_MANIFEST_DIR")).join("..").as_path())?;
    let alloy_image = xtask::alloy::image(&root)?;
    xtask::alloy::build(&root, &alloy_image)?;
    let quint_image = xtask::quint::image(&root)?;
    xtask::quint::build(&root, &quint_image)?;
    let alloy = tempfile::tempdir()?;
    let quint = tempfile::tempdir()?;
    let commands = [
        xtask::alloy::run_arguments(
            &root,
            alloy.path(),
            &alloy_image,
            "specs/manifest.als",
            "sat4j",
            "Disjoint",
        ),
        xtask::quint::run_arguments(
            &root,
            quint.path(),
            &quint_image,
            &[
                "run",
                MODEL,
                "--backend",
                "typescript",
                "--main",
                "pr_ready",
                "--seed",
                SEED,
                "--max-samples",
                ONE,
                "--n-traces",
                ONE,
                "--max-steps",
                ONE,
                "--mbt",
                "--out-itf",
                TRACE,
            ],
        ),
    ];
    for arguments in commands {
        let output = platform::tool::Tool::Docker
            .command()
            .args(arguments)
            .output()?;
        anyhow::ensure!(
            output.status.success(),
            "proof output producer failed: {}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let receipt: serde_json::Value =
        serde_json::from_slice(&std::fs::read(alloy.path().join("Disjoint/receipt.json"))?)?;
    assert!(!xtask::alloy::solved(&receipt, "Disjoint")?);
    let trace: serde_json::Value =
        serde_json::from_slice(&std::fs::read(quint.path().join("owner.0.itf.json"))?)?;
    assert!(
        trace["states"]
            .as_array()
            .is_some_and(|states| !states.is_empty())
    );
    for output in [alloy, quint] {
        assert_native_output_ownership(output.path())?;
        let path = output.path().to_path_buf();
        output.close()?;
        assert!(!path.exists());
    }
    Ok(())
}

fn assert_native_output_ownership(path: &Path) -> anyhow::Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    assert!(!metadata.file_type().is_symlink(), "{}", path.display());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            metadata.uid(),
            rustix::process::geteuid().as_raw(),
            "{}",
            path.display()
        );
        assert_eq!(
            metadata.gid(),
            rustix::process::getegid().as_raw(),
            "{}",
            path.display()
        );
    }
    if metadata.is_dir() {
        for entry in std::fs::read_dir(path)? {
            assert_native_output_ownership(&entry?.path())?;
        }
    }
    Ok(())
}

#[test]
fn an_alloy_receipt_with_a_solution_reports_an_instance_or_a_counterexample() {
    let solved: serde_json::Value = serde_json::from_str(
        r#"{"commands":{"Bad":{"type":"check","solution":[{"instances":[]}]}}}"#,
    )
    .unwrap();
    let unsolved: serde_json::Value =
        serde_json::from_str(r#"{"commands":{"Good":{"type":"check"}}}"#).unwrap();
    assert!(xtask::alloy::solved(&solved, "Bad").unwrap());
    assert!(!xtask::alloy::solved(&unsolved, "Good").unwrap());
    assert!(xtask::alloy::solved(&unsolved, "Missing").is_err());
    for malformed in [
        json!(null),
        json!({}),
        json!({"type":"check", "solution":false}),
    ] {
        let receipt = json!({"commands":{"Bad":malformed}});
        assert!(xtask::alloy::solved(&receipt, "Bad").is_err());
    }
}

#[test]
fn the_quint_reports_yield_the_state_count_and_each_witness() {
    let tlc = "Model checking completed. No error has been found.\n1928 states generated, 971 distinct states found, 0 states left on queue.\n";
    assert_eq!(xtask::quint::distinct_states(tlc), Some(971));
    assert_eq!(
        xtask::quint::distinct_states(
            "67 states generated, 61 distinct states found, 43 states left on queue."
        ),
        None
    );
    let run = "keptReady was witnessed in 70 trace(s) out of 1000 explored (7.00%)\nrefused was witnessed in 0 trace(s) out of 1000 explored (0.00%)\n";
    assert_eq!(xtask::quint::witnessed(run, "keptReady"), Some(70));
    assert_eq!(xtask::quint::witnessed(run, "refused"), Some(0));
    assert_eq!(xtask::quint::witnessed(run, "returnedToDraft"), None);
}
