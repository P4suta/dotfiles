use std::collections::BTreeSet;
use xtask::obligations::parse_manifest;
use xtask::verification_env::{Backend, parse_image, required};

#[path = "../../crates/platform/tests/support/fixture_process.rs"]
mod fixture_process;

#[test]
fn environment_selection_follows_execution_obligations_and_shares_the_temporal_backend() {
    let manifest = parse_manifest(
        r#"
[[obligation]]
verifier = "kani"
source = "crates/rules/src/a_rules.rs"
[[obligation]]
verifier = "kani-contract"
source = "crates/rules/src/a_rules.rs"
function = "checked"
flags = ["function-contracts"]
[[obligation]]
verifier = "lean"
package = "specs/lean"
theorems = []
[[obligation]]
verifier = "alloy"
spec = "specs/a.als"
solver = "sat4j"
checks = []
"#,
    )
    .unwrap();
    assert_eq!(
        required(&manifest),
        BTreeSet::from([Backend::Kani, Backend::Lean, Backend::Alloy])
    );
    let contract_only = parse_manifest(
        r#"
[[obligation]]
verifier = "kani-contract"
source = "crates/rules/src/a_rules.rs"
function = "checked"
flags = ["function-contracts"]
"#,
    )
    .unwrap();
    assert_eq!(required(&contract_only), BTreeSet::new());
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    assert_eq!(
        required(&xtask::obligations::manifest(root).unwrap()),
        BTreeSet::from([Backend::Kani, Backend::Lean, Backend::Alloy, Backend::Quint])
    );
}

#[test]
fn a_cached_image_must_have_a_complete_content_identity_at_the_required_platform() {
    let complete = serde_json::json!({"Id": format!("sha256:{}", "a".repeat(64)), "Os": "linux", "Architecture": "arm64"});
    let actual = parse_image(&complete.to_string(), "linux/arm64").unwrap();
    assert_eq!(actual.architecture, "arm64");
    for invalid in [
        serde_json::json!({"Id": "mutable:latest", "Os": "linux", "Architecture": "arm64"}),
        serde_json::json!({"Id": "sha256:aaa", "Os": "linux", "Architecture": "arm64"}),
        serde_json::json!({"Id": format!("sha256:{}", "z".repeat(64)), "Os": "linux", "Architecture": "arm64"}),
        serde_json::json!({"Id": complete["Id"], "Os": "windows", "Architecture": "arm64"}),
        serde_json::json!({"Id": complete["Id"], "Os": "linux", "Architecture": "amd64"}),
        serde_json::json!({"Os": "linux", "Architecture": "arm64"}),
    ] {
        assert!(
            parse_image(&invalid.to_string(), "linux/arm64").is_err(),
            "{invalid}"
        );
    }
    assert!(parse_image("not JSON", "linux/arm64").is_err());
    assert!(parse_image(&complete.to_string(), "linux/amd64").is_err());
}

#[test]
#[ignore = "requires the prepared pinned Linux Kani image and bounded Docker runtime"]
fn prepared_container_doctor_checks_the_standalone_kani_driver() {
    let scope = tempfile::tempdir().expect("fixture directory");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let output = fixture_process::command(env!("CARGO_BIN_EXE_xtask"), scope.path())
        .current_dir(root)
        .args(["verification-env", "--backend", "kani", "doctor"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let environments = report["environments"].as_array().unwrap();
    assert_eq!(environments.len(), 1);
    let environment = &environments[0];
    assert_eq!(environment["backend"], "kani");
    assert_eq!(environment["image"]["os"], "linux");
    assert_eq!(environment["image"]["architecture"], "amd64");
    let components = environment["components"].as_array().unwrap();
    assert_eq!(components.len(), 1);
    assert!(xtask::skill_proofs::kani_environment_version_matches(
        components[0]["actual"].as_str().unwrap()
    ));
    assert!(components[0]["failure"].is_null());
    assert!(environment["failure"].is_null());
}
