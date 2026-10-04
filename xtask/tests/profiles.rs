use dotfiles_xtask::{
    profile_rules::{Mode, Profile, Reconcile, permitted, reconcile, selected},
    profiles, quality,
};
use std::fs;
use std::path::Path;

#[test]
fn runtime_credentials_and_personal_values_are_refused() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("README.md"),
        "Example configuration.\n",
    )
    .unwrap();
    profiles::check_public(directory.path()).unwrap();
    fs::create_dir_all(directory.path().join("dot_codex")).unwrap();
    fs::write(directory.path().join("dot_codex/auth.json"), "{}\n").unwrap();
    assert!(profiles::check_public(directory.path()).is_err());
    fs::remove_file(directory.path().join("dot_codex/auth.json")).unwrap();
    let key = directory.path().join("key.txt");
    for kind in [
        "OPENSSH PRIVATE KEY",
        "RSA PRIVATE KEY",
        "EC PRIVATE KEY",
        "PRIVATE KEY",
    ] {
        fs::write(&key, format!("-----BEGIN {kind}-----\nfixture\n")).unwrap();
        assert!(profiles::check_public(directory.path()).is_err());
    }
}

#[test]
fn cross_profile_preview_cannot_turn_into_a_live_apply() {
    assert!(permitted(Mode::Preview, false, false, false));
    assert!(permitted(Mode::FilesOnly, false, true, false));
    assert!(!permitted(Mode::FilesOnly, true, false, true));
    assert!(!permitted(Mode::Full, false, true, true));
    assert!(!permitted(Mode::Full, true, true, false));
    assert!(permitted(Mode::Full, true, false, true));
}

#[test]
fn new_upstream_edits_preserve_local_work_or_require_a_conflict() {
    assert_eq!(reconcile(false, false, false), Reconcile::Conflict);
    assert_eq!(reconcile(true, false, false), Reconcile::TakeUpstream);
    assert_eq!(reconcile(false, true, false), Reconcile::KeepLocal);
    assert_eq!(reconcile(false, false, true), Reconcile::KeepLocal);
    for profile in Profile::ALL {
        assert!(selected(profile, profile.bit()));
        assert!(!selected(profile, 15 ^ profile.bit()));
    }
}

#[test]
fn windows_launcher_parsing_uses_the_git_shell_rather_than_wsl() {
    assert_eq!(
        quality::git_for_windows_shell(Path::new("C:/Program Files/Git/mingw64/libexec/git-core")),
        Some(Path::new("C:/Program Files/Git/usr/bin/sh.exe").to_path_buf())
    );
    assert_eq!(
        quality::git_for_windows_shell(Path::new("libexec/git-core")),
        None
    );
}
