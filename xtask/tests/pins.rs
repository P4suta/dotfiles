//! Every file that names a pinned toolchain agrees with the pin, so an update that moves only the pin fails here.
//! The hosted dependency gate runs these tests on each pull request that skipped the local hooks.

const MISE: &str = include_str!("../../mise.toml");
const VERSIONS: &str = include_str!("../../home/.chezmoidata/versions.toml");
const MATERIALS: &str = include_str!("../../policy/native-materials.toml");
const WORKFLOW: &str = include_str!("../../.github/workflows/ci.yml");

fn table(text: &str) -> toml::Table {
    toml::from_str(text).unwrap()
}

fn rust() -> String {
    table(MISE)["tools"]["rust"]["version"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn every_file_that_names_the_rust_toolchain_names_the_pinned_release() {
    let rust = rust();
    let stable = &table(MATERIALS)["stable"];
    assert_eq!(stable["channel"].as_str(), Some(rust.as_str()));
    assert!(
        stable["url"]
            .as_str()
            .unwrap()
            .ends_with(&format!("/channel-rust-{rust}.toml"))
    );
    for (path, text, expected) in [
        (
            "xtask/proofs/Dockerfile",
            include_str!("../proofs/Dockerfile"),
            format!("rust:{rust}-"),
        ),
        (
            "xtask/src/benchmark_profile.rs",
            include_str!("../src/benchmark_profile.rs"),
            format!("\"release: {rust}\""),
        ),
        (
            "crates/dotfiles/src/artifacts/rustup_consume.rs",
            include_str!("../../crates/dotfiles/src/artifacts/rustup_consume.rs"),
            format!("RUST_VERSION: &str = \"{rust}\""),
        ),
        (
            "crates/dotfiles/tests/rustup_material_consumption.rs",
            include_str!("../../crates/dotfiles/tests/rustup_material_consumption.rs"),
            format!("version='{rust}'"),
        ),
    ] {
        assert!(text.contains(&expected), "{path} doesn't name {expected}");
    }
}

#[test]
fn the_hosted_gate_installs_the_pinned_mise_release() {
    let mise = &table(VERSIONS)["mise"];
    let version = mise["version"].as_str().unwrap();
    let digest = mise["sha256"]["linux-x64"].as_str().unwrap();
    assert!(WORKFLOW.contains(&format!("          version: {version}\n")));
    assert!(WORKFLOW.contains(&format!("          sha256: {digest}\n")));
}
