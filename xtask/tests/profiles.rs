use dotfiles_xtask::{
    profile_rules::{Mode, Profile, Reconcile, permitted, reconcile},
    profiles, quality,
};
use std::fs;
use std::path::Path;

#[expect(
    clippy::disallowed_methods,
    reason = "the shared constructor for fixture Git processes"
)]
#[path = "../../crates/dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

#[test]
fn templates_read_data_at_explicit_paths_instead_of_a_merged_platform_context() {
    fn walk(directory: &Path, found: &mut Vec<String>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, found);
            } else if fs::read_to_string(&path)
                .is_ok_and(|text| text.contains("index .platforms .profile"))
            {
                found.push(path.display().to_string());
            }
        }
    }
    let mut found = Vec::new();
    walk(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../home"),
        &mut found,
    );
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn wsl_is_linux_on_a_microsoft_kernel() {
    use dotfiles_xtask::profile_rules::{System, detected, microsoft_kernel};
    assert!(microsoft_kernel("5.15.167.4-microsoft-standard-WSL2"));
    assert!(microsoft_kernel("4.4.0-19041-Microsoft"));
    assert!(!microsoft_kernel("6.8.0-45-generic"));
    assert_eq!(detected(System::Linux, true), Profile::Wsl);
    assert_eq!(detected(System::Linux, false), Profile::Linux);
    assert_eq!(detected(System::Mac, true), Profile::Mac);
    assert_eq!(detected(System::Windows, true), Profile::Windows);
}

#[test]
fn the_configuration_template_writes_exactly_the_keys_its_consumers_read() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for combination in dotfiles_xtask::profile_snapshot::Combination::all() {
        let mut answers =
            profiles::Answers::fixture(combination.profile, "/fixture/projects".into());
        answers.role = combination.role.name();
        if combination.signing {
            answers.public_key = "ssh-ed25519 AAAA fixture".into();
        }
        let text = profiles::configuration(&root, &answers).unwrap();
        let config: toml::Table = toml::from_str(&text).unwrap();
        let data = serde_json::to_value(&config["data"]).unwrap();
        assert_eq!(data["profile"], combination.profile.name());
        assert_eq!(data["role"], combination.role.name());
        assert_eq!(data["signing"]["enabled"], combination.signing);
        assert_eq!(data["paths"]["projects"], "/fixture/projects");
        let mut keys = Vec::new();
        let mut stack = vec![(String::new(), &data)];
        while let Some((prefix, value)) = stack.pop() {
            for (key, inner) in value.as_object().into_iter().flatten() {
                let path = format!("{prefix}{key}");
                if inner.is_object() {
                    stack.push((format!("{path}."), inner));
                } else {
                    keys.push(path);
                }
            }
        }
        keys.sort();
        assert_eq!(
            keys,
            [
                "doppler.config",
                "doppler.project",
                "doppler.secrets.claude",
                "doppler.secrets.codex",
                "doppler.secrets.opencode",
                "git.email",
                "git.name",
                "herdr_agent.public_keys",
                "paths.projects",
                "profile",
                "role",
                "signing.enabled",
                "signing.public_key",
            ],
            "{}",
            combination.name()
        );
        assert_eq!(
            config
                .get("interpreters")
                .and_then(|interpreters| interpreters.get("ps1"))
                .and_then(|ps1| ps1.get("command"))
                .and_then(toml::Value::as_str),
            (combination.profile == Profile::Windows).then_some("pwsh"),
            "{}",
            combination.name()
        );
    }
}

#[test]
fn the_target_manifest_refuses_unknown_profiles_and_sets() {
    let data = serde_json::json!({
        "profiles": {"mac": ["mac"], "all": ["mac", "linux", "windows", "wsl"], "typo": ["macos"]},
        "targets": {".zshrc": "mac", ".bashrc": "unix"},
        "retired": {".gitconfig": "mac"},
    });
    let found = profiles::manifest_findings(&data);
    assert_eq!(found.len(), 3, "{found:?}");
    assert!(
        found
            .iter()
            .any(|line| line.contains("set all holds every profile"))
    );
    assert!(
        found
            .iter()
            .any(|line| line.contains("unknown profile macos"))
    );
    assert!(
        found
            .iter()
            .any(|line| line.contains(".bashrc names no set"))
    );
    let data = profiles::source_data(&Path::new(env!("CARGO_MANIFEST_DIR")).join("..")).unwrap();
    assert_eq!(profiles::manifest_findings(&data), Vec::<String>::new());
}

