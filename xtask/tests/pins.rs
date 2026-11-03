//! Every file that names a pinned toolchain agrees with the pin, so an update that moves only the pin fails here.
//! The hosted dependency gate runs these tests on each pull request that skipped the local hooks.

const MISE: &str = include_str!("../../mise.toml");
const VERSIONS: &str = include_str!("../../home/.chezmoidata/versions.toml");
const MATERIALS: &str = include_str!("../../policy/native-materials.toml");
const WORKFLOW: &str = include_str!("../../.github/workflows/ci.yml");
const MANIFESTS: &str = include_str!("../../.github/workflows/manifests.yml");

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
fn every_skill_lists_the_releases_that_the_hosts_install() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    if let Err(error) = xtask::inventory::check(&root) {
        panic!("{error:#}");
    }
}

#[test]
fn the_hosted_gate_installs_the_pinned_mise_release() {
    let mise = &table(VERSIONS)["mise"];
    let version = mise["version"].as_str().unwrap();
    let digest = mise["sha256"]["linux-x64"].as_str().unwrap();
    assert!(WORKFLOW.contains(&format!("          version: v{version}\n")));
    assert!(WORKFLOW.contains(&format!("          sha256: {digest}\n")));
}

#[test]
fn every_hosted_runner_installs_the_mise_release_of_its_platform() {
    let mise = &table(VERSIONS)["mise"];
    let version = mise["version"].as_str().unwrap();
    let digest = |platform: &str| mise["sha256"][platform].as_str().unwrap().to_owned();
    // On Windows, mise-action checks the extracted mise.exe, whose digest versions.toml doesn't record.
    // The Windows job names that digest as one more full digest, and a dispatch refuses a stale one.
    for (path, workflow, platforms, binaries) in [
        (".github/workflows/ci.yml", WORKFLOW, &["linux-x64"][..], 0),
        (
            ".github/workflows/manifests.yml",
            MANIFESTS,
            &["linux-x64"][..],
            1,
        ),
    ] {
        assert!(
            workflow.contains(&format!("          version: v{version}\n")),
            "{path} installs another mise release"
        );
        let named: Vec<&str> = workflow
            .lines()
            .filter_map(|line| line.trim().split_once("sha256: ").map(|(_, value)| value))
            .filter(|value| !value.starts_with("${{"))
            .collect();
        let expected: Vec<String> = platforms.iter().map(|platform| digest(platform)).collect();
        assert_eq!(
            named.len(),
            platforms.len() + binaries,
            "{path} names other mise digests"
        );
        assert_eq!(
            named[..platforms.len()],
            expected,
            "{path} names other mise digests"
        );
        for binary in &named[platforms.len()..] {
            assert!(
                binary.len() == 64
                    && binary
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
                "{path} names a malformed binary digest {binary}"
            );
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.github/workflows");
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            !text.contains("jdx/mise-action@")
                || ["ci.yml", "manifests.yml"].contains(&name.as_str()),
            "{name} installs mise without a digest check here"
        );
    }
}
