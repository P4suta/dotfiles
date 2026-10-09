use serde_json::json;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use xtask::skill_proofs::{
    FUZZ_VERSION, KANI_VERSION, MUTANTS_VERSION, container_image, container_run_arguments,
    container_target_volume, validate_results,
};

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

#[test]
fn proof_gate_rejects_empty_missing_vacuous_failed_and_slow_results() -> anyhow::Result<()> {
    let expected = ["actual-production-contract"];
    let valid = json!({"verification_results":{"summary":{"status":"completed","executed":1,"failed":0,"successful":1},"results":[{"harness_id":expected[0],"status":"Success","duration_ms":1200,"checks":[{"category":"assertion","status":"Success"},{"category":"cover","status":"Satisfied"},{"category":"cover","status":"Satisfied"}]}]}});
    let mut slow = valid.clone();
    slow["verification_results"]["results"][0]["duration_ms"] = json!(30_001);
    let error = validate_results(&slow, &expected, false)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("actual-production-contract took 30001 ms"),
        "{error}"
    );
    let mut unmeasured = valid.clone();
    unmeasured["verification_results"]["results"][0]
        .as_object_mut()
        .unwrap()
        .remove("duration_ms");
    assert!(validate_results(&unmeasured, &expected, false).is_err());
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
    assert!(text.contains(&"dotfiles-proofs-target-fixture:/cache".into()));
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