#[test]
fn a_manifest_target_without_a_source_is_found() {
    let data = serde_json::json!({
        "profiles": {"mac": ["mac"]},
        "targets": {".zshrc": "mac", ".config/ghostty": "mac", ".chezmoiscripts/unix/**": "mac", ".gone": "mac"},
    });
    let dump = serde_json::json!({".zshrc": {}, ".config/ghostty/config": {}});
    assert_eq!(
        profiles::absent_targets(&data, Profile::Mac, &dump),
        [".gone"]
    );
    assert_eq!(
        profiles::absent_targets(&data, Profile::Linux, &dump),
        Vec::<String>::new()
    );
}

#[test]
fn the_source_data_files_hold_only_known_keys() {
    let data = profiles::source_data(&Path::new(env!("CARGO_MANIFEST_DIR")).join("..")).unwrap();
    for profile in Profile::ALL {
        assert!(
            data["platforms"][profile.name()]["tools"]["common"].is_array(),
            "{}",
            profile.name()
        );
    }
    assert!(data["palette"]["bg"].is_string());
}

#[test]
fn every_ignore_and_remove_pattern_is_distinct_and_justified() {
    dotfiles_xtask::ignore_audit::check(&Path::new(env!("CARGO_MANIFEST_DIR")).join("..")).unwrap();
}

#[test]
fn an_ignore_pattern_that_matches_nothing_is_refused() {
    let snapshot = source_snapshot();
    let ignore = snapshot.path().join("home/.chezmoiignore");
    let mut text = fs::read_to_string(&ignore).unwrap();
    text.push_str("\n.config/never-declared\n.config/never-declared\n");
    fs::write(&ignore, text).unwrap();
    let error = format!(
        "{:#}",
        dotfiles_xtask::ignore_audit::check(snapshot.path()).unwrap_err()
    );
    for profile in Profile::ALL {
        assert!(
            error.contains(&format!(
                "{} .chezmoiignore .config/never-declared: Unjustified",
                profile.name()
            )) && error.contains(&format!(
                "{} .chezmoiignore .config/never-declared: Repeated",
                profile.name()
            )),
            "{error}"
        );
    }
}

#[test]
fn a_rendering_that_differs_from_its_manifest_is_refused() {
    let snapshot = source_snapshot();
    let attributes = snapshot.path().join("home/dot_config/git/attributes");
    let mut text = fs::read_to_string(&attributes).unwrap();
    text.push_str("*.fixture binary\n");
    fs::write(&attributes, text).unwrap();
    let error = format!(
        "{:#}",
        dotfiles_xtask::profile_snapshot::check(snapshot.path()).unwrap_err()
    );
    for combination in dotfiles_xtask::profile_snapshot::Combination::all() {
        assert!(
            error.contains(&format!(
                "{}: targets .config/git/attributes",
                combination.name()
            )),
            "{error}"
        );
    }
    assert!(error.contains("profile-snapshots --write"), "{error}");
}

