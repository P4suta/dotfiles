use serde_json::json;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use xtask::skill_proofs::{
    KANI_VERSION, RULES, container_image, container_run_arguments, container_target_volume,
    validate_results,
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
    let registered: BTreeSet<String> = RULES.iter().map(|rules| rules.source.to_owned()).collect();
    assert_eq!(found, registered);
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
    let rust = mise
        .lines()
        .find_map(|line| line.strip_prefix("rust = { version = \""))
        .and_then(|rest| rest.split('"').next())
        .expect("pinned rust");
    assert!(definition.contains(&format!("rust:{rust}-")));
}
