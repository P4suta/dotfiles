use std::collections::BTreeSet;
use xtask::obligations::parse_manifest;
use xtask::verification_env::{Backend, parse_image, required};

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
