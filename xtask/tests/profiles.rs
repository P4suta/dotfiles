#![allow(
    clippy::disallowed_methods,
    reason = "integration tests spawn the binaries and real tools they verify"
)]

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

#[test]
fn directories_without_managed_content_are_reported() {
    let directories =
        ["Library", "Library/LaunchAgents", ".config", ".config/git"].map(String::from);
    let leaves = [".config/git/config", "Library-notes.txt"].map(String::from);
    assert_eq!(
        profiles::empty_directories(&directories, &leaves),
        ["Library", "Library/LaunchAgents"]
    );
}

#[test]
fn managed_listings_are_nul_separated_text() {
    assert_eq!(
        profiles::managed_paths(b".config/git/config\0path with ' quote\0").unwrap(),
        [".config/git/config", "path with ' quote"]
    );
    assert!(profiles::managed_paths(b"").unwrap().is_empty());
    assert!(profiles::managed_paths(b"\xff\0").is_err());
}

#[test]
fn native_verification_ignores_always_run_scripts() {
    assert_eq!(
        profiles::native_action_arguments("verify"),
        ["verify", "--exclude", "scripts"]
    );
    assert_eq!(profiles::native_action_arguments("apply"), ["apply"]);
    assert_eq!(profiles::native_action_arguments("diff"), ["diff"]);
}

#[test]
fn skipped_setup_scripts_need_a_target_name_and_a_reason() {
    let accepted = serde_json::json!({"setup": {"skip": [{"script": "setup-ocaml.sh", "reason": "compiles a toolchain"}]}});
    assert_eq!(profiles::declared_skips(&accepted).unwrap().len(), 1);
    for rejected in [
        serde_json::json!({"setup": {"skip": [{"script": "setup-ocaml.sh", "reason": " "}]}}),
        serde_json::json!({"setup": {"skip": [{"script": "../escape.sh", "reason": "x"}]}}),
        serde_json::json!({"setup": {"skip": [{"script": "setup-ocaml.sh"}]}}),
        serde_json::json!({"setup": {"skip": [{"script": "a.sh", "reason": "x", "extra": 1}]}}),
    ] {
        assert!(profiles::declared_skips(&rejected).is_err(), "{rejected}");
    }
    assert!(
        profiles::declared_skips(&serde_json::json!({}))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn rehearsal_configuration_is_read_by_the_pinned_chezmoi_as_intended() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    for profile in Profile::ALL {
        let scope = tempfile::tempdir().unwrap();
        let home = scope.path().join("home");
        fs::create_dir(&home).unwrap();
        let config = scope.path().join("chezmoi.toml");
        fs::write(
            &config,
            dotfiles_xtask::rehearsal::configuration(root, profile, &home).unwrap(),
        )
        .unwrap();
        let output = profiles::chezmoi(root, scope.path(), &config, &home)
            .args(["data", "--format", "json"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let data: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(profiles::configured_profile(&data).unwrap(), profile);
        assert_eq!(
            data["platforms"][profile.name()]["tools"]["common"],
            serde_json::json!(["jq"])
        );
        let mise = profiles::chezmoi(root, scope.path(), &config, &home)
            .arg("--override-data")
            .arg(format!(
                r#"{{"platforms":{{"{0}":{{"tools":{{"personal":["jq"]}}}}}}}}"#,
                profile.name()
            ))
            .args(["cat"])
            .arg(home.join(".config/mise/config.toml"))
            .output()
            .unwrap();
        assert!(
            mise.status.success(),
            "{}",
            String::from_utf8_lossy(&mise.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&mise.stdout)
                .matches("\n\"jq\" =")
                .count(),
            1,
            "a tool listed twice must render once"
        );
        let skips = profiles::declared_skips(&data).unwrap();
        assert_eq!(skips.is_empty(), profile == Profile::Windows, "{skips:?}");
        let listed = profiles::chezmoi(root, scope.path(), &config, &home)
            .args(["managed", "--include", "scripts", "--nul-path-separator"])
            .output()
            .unwrap();
        assert!(
            listed.status.success(),
            "{}",
            String::from_utf8_lossy(&listed.stderr)
        );
        let scripts = profiles::managed_paths(&listed.stdout).unwrap();
        assert!(!scripts.is_empty());
        for skip in skips {
            assert!(
                !scripts.contains(&skip.script),
                "{} still runs",
                skip.script
            );
        }
    }
}

#[test]
fn public_owner_sources_clone_over_https_despite_a_host_ssh_rewrite() {
    use dotfiles_xtask::setup::OwnerSource;
    let scope = tempfile::tempdir().unwrap();
    let global = scope.path().join("gitconfig");
    fs::write(
        &global,
        "[url \"git@github.com:\"]\n\tinsteadOf = https://github.com/\n",
    )
    .unwrap();
    for (source, expected) in [
        (
            OwnerSource::Ocomment,
            "https://github.com/P4suta/OComment.git",
        ),
        (
            OwnerSource::Domyjob,
            "https://github.com/P4suta/domyjob.git",
        ),
        (OwnerSource::Fleet, "git@github.com:P4suta/fleet"),
    ] {
        let mut arguments = source.clone_arguments(&scope.path().join("checkout"));
        let clone = arguments
            .iter()
            .position(|argument| argument == "clone")
            .unwrap();
        let url = arguments[clone + 1].clone();
        arguments.truncate(clone);
        let resolved = std::process::Command::new("git")
            .env("GIT_CONFIG_GLOBAL", &global)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args(&arguments)
            .args(["ls-remote", "--get-url"])
            .arg(url)
            .output()
            .unwrap();
        assert!(resolved.status.success());
        assert_eq!(String::from_utf8(resolved.stdout).unwrap().trim(), expected);
    }
}
