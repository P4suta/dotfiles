use platform::runtime::{Real, checkout_key, claim_hook_target, hook_checkout, hook_target};
use platform::target_rules::{Target, target};
use platform::tool::Tool;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

#[test]
fn an_absolute_configured_target_wins_over_the_shared_directories() {
    assert_eq!(target(true, true), Target::Configured);
    assert_eq!(target(true, false), Target::Configured);
}

#[test]
fn an_unconfigured_or_relative_target_is_shared() {
    assert_eq!(target(false, true), Target::Development);
    assert_eq!(target(false, false), Target::Home);
}

#[cfg(windows)]
#[test]
fn windows_keeps_one_target_per_checkout_on_the_volume_that_holds_temp() {
    let home = Path::new(r"C:\Users\owner");
    let checkout = Path::new(r"D:\projects\dotfiles");
    let expected = PathBuf::from(r"D:\")
        .join("cargo/dotfiles-targets")
        .join(checkout_key(checkout));
    assert_eq!(
        hook_target(None, Some(Path::new(r"D:\tmp")), home, checkout),
        expected
    );
    assert_eq!(
        hook_target(
            Some(Path::new("target")),
            Some(Path::new(r"D:\tmp")),
            home,
            checkout
        ),
        expected
    );
    assert_eq!(
        hook_target(
            Some(Path::new(r"E:\cache")),
            Some(Path::new(r"D:\tmp")),
            home,
            checkout
        ),
        PathBuf::from(r"E:\cache")
    );
    assert_eq!(
        hook_target(None, None, home, checkout),
        home.join(".cache/dotfiles-targets")
            .join(checkout_key(checkout))
    );
}

#[cfg(not(windows))]
#[test]
fn other_hosts_keep_one_target_per_checkout_in_the_home_directory() {
    let home = Path::new("/home/owner");
    let checkout = Path::new("/home/owner/projects/dotfiles");
    let expected = home
        .join(".cache/dotfiles-targets")
        .join(checkout_key(checkout));
    assert_eq!(
        hook_target(None, Some(Path::new("/tmp")), home, checkout),
        expected
    );
    assert_eq!(
        hook_target(Some(Path::new("target")), None, home, checkout),
        expected
    );
    assert_eq!(
        hook_target(Some(Path::new("/cache")), None, home, checkout),
        PathBuf::from("/cache")
    );
}

#[test]
fn two_worktrees_never_share_a_hook_target() {
    let home = Path::new("/home/owner");
    let first = Path::new("/home/owner/projects/dotfiles");
    let second = Path::new("/home/owner/projects/dotfiles-f6");
    let renamed = Path::new("/srv/dotfiles");
    assert_ne!(
        hook_target(None, None, home, first),
        hook_target(None, None, home, second)
    );
    assert_ne!(checkout_key(first), checkout_key(renamed));
    assert_eq!(checkout_key(first), checkout_key(first));
    assert!(checkout_key(second).starts_with("dotfiles-f6-"));
    assert!(
        checkout_key(Path::new("/x/a b:c"))
            .chars()
            .all(|character| character.is_ascii_alphanumeric()
                || matches!(character, '-' | '_' | '.'))
    );
}

#[test]
fn a_hook_runs_in_the_checkout_that_holds_its_directory() -> std::io::Result<()> {
    let scope = tempfile::tempdir()?;
    let checkout = scope.path().join("repository");
    std::fs::create_dir_all(checkout.join(".git"))?;
    std::fs::create_dir_all(checkout.join("crates/nested"))?;
    let canonical = platform::paths::canonical(&checkout)?;
    assert_eq!(hook_checkout(&checkout.join("crates/nested")), canonical);
    assert_eq!(hook_checkout(&checkout), canonical);
    Ok(())
}

#[test]
fn a_hook_target_carries_a_storage_scout_cache_marker_for_its_checkout() -> anyhow::Result<()> {
    let scope = tempfile::tempdir()?;
    let checkout = scope.path().join("repository");
    let target = scope.path().join("targets/repository-0123");
    claim_hook_target(&target, &checkout)?;
    claim_hook_target(&target, &checkout)?;
    let marker: serde_json::Value =
        serde_json::from_slice(&std::fs::read(target.join("owner.json"))?)?;
    assert!(
        marker["schema"]
            .as_str()
            .is_some_and(|schema| schema.ends_with("-temp-owner-v1"))
    );
    assert_eq!(marker["role"], "cache");
    assert_eq!(marker["kept"], false);
    assert_eq!(
        marker["keyed_to"].as_str().map(PathBuf::from),
        Some(checkout)
    );
    assert!(target.join("owner.lock").is_file());
    Ok(())
}

#[test]
fn hook_gates_build_outside_the_worktree() -> std::io::Result<()> {
    let home = tempfile::tempdir()?;
    let native = Real::new(home.path()).expect("native");
    let command = native.hook_command(Tool::Lefthook);
    let value = command
        .get_envs()
        .find(|(name, _)| *name == OsStr::new("CARGO_TARGET_DIR"))
        .and_then(|(_, value)| value)
        .expect("CARGO_TARGET_DIR is set");
    assert!(Path::new(value).is_absolute());
    Ok(())
}
