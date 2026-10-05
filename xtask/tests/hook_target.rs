use dotfiles_xtask::runtime::{Native, hook_target};
use dotfiles_xtask::target_rules::{Target, target};
use dotfiles_xtask::tool::Tool;
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
fn windows_shares_one_target_on_the_volume_that_holds_temp() {
    let home = Path::new(r"C:\Users\owner");
    let expected = PathBuf::from(r"D:\").join("cargo/target");
    assert_eq!(
        hook_target(None, Some(Path::new(r"D:\tmp")), home),
        expected
    );
    assert_eq!(
        hook_target(Some(Path::new("target")), Some(Path::new(r"D:\tmp")), home),
        expected
    );
    assert_eq!(
        hook_target(
            Some(Path::new(r"E:\cache")),
            Some(Path::new(r"D:\tmp")),
            home
        ),
        PathBuf::from(r"E:\cache")
    );
    assert_eq!(
        hook_target(None, None, home),
        home.join(".cache/dotfiles-target")
    );
}

#[cfg(not(windows))]
#[test]
fn other_hosts_share_one_target_in_the_home_directory() {
    let home = Path::new("/home/owner");
    assert_eq!(
        hook_target(None, Some(Path::new("/tmp")), home),
        home.join(".cache/dotfiles-target")
    );
    assert_eq!(
        hook_target(Some(Path::new("target")), None, home),
        home.join(".cache/dotfiles-target")
    );
    assert_eq!(
        hook_target(Some(Path::new("/cache")), None, home),
        PathBuf::from("/cache")
    );
}

#[test]
fn hook_gates_build_outside_the_worktree() {
    let native = Native::new(Path::new("home")).expect("native");
    let command = native.hook_command(Tool::Lefthook);
    let value = command
        .get_envs()
        .find(|(name, _)| *name == OsStr::new("CARGO_TARGET_DIR"))
        .and_then(|(_, value)| value)
        .expect("CARGO_TARGET_DIR is set");
    assert!(Path::new(value).is_absolute() || Path::new(value).starts_with("home"));
}
