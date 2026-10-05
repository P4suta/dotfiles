use dotfiles_xtask::skill_proofs::{
    KANI_VERSION, container_image, container_run_arguments, container_target_volume,
};
use std::ffi::OsString;
use std::path::Path;

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
        r"type=bind,source=C:\Users\fixture\dotfiles,target=/source,readonly"
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