#[test]
fn every_profile_holds_what_the_installers_apply() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for profile in Profile::ALL {
        let scope = tempfile::tempdir().unwrap();
        let home = scope.path().join("home");
        fs::create_dir(&home).unwrap();
        let config = scope.path().join("chezmoi.toml");
        fs::write(
            &config,
            dotfiles_xtask::rehearsal::configuration(&root, profile, &home).unwrap(),
        )
        .unwrap();
        for (source, targets) in [
            (
                true,
                vec![root.join(dotfiles_xtask::review_guard::SOURCE_POLICY)],
            ),
            (
                false,
                vec![
                    home.join(".config/prose"),
                    home.join(".config/opencode/plugins/prose.ts"),
                ],
            ),
        ] {
            let mut command = profiles::chezmoi(&root, scope.path(), &config, &home);
            command.args(["apply", "--dry-run", "--force", "--include", "dirs,files"]);
            if source {
                command.arg("--source-path");
            }
            let output = command
                .arg("--parent-dirs")
                .args(&targets)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{} cannot install {targets:?}: {}",
                profile.name(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

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
    assert_eq!(profiles::managed_paths(b"").unwrap(), Vec::<String>::new());
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
    assert_eq!(
        profiles::declared_skips(&serde_json::json!({})).unwrap(),
        Vec::<profiles::SkippedScript>::new()
    );
}

/// How many `jq` entries the mise configuration renders when the personal list repeats the common one.
fn rendered_tool_entries(
    mut chezmoi: std::process::Command,
    profile: Profile,
    home: &Path,
) -> usize {
    let mise = chezmoi
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
    String::from_utf8_lossy(&mise.stdout)
        .matches("\n\"jq\" =")
        .count()
}

#[test]
fn rehearsal_configuration_is_read_by_the_pinned_chezmoi_as_intended() {
    let snapshot = source_snapshot();
    let root = snapshot.path();
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
        let common = data["platforms"][profile.name()]["tools"]["common"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(common[0], "jq");
        for kept in &common[1..] {
            assert!(
                dotfiles_xtask::tool::Tool::ALL
                    .iter()
                    .filter_map(|tool| tool.package(profile))
                    .any(|(list, entry)| list == "/tools/common" && kept == entry),
                "the rehearsal keeps only declared requirements: {kept}"
            );
        }
        assert_eq!(
            rendered_tool_entries(
                profiles::chezmoi(root, scope.path(), &config, &home),
                profile,
                &home
            ),
            1,
            "a tool listed twice must render once"
        );
        let platform = &data["platforms"][profile.name()];
        let rendered = profiles::chezmoi(root, scope.path(), &config, &home)
            .args(["dump", "--include", "files,scripts", "--format", "json"])
            .output()
            .unwrap();
        let rendered: serde_json::Value = serde_json::from_slice(&rendered.stdout).unwrap();
        let contents = |scripts: bool| -> String {
            rendered
                .as_object()
                .unwrap()
                .values()
                .filter(|entry| (entry["type"] == "script") == scripts)
                .filter_map(|entry| entry["contents"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert!(
            profiles::unprovisioned(profile, platform, &contents(true), &contents(false))
                .is_empty(),
            "the rehearsal data must still install what its scripts and entry points require"
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
        assert_ne!(scripts, Vec::<String>::new());
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
        let resolved = fixture_git::command("git", &global)
            .args(&arguments)
            .args(["ls-remote", "--get-url"])
            .arg(url)
            .output()
            .unwrap();
        assert!(resolved.status.success());
        assert_eq!(String::from_utf8(resolved.stdout).unwrap().trim(), expected);
    }
}

#[test]
fn scripted_steps_are_read_from_rendered_launchers_in_order() {
    use dotfiles_xtask::setup::{Step, scripted_steps};
    let scripts = "#!/bin/sh\nexec '/h/.local/bin/dotfiles-xtask' --root '/s' setup fleet --config '/c' --live\n#!/bin/sh\nexec cargo run -- --root '/s' setup runtime --config '/c' --live\n& 'dotctl.exe' setup keyboard\n";
    assert_eq!(scripted_steps(scripts), [Step::Fleet, Step::Runtime]);
}

#[test]
fn comment_overrides_cannot_capture_whole_directories() {
    let scope = tempfile::tempdir().unwrap();
    fs::write(scope.path().join("notes"), "prose\n").unwrap();
    let write = |paths: &str| {
        fs::write(
            scope.path().join(".ocomment.toml"),
            format!("version = 1\n[[overrides]]\npaths = [{paths}]\nlanguage = \"markdown\"\n"),
        )
        .unwrap();
    };
    write(r#""**/*.md.tmpl", "notes""#);
    quality::comment_scopes(scope.path()).unwrap();
    for refused in [r#"".chezmoitemplates/*""#, r#""missing""#, r#""docs/**""#] {
        write(refused);
        assert!(quality::comment_scopes(scope.path()).is_err(), "{refused}");
    }
}

#[test]
fn setup_order_rejects_a_step_that_runs_before_its_prerequisite() {
    use dotfiles_xtask::setup::{Step, ordering_violations};
    assert_eq!(
        ordering_violations(&[Step::Runtime, Step::Mise, Step::Tools, Step::Domyjob]),
        Vec::<(Step, Step)>::new()
    );
    assert_eq!(
        ordering_violations(&[Step::Mise, Step::Domyjob, Step::Tools]),
        [(Step::Domyjob, Step::Tools)]
    );
    assert_eq!(
        ordering_violations(&[Step::Domyjob]),
        Vec::<(Step, Step)>::new()
    );
    let listing = br#"{"10-b.sh":{"sourceRelative":"run_onchange_after_10-b.sh.tmpl"},"a.sh":{"sourceRelative":"run_after_a.sh.tmpl"},"z.sh":{"sourceRelative":"run_onchange_before_z.sh.tmpl"}}"#;
    assert_eq!(
        profiles::script_order(listing).unwrap(),
        ["z.sh", "10-b.sh", "a.sh"]
    );
}

#[test]
fn setup_that_needs_a_package_requires_the_profile_to_install_it() {
    let scripts = "exec dotfiles-xtask --root /s setup domyjob --config /c --live\n";
    let provided = serde_json::json!({"brew": {"formulae": ["jq", "lefthook"]}});
    assert_eq!(
        profiles::unprovisioned(Profile::Mac, &provided, scripts, ""),
        Vec::<String>::new()
    );
    let missing = serde_json::json!({"brew": {"formulae": ["jq"]}});
    assert_eq!(
        profiles::unprovisioned(Profile::Mac, &missing, scripts, ""),
        ["/brew/formulae/lefthook"]
    );
    let disabled = serde_json::json!({"nix": {"packages": {"lefthook": ""}}});
    assert_eq!(
        profiles::unprovisioned(Profile::Linux, &disabled, "setup ocomment\n", "").len(),
        1
    );
    let shell = "& 'C:/h/.local/bin/dotctl.exe' setup shell\n";
    assert_eq!(
        profiles::unprovisioned(
            Profile::Windows,
            &serde_json::json!({"scoop": {"apps": ["starship"]}}),
            shell,
            ""
        ),
        ["/scoop/apps/zoxide"]
    );
}

#[test]
fn launchers_exec_their_installed_path_rather_than_the_runtime_home() {
    let home = "#!/bin/sh\nexec \"$HOME/.local/bin/dotguard\" git \"$@\"\n";
    assert!(quality::launcher_resolves_through_home(
        ".local/bin/git",
        home
    ));
    assert!(quality::launcher_resolves_through_home(
        ".config/git/hooks/pre-push",
        home
    ));
    let installed = "#!/bin/sh\nexec '/Users/fixture/.local/bin/dotguard' git \"$@\"\n";
    assert!(!quality::launcher_resolves_through_home(
        ".local/bin/git",
        installed
    ));
    assert!(!quality::launcher_resolves_through_home(".bashrc", home));
}

#[test]
fn shared_templates_must_be_placed_included_and_distinct() {
    let scope = tempfile::tempdir().unwrap();
    let templates = scope.path().join("home/.chezmoitemplates");
    for directory in ["partials", "native/topic", "native/twin", "profiles/mac"] {
        fs::create_dir_all(templates.join(directory)).unwrap();
    }
    fs::write(templates.join("partials/used"), "a\n").unwrap();
    fs::write(templates.join("partials/stale"), "b\n").unwrap();
    fs::write(templates.join("native/topic/mac.toml"), "c\n").unwrap();
    fs::write(templates.join("native/topic/linux.toml"), "d\n").unwrap();
    fs::write(templates.join("native/twin/mac"), "e\n").unwrap();
    fs::write(templates.join("native/twin/wsl"), "e\n").unwrap();
    fs::write(templates.join("profiles/mac/dot_x"), "f\n").unwrap();
    fs::write(
        scope.path().join("home/dot_used.tmpl"),
        concat!(
            "{{ includeTemplate \"partials/used\" . }}\n",
            "{{ include (printf \".chezmoitemplates/native/topic/%s.toml\" .profile) }}\n",
            "{{ include \".chezmoitemplates/native/twin/mac\" }}{{ include \".chezmoitemplates/native/twin/wsl\" }}\n",
            "{{ includeTemplate \"profiles/mac/dot_x\" . }}\n",
        ),
    )
    .unwrap();
    fs::write(
        scope.path().join("home/dot_guarded.tmpl"),
        "{{ fail \"Target is unavailable for this profile\" }}\n",
    )
    .unwrap();
    let found = quality::template_findings(scope.path()).unwrap();
    assert_eq!(found.len(), 6, "{found:#?}");
    for expected in [
        ".chezmoitemplates/native/twin/mac and native/twin/wsl hold the same bytes",
        ".chezmoitemplates/partials/stale: no source template includes it",
        ".chezmoitemplates/profiles/mac/dot_x: no source template includes it",
        ".chezmoitemplates/profiles/mac/dot_x: `dot_x` carries a chezmoi source attribute",
        ".chezmoitemplates/profiles/mac/dot_x: only partials/ and native/<topic>/<profile> may hold templates",
        "home/dot_guarded.tmpl: a profile guard inside a template",
    ] {
        assert!(
            found.iter().any(|line| line.starts_with(expected)),
            "{expected} missing from {found:#?}"
        );
    }
}

#[test]
fn a_native_body_is_named_after_a_profile() {
    assert_eq!(
        quality::template_path_findings("native/mise/windows.toml.tmpl"),
        Vec::<String>::new()
    );
    assert_eq!(
        quality::template_path_findings("partials/sh_quote"),
        Vec::<String>::new()
    );
    assert_eq!(
        quality::template_path_findings("native/mise/darwin.toml").len(),
        1
    );
    assert_eq!(quality::template_path_findings("native/mise.toml").len(), 1);
    assert_eq!(quality::template_path_findings("inputs").len(), 1);
}

#[test]
fn the_source_templates_hold_no_finding() {
    assert_eq!(
        quality::template_findings(&Path::new(env!("CARGO_MANIFEST_DIR")).join("..")).unwrap(),
        Vec::<String>::new()
    );
}

/// chezmoi reads every entry under its source, including ignored build directories, so a concurrent build in the checkout can remove a file between its listing and its lstat.
/// Contract tests read a copy without build outputs instead.
fn source_snapshot() -> tempfile::TempDir {
    fn copy(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            if matches!(name.to_str(), Some("target" | "node_modules" | ".git")) {
                continue;
            }
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                copy(&entry.path(), &to.join(&name));
            } else if kind.is_file() {
                fs::copy(entry.path(), to.join(&name)).unwrap();
            }
        }
    }
    let snapshot = tempfile::tempdir().unwrap();
    copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap(),
        snapshot.path(),
    );
    snapshot
}

#[test]
fn forgetting_script_runs_reruns_every_onchange_script() {
    let scope = tempfile::tempdir().unwrap();
    let source = scope.path().join("source");
    let destination = scope.path().join("home");
    let state = scope.path().join("state");
    let log = scope.path().join("runs");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&destination).unwrap();
    let (script, contents) = if cfg!(windows) {
        (
            "run_onchange_after_10-record.ps1",
            format!("Add-Content -LiteralPath '{}' -Value ran\n", log.display()),
        )
    } else {
        (
            "run_onchange_after_10-record.sh",
            format!("#!/bin/sh\necho ran >> '{}'\n", log.display()),
        )
    };
    fs::write(source.join(script), contents).unwrap();
    let config = scope.path().join("chezmoi.toml");
    fs::write(&config, "").unwrap();
    let apply = || {
        let status = dotfiles_xtask::profiles::chezmoi(&source, &state, &config, &destination)
            .arg("apply")
            .status()
            .unwrap();
        assert!(status.success());
    };
    apply();
    apply();
    assert_eq!(fs::read_to_string(&log).unwrap().lines().count(), 1);
    dotfiles_xtask::profiles::forget_script_runs(&source, &state, &config, &destination).unwrap();
    apply();
    assert_eq!(fs::read_to_string(&log).unwrap().lines().count(), 2);
}

#[test]
fn hosts_may_differ_only_in_declared_git_settings() {
    let mac = "core.pager=delta\ncore.fsmonitor=true\ncore.hookspath=~/.config/git/hooks\n";
    let wsl = "core.pager=delta\ncore.sshcommand=ssh.exe\ncore.hookspath=~/.config/git/hooks\n";
    assert_eq!(profiles::policy(mac), profiles::policy(wsl));
    let drifted = "core.pager=less\ncore.hookspath=~/.config/git/hooks\n";
    assert_ne!(profiles::policy(mac), profiles::policy(drifted));
}

/// chezmoi keys `run_onchange_` state by target name and contents, so moving a script into `.chezmoiscripts/` reruns it once.
#[test]
fn moving_an_onchange_script_into_chezmoiscripts_reruns_it_once() {
    let scope = tempfile::tempdir().unwrap();
    let source = scope.path().join("source");
    let destination = scope.path().join("home");
    let state = scope.path().join("state");
    let log = scope.path().join("runs");
    let family = source.join(".chezmoiscripts/family");
    fs::create_dir_all(&family).unwrap();
    fs::create_dir_all(&destination).unwrap();
    let (script, contents) = if cfg!(windows) {
        (
            "run_onchange_after_10-record.ps1",
            format!("Add-Content -LiteralPath '{}' -Value ran\n", log.display()),
        )
    } else {
        (
            "run_onchange_after_10-record.sh",
            format!("#!/bin/sh\necho ran >> '{}'\n", log.display()),
        )
    };
    fs::write(source.join(script), contents).unwrap();
    let config = scope.path().join("chezmoi.toml");
    fs::write(&config, "").unwrap();
    let apply = || {
        let status = dotfiles_xtask::profiles::chezmoi(&source, &state, &config, &destination)
            .arg("apply")
            .status()
            .unwrap();
        assert!(status.success());
        fs::read_to_string(&log).unwrap().lines().count()
    };
    assert_eq!(apply(), 1);
    assert_eq!(apply(), 1);
    fs::rename(source.join(script), family.join(script)).unwrap();
    assert_eq!(apply(), 2);
    assert_eq!(apply(), 2);
}

/// Launchers pass the checkout root, which `.chezmoi.workingTree` names even when `.chezmoiroot` moves the source state below it.
#[test]
fn the_working_tree_is_the_checkout_root_under_a_chezmoiroot() {
    let scope = tempfile::tempdir().unwrap();
    let checkout = scope.path().join("checkout");
    let home = checkout.join("home");
    fs::create_dir_all(&home).unwrap();
    assert!(
        fixture_git::command("git", &scope.path().join("gitconfig"))
            .arg("-C")
            .arg(&checkout)
            .args(["init", "--quiet"])
            .status()
            .unwrap()
            .success()
    );
    fs::write(checkout.join(".chezmoiroot"), "home\n").unwrap();
    fs::write(
        home.join("probe.tmpl"),
        "{{ .chezmoi.workingTree }}|{{ .chezmoi.sourceDir }}",
    )
    .unwrap();
    let destination = scope.path().join("target");
    fs::create_dir_all(&destination).unwrap();
    let config = scope.path().join("chezmoi.toml");
    fs::write(&config, "").unwrap();
    let output = dotfiles_xtask::profiles::chezmoi(&checkout, scope.path(), &config, &destination)
        .args(["cat"])
        .arg(destination.join("probe"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rendered = String::from_utf8(output.stdout).unwrap();
    let canonical = |path: &Path| {
        dotfiles_xtask::profiles::chezmoi_path(&dotfiles_xtask::canonical(path).unwrap())
    };
    let (working_tree, source) = rendered.split_once('|').unwrap();
    assert_eq!(
        canonical(Path::new(working_tree)),
        canonical(&checkout),
        "{rendered}"
    );
    assert_eq!(canonical(Path::new(source)), canonical(&home), "{rendered}");
}

#[test]
fn recipe_references_are_commands_rather_than_prose() {
    let found = |text: &str, markdown: bool| -> Vec<String> {
        quality::recipe_references(text, markdown)
            .into_iter()
            .map(|(_, recipe)| recipe)
            .collect()
    };
    assert_eq!(found("drifted; run: just brew", false), ["brew"]);
    assert_eq!(
        found("run 'just apply' (or `just guard`)", false),
        ["apply", "guard"]
    );
    assert_eq!(found("see just apply", false), ["apply"]);
    assert_eq!(found("just refresh CONFIG", true), ["refresh"]);
    assert_eq!(found("$ just verify", true), ["verify"]);
    assert_eq!(
        found("This runs just before the hook.", false),
        Vec::<String>::new()
    );
    assert_eq!(found("just refresh CONFIG", false), Vec::<String>::new());
    assert_eq!(
        found("`just --list` prints recipes", true),
        Vec::<String>::new()
    );
}

#[test]
fn agent_launchers_require_doppler_wherever_they_render() {
    use dotfiles_xtask::setup::entry_requirements;
    use dotfiles_xtask::tool::Tool;
    assert_eq!(
        entry_requirements(
            r#"  ^dotfiles-xtask --root "/source" agent opencode --config "/config" -- ...$args"#
        ),
        [Tool::Doppler]
    );
    assert_eq!(
        entry_requirements("& 'C:/tools/dotfiles-xtask.exe' agent codex"),
        [Tool::Doppler]
    );
    assert_eq!(
        entry_requirements("dotfiles-xtask --root /source setup tools --live"),
        Vec::<dotfiles_xtask::tool::Tool>::new()
    );
    assert_eq!(
        entry_requirements(
            r#"exec 'C:/Users/owner/.local/bin/dotfiles-xtask.exe' hook pre-commit -- "$@""#
        ),
        [Tool::Lefthook]
    );
    assert_eq!(
        entry_requirements("The agent reads its configuration here."),
        Vec::<dotfiles_xtask::tool::Tool>::new()
    );
}

#[test]
fn a_profile_without_the_package_an_entry_point_needs_is_unprovisioned() {
    let launcher = "^dotfiles-xtask agent opencode";
    let without = serde_json::json!({"tools": {"common": ["jq"]}});
    assert_eq!(
        profiles::unprovisioned(Profile::Mac, &without, "", launcher),
        ["/tools/common/doppler"]
    );
    let with = serde_json::json!({"tools": {"common": ["jq", "doppler"]}});
    assert_eq!(
        profiles::unprovisioned(Profile::Mac, &with, "", launcher),
        Vec::<String>::new()
    );
    assert_eq!(
        profiles::unprovisioned(Profile::Mac, &without, "", ""),
        Vec::<String>::new()
    );
}

#[test]
fn only_files_changed_since_the_last_application_are_reported() {
    assert_eq!(
        profiles::externally_changed(
            "MM .config/wezterm/wezterm.lua\n M .config/git/config\n A .b\n"
        ),
        [".config/wezterm/wezterm.lua"]
    );
    let scope = tempfile::tempdir().unwrap();
    let source = scope.path().join("source");
    let destination = scope.path().join("home");
    let state = scope.path().join("state");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&destination).unwrap();
    fs::write(source.join("dot_edited"), "desired\n").unwrap();
    fs::write(source.join("dot_kept"), "desired\n").unwrap();
    fs::write(destination.join(".edited"), "from an earlier repository\n").unwrap();
    let config = scope.path().join("chezmoi.toml");
    fs::write(&config, "").unwrap();
    let status = || {
        let output = profiles::chezmoi(&source, &state, &config, &destination)
            .args(["status", "--exclude", "scripts"])
            .output()
            .unwrap();
        assert!(output.status.success());
        profiles::externally_changed(&String::from_utf8_lossy(&output.stdout))
    };
    assert!(
        status().is_empty(),
        "a file chezmoi never wrote is taken over"
    );
    let applied = profiles::chezmoi(&source, &state, &config, &destination)
        .args(["apply", "--force"])
        .status()
        .unwrap();
    assert!(applied.success());
    fs::write(destination.join(".edited"), "fixed on the host\n").unwrap();
    fs::write(source.join("dot_kept"), "changed in the source\n").unwrap();
    assert_eq!(status(), [".edited"]);
}

#[test]
fn a_client_rewriting_a_merged_target_is_not_an_uncommitted_host_edit() {
    let scope = tempfile::tempdir().unwrap();
    let source = scope.path().join("source");
    let destination = scope.path().join("home");
    let state = scope.path().join("state");
    fs::create_dir_all(source.join("dot_client")).unwrap();
    fs::create_dir_all(&destination).unwrap();
    fs::write(source.join("dot_edited"), "desired\n").unwrap();
    fs::write(
        source.join("dot_client/modify_settings.json"),
        "{{- /* chezmoi:modify-template */ -}}\n{{- $current := dict -}}\n{{- if trim .chezmoi.stdin -}}{{- $current = fromJson .chezmoi.stdin -}}{{- end -}}\n{{ setValueAtPath \"memory\" false $current | toJson }}\n",
    )
    .unwrap();
    let config = scope.path().join("chezmoi.toml");
    fs::write(&config, "").unwrap();
    let chezmoi = || profiles::chezmoi(&source, &state, &config, &destination);
    assert!(
        chezmoi()
            .args(["apply", "--force"])
            .status()
            .unwrap()
            .success()
    );
    fs::write(
        destination.join(".client/settings.json"),
        r#"{"memory":true,"model":"chosen at run time"}"#,
    )
    .unwrap();
    fs::write(destination.join(".edited"), "fixed on the host\n").unwrap();
    let status = chezmoi()
        .args(["status", "--exclude", "scripts"])
        .output()
        .unwrap();
    let changed = profiles::externally_changed(&String::from_utf8_lossy(&status.stdout));
    assert_eq!(changed, [".client/settings.json", ".edited"]);
    let listing = chezmoi()
        .args([
            "managed",
            "--include",
            "files",
            "--path-style",
            "all",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(listing.status.success());
    let merged = profiles::merged_targets(&listing.stdout).unwrap();
    assert_eq!(merged, [".client/settings.json"]);
    assert_eq!(
        profiles::uncommitted_host_edits(&source, &state, &config, &destination).unwrap(),
        [".edited"]
    );
    assert!(
        chezmoi()
            .args(["apply", "--force"])
            .status()
            .unwrap()
            .success()
    );
    let applied: serde_json::Value =
        serde_json::from_slice(&fs::read(destination.join(".client/settings.json")).unwrap())
            .unwrap();
    assert_eq!(
        applied,
        serde_json::json!({"memory": false, "model": "chosen at run time"})
    );
    assert!(profiles::merged_targets(br#"{".a":{}}"#).is_err());
}

#[test]
fn a_modify_template_that_drops_a_host_key_still_refuses_application() {
    let scope = tempfile::tempdir().unwrap();
    let source = scope.path().join("source");
    let destination = scope.path().join("home");
    let state = scope.path().join("state");
    fs::create_dir_all(source.join("dot_client")).unwrap();
    fs::create_dir_all(&destination).unwrap();
    fs::write(
        source.join("dot_client/modify_settings.json"),
        "{{- /* chezmoi:modify-template */ -}}
{\"memory\":false}
",
    )
    .unwrap();
    let config = scope.path().join("chezmoi.toml");
    fs::write(&config, "").unwrap();
    let chezmoi = || profiles::chezmoi(&source, &state, &config, &destination);
    assert!(
        chezmoi()
            .args(["apply", "--force"])
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(
        profiles::uncommitted_host_edits(&source, &state, &config, &destination).unwrap(),
        Vec::<String>::new()
    );
    fs::write(
        destination.join(".client/settings.json"),
        r#"{"memory":true,"model":"chosen at run time"}"#,
    )
    .unwrap();
    assert_eq!(
        profiles::uncommitted_host_edits(&source, &state, &config, &destination).unwrap(),
        [".client/settings.json"]
    );
}

#[test]
fn a_merge_keeps_host_keys_only_when_every_key_path_survives() {
    let keeps = |target, current, rendered| profiles::keeps_host_keys(target, current, rendered);
    assert!(
        keeps(
            "a.json",
            r#"{"a":{"b":1},"c":2}"#,
            r#"{"a":{"b":3,"d":4},"c":true}"#
        )
        .unwrap()
    );
    assert!(!keeps("a.json", r#"{"a":{"b":1},"c":2}"#, r#"{"a":{"d":4},"c":2}"#).unwrap());
    assert!(!keeps("a.json", r#"{"a":1}"#, r"{}").unwrap());
    assert!(keeps("a.json", "", r#"{"memory":false}"#).unwrap());
    assert!(
        keeps(
            "a.toml",
            "x = 1
[t]
y = 2
",
            "x = 1
z = 0
[t]
y = 3
"
        )
        .unwrap()
    );
    assert!(
        !keeps(
            "a.toml",
            "x = 1
[t]
y = 2
",
            "x = 1
[t]
"
        )
        .unwrap()
    );
    assert!(keeps("a.json", "{", "{}").is_err());
    let unknown = keeps("a.yaml", "a: 1", "a: 1").unwrap_err().to_string();
    assert!(
        unknown.contains("a.yaml") && unknown.contains("keeps_host_keys"),
        "{unknown}"
    );
}

#[test]
fn the_dropped_host_key_refusal_names_the_cause_and_a_runnable_next_action() {
    let message = profiles::dropped_host_key_refusal("wsl", ".claude/settings.json");
    let (cause, action) = message.split_once('\n').unwrap();
    assert!(
        cause.contains("wsl") && cause.contains(".claude/settings.json"),
        "{message}"
    );
    assert!(action.contains("`just profiles`"), "{message}");
}

#[test]
fn the_host_edit_refusal_names_each_target_and_runnable_commands() {
    let message = profiles::host_edit_refusal(
        &[".edited".into()],
        Path::new("/c/config.toml"),
        Path::new("/home/user"),
        Path::new("/s"),
    );
    let (cause, action) = message.split_once('\n').unwrap();
    assert!(
        cause.contains("1 managed files") && cause.contains(".edited"),
        "{message}"
    );
    assert!(
        action.contains("`just diff '/c/config.toml' '/home/user' '/s'`")
            && action.contains("`just apply '/c/config.toml' '/home/user' '/s' "),
        "{message}"
    );
}
