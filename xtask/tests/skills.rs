use anyhow::Result;
use serde_json::{Value, json};
use skill_ops::{Metadata, export_checks, metadata, validate_links, validate_tree};
use std::fs;
use std::path::Path;

#[path = "../../crates/platform/tests/support/fixture_process.rs"]
mod fixture_process;

const PROFILE_APPLY_FIXTURE: &str = "DOTFILES_SKILL_APPLY_SOURCE";
const PROFILE_APPLY_TEST: &str =
    "profile_apply_records_changed_skills_before_scoped_pr_install_without_enabling_maintenance";

fn catalog_source() -> Result<tempfile::TempDir> {
    let source = tree()?;
    let root = source.path();
    fs::write(root.join(".chezmoiroot"), "home\n")?;
    for relative in ["home/dot_agents/skills/pull-request", "home/.chezmoidata"] {
        fs::create_dir_all(root.join(relative))?;
    }
    fs::write(
        root.join("home/dot_agents/skills/pull-request/SKILL.md"),
        "---\nname: pull-request\ndescription: Publish a checked change.\n---\n# Contract\nCheck the current change.\n",
    )?;
    fs::write(
        root.join("home/dot_claude/skills/symlink_pull-request"),
        "../../.agents/skills/pull-request\n",
    )?;
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../home/.chezmoidata");
    for name in ["targets.toml", "profiles.toml"] {
        fs::write(
            root.join("home/.chezmoidata").join(name),
            fs::read(data.join(name))?,
        )?;
    }
    skill_ops::write_json(
        &root.join("home/dot_config/skill-ops/policy.json"),
        &json!({"schema":1,"review_every":3,"minimum_pair_sessions":2,
            "pair_percent":80,"max_entry_words":100,"requirements":[],"bundles":{}}),
        false,
    )?;
    track_catalog(root)?;
    Ok(source)
}

fn track_catalog(root: &Path) -> Result<()> {
    for (name, skill) in skill_ops::catalog(&root.join("home/dot_agents/skills"))?.skills {
        skill_ops::write_json(
            &root.join(skill_ops::DECISIONS).join(format!("{name}.json")),
            &skill_ops::SkillDecision {
                revision: skill.revision,
                outcome: skill_ops::Outcome::Keep,
                reason: "The fixture checks the native installation boundary.".into(),
                evidence: vec![format!("home/dot_agents/skills/{name}/SKILL.md")],
                revisit: None,
                size: None,
            },
            true,
        )?;
    }
    Ok(())
}

fn install_catalog_command(source: &Path, home: &Path) -> Result<()> {
    let configuration = home.join(".config/chezmoi/chezmoi.toml");
    fs::create_dir_all(configuration.parent().expect("configuration parent"))?;
    fs::write(
        configuration,
        toml::to_string(&json!({
            "sourceDir": source,
            "data": {"profile": xtask::profiles::native_profile()?.name()}
        }))?,
    )?;
    let output = fixture_process::command(env!("CARGO_BIN_EXE_xtask"), source)
        .current_dir(source)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .arg("--root")
        .arg(source)
        .arg("install")
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn apply_profile_command(source: &Path, home: &Path) -> Result<()> {
    let output = fixture_process::command(std::env::current_exe()?, source)
        .args(["--exact", PROFILE_APPLY_TEST, "--nocapture"])
        .env(PROFILE_APPLY_FIXTURE, source)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn refuse_catalog_drift(source: &Path, user: &Path, binary: &Path) -> Result<()> {
    let manifest = user.join(".config/skill-ops/approved.json");
    let approved_bytes = fs::read(&manifest)?;
    let policy = user.join(".config/skill-ops/policy.json");
    let policy_bytes = fs::read(&policy)?;
    let mut mismatched_policy: skill_ops::Policy = skill_ops::read_json(&policy)?;
    mismatched_policy.review_every += 1;
    skill_ops::write_json(&policy, &mismatched_policy, true)?;
    let profile = xtask::profiles::native_profile()?;
    assert!(
        skill_ops::skill_install::install_catalog(source, user, profile)
            .unwrap_err()
            .to_string()
            .contains("policy differs")
    );
    assert_eq!(fs::read(&manifest)?, approved_bytes);
    fs::write(&policy, policy_bytes)?;
    fs::write(
        user.join(".agents/skills/example/SKILL.md"),
        "---\nname: example\ndescription: Inspect a changed native input.\n---\n# Drift\nRefuse this unreviewed native change.\n",
    )?;
    assert!(
        skill_ops::skill_install::install_catalog(source, user, profile)
            .unwrap_err()
            .to_string()
            .contains("installed skill catalog differs")
    );
    let mut applied = false;
    let error = pr_workflow::deploy(source, binary, user, |_| {
        applied = true;
        Ok(())
    })
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "unrelated installed managed skills have changed; preserve and review them before installation"
    );
    assert!(!applied);
    assert_eq!(fs::read(&manifest)?, approved_bytes);
    Ok(())
}

#[test]
fn profile_apply_records_changed_skills_before_scoped_pr_install_without_enabling_maintenance()
-> Result<()> {
    if let Some(source) = std::env::var_os(PROFILE_APPLY_FIXTURE) {
        let home = platform::home::home().expect("fixture home");
        return dotfiles::profiles::operate(
            Path::new(&source),
            dotfiles::profiles::NativeAction::Apply,
            &home.join(".config/chezmoi/chezmoi.toml"),
            &home,
            true,
            None,
            Some(&home.join("profile-backup")),
        );
    }
    for owner_disabled in [false, true] {
        let source = catalog_source()?;
        let home = tempfile::tempdir()?;
        let user = home.path();
        let history = user.join(".local/state/skill-ops/history.json");
        fs::create_dir_all(history.parent().expect("history parent"))?;
        fs::write(&history, "preserved history\n")?;
        let hooks = user.join(".codex/hooks.json");
        skill_ops::write_json(
            &hooks,
            &json!({"hooks":{"Stop":[{"hooks":[
            {"type":"command","command":"unrelated-guard"}]}]}}),
            false,
        )?;
        let previous_hooks = fs::read(&hooks)?;
        let stop = user.join(platform::incidents::DISABLED);
        let opt_in = user.join(platform::incidents::EXPERIMENTAL);
        if owner_disabled {
            fs::create_dir_all(stop.parent().expect("consent parent"))?;
            fs::write(&stop, "owner stop\n")?;
            fs::write(&opt_in, rules::incident_rules::EXPERIMENTAL_OPT_IN)?;
        }
        let manifest = user.join(".config/skill-ops/approved.json");
        let binary = user.join("built-pr-workflow");
        fs::write(&binary, "owned executable fixture")?;
        for revision in ["initial", "updated"] {
            fs::write(
                source
                    .path()
                    .join("home/dot_agents/skills/example/SKILL.md"),
                format!(
                    "---\nname: example\ndescription: Inspect a checked input.\n---\n# Contract\nCheck the {revision} input.\n"
                ),
            )?;
            track_catalog(source.path())?;
            if revision == "initial" {
                install_catalog_command(source.path(), user)?;
            } else {
                apply_profile_command(source.path(), user)?;
            }
            assert!(
                manifest.is_file(),
                "shared-skill installation must record the approved catalog"
            );
            let approved: skill_ops::Approved = skill_ops::read_json(&manifest)?;
            assert_eq!(
                approved.catalog,
                skill_ops::catalog(&source.path().join("home/dot_agents/skills"))?
            );
            assert_eq!(approved.reviewed.len(), 2);
            pr_workflow::deploy(source.path(), &binary, user, |_| Ok(()))?;
        }
        refuse_catalog_drift(source.path(), user, &binary)?;
        assert!(!platform::incidents::enabled_at(user));
        assert_eq!(fs::read(&history)?, b"preserved history\n");
        assert_eq!(fs::read(hooks)?, previous_hooks);
        assert!(
            !user
                .join(".local/bin")
                .join(format!("skill-ops{}", std::env::consts::EXE_SUFFIX))
                .try_exists()?
        );
        if owner_disabled {
            assert_eq!(fs::read(stop)?, b"owner stop\n");
            assert_eq!(
                fs::read(opt_in)?,
                rules::incident_rules::EXPERIMENTAL_OPT_IN
            );
        } else {
            assert!(!stop.try_exists()? && !opt_in.try_exists()?);
        }
    }
    Ok(())
}

#[test]
fn disabled_installer_removes_owned_hooks_without_loading_or_building_source() -> Result<()> {
    for owner_disabled in [false, true] {
        let scope = tempfile::tempdir()?;
        let source = scope.path().join("source");
        let home = scope.path().join("home");
        fs::create_dir(&source)?;
        let history = home.join(".local/state/skill-ops/history.json");
        fs::create_dir_all(history.parent().expect("history parent"))?;
        fs::write(&history, "preserved history\n")?;
        let stop = home.join(platform::incidents::DISABLED);
        let opt_in = home.join(platform::incidents::EXPERIMENTAL);
        if owner_disabled {
            fs::create_dir_all(stop.parent().expect("consent parent"))?;
            fs::write(&stop, "owner stop\n")?;
            fs::write(&opt_in, rules::incident_rules::EXPERIMENTAL_OPT_IN)?;
        }
        let expected = json!({"theme":"auto", "hooks":{
            "Stop":[{"matcher":"*", "hooks":[
                {"type":"command", "command":"unrelated-guard"},
                {"type":"command", "command":"skill-ops-custom stop"}
            ]}],
            "PostToolUse":[{"hooks":[{"type":"command", "command":"line-endings"}]}]
        }});
        for (client, relative) in [
            ("codex", ".codex/hooks.json"),
            ("claude", ".claude/settings.json"),
        ] {
            let mut settings = expected.clone();
            settings["hooks"]["Stop"][0]["hooks"]
                .as_array_mut()
                .expect("stop handlers")
                .push(json!({"type":"command", "command":"skill-ops stop"}));
            settings["hooks"]["PostToolUse"][0]["hooks"].as_array_mut().expect("observe handlers")
                .push(json!({"type":"command", "command":format!("skill-ops observe --client {client}")}));
            let path = home.join(relative);
            fs::create_dir_all(path.parent().expect("settings parent"))?;
            fs::write(path, serde_json::to_vec(&settings)?)?;
        }
        let output = fixture_process::command(env!("CARGO_BIN_EXE_xtask"), scope.path())
            .current_dir(&source)
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .arg("--root")
            .arg(&source)
            .arg("install-skill-ops")
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        for relative in [".codex/hooks.json", ".claude/settings.json"] {
            let actual: Value = serde_json::from_slice(&fs::read(home.join(relative))?)?;
            assert_eq!(actual, expected);
        }
        assert_eq!(fs::read_to_string(history)?, "preserved history\n");
        assert_eq!(fs::read_dir(&source)?.count(), 0);
        assert!(!home.join(".local/bin").try_exists()?);
        assert!(!home.join(".local/state/dotfiles").try_exists()?);
        assert!(!home.join(".config/skill-ops/approved.json").try_exists()?);
        if owner_disabled {
            assert_eq!(fs::read(stop)?, b"owner stop\n");
            assert_eq!(
                fs::read(opt_in)?,
                rules::incident_rules::EXPERIMENTAL_OPT_IN
            );
        } else {
            assert!(!stop.try_exists()? && !opt_in.try_exists()?);
        }
    }
    Ok(())
}

#[test]
fn folded_and_quoted_yaml_descriptions_are_discovered() -> Result<()> {
    let folded = "---\nname: example-skill\ndescription: >-\n  Inspect a project.\n  Use for a concrete task.\n---\n\n# Example\n";
    assert_eq!(
        metadata(folded, "example-skill")?,
        Metadata {
            name: "example-skill".into(),
            description: "Inspect a project. Use for a concrete task.".into(),
        }
    );
    let quoted =
        "---\nname: example-skill\ndescription: \"Inspect: a project.\"\n---\n\n# Example\n";
    assert_eq!(
        metadata(quoted, "example-skill")?.description,
        "Inspect: a project."
    );
    Ok(())
}

#[test]
fn malformed_or_incompatible_metadata_is_rejected() {
    for text in [
        "name: example-skill\ndescription: Task\n",
        "---\nname: example-skill\ndescription: [unterminated\n---\n",
        "---\nname: example-skill\ndescription: true\n---\n# Body\n",
        "---\nname: example-skill\ndescription: Task\nclient-only-field: true\n---\n# Body\n",
        "---\nname: other\ndescription: Task\n---\n# Body\n",
        "---\nname: example-skill\ndescription: Task\n---\n",
    ] {
        assert!(metadata(text, "example-skill").is_err(), "accepted {text}");
    }
}

#[test]
fn invalid_discovery_names_and_description_lengths_are_rejected() {
    for name in [
        "Example",
        "-example",
        "example--skill",
        "example-",
        "synced",
    ] {
        let text = format!("---\nname: {name}\ndescription: Task\n---\n# Body\n");
        assert!(metadata(&text, name).is_err());
    }
    for description in [String::new(), "x".repeat(1025)] {
        let text = format!("---\nname: example\ndescription: '{description}'\n---\n# Body\n");
        assert!(metadata(&text, "example").is_err());
    }
}

#[test]
fn relative_resources_resolve_and_fenced_examples_are_not_links() -> Result<()> {
    let root = tempfile::tempdir()?;
    let file = root.path().join("SKILL.md");
    fs::write(root.path().join("reference.md"), "# Reference\n")?;
    let text = "[Reference](reference.md#subject)\n\n```md\n[Sample](missing.md)\n```\n[Official](https://example.com/manual)\n";
    validate_links(text, &file, root.path())
}

#[test]
fn broken_or_escaping_local_resources_are_rejected() -> Result<()> {
    let root = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    fs::write(outside.path().join("reference.md"), "# External\n")?;
    let file = root.path().join("SKILL.md");
    assert!(validate_links("[Broken](missing.md)", &file, root.path()).is_err());
    let escaped = format!(
        "[Escape]({})",
        outside.path().join("reference.md").display()
    );
    assert!(validate_links(&escaped, &file, root.path()).is_err());
    Ok(())
}

fn tree() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir()?;
    fs::create_dir_all(root.path().join("home/dot_agents/skills/example"))?;
    fs::create_dir_all(root.path().join("home/dot_claude/skills"))?;
    fs::write(
        root.path().join("home/dot_agents/skills/example/SKILL.md"),
        "---\nname: example\ndescription: Task\n---\n# Body\n",
    )?;
    fs::write(
        root.path().join("home/dot_claude/skills/symlink_example"),
        "../../.agents/skills/example\n",
    )?;
    Ok(root)
}

#[test]
fn discovery_aliases_are_complete_and_cannot_drift() -> Result<()> {
    let root = tree()?;
    assert_eq!(validate_tree(root.path())?, 1);
    let alias = root.path().join("home/dot_claude/skills/symlink_example");
    fs::write(&alias, "/tmp/unrelated-source\n")?;
    assert!(validate_tree(root.path()).is_err());
    fs::remove_file(&alias)?;
    assert!(validate_tree(root.path()).is_err());
    fs::write(&alias, "../../.agents/skills/example\n")?;
    fs::create_dir(root.path().join("home/dot_claude/skills/independent-copy"))?;
    assert!(validate_tree(root.path()).is_err());
    Ok(())
}

#[test]
fn check_snapshots_preserve_sources_and_exclude_builds_and_unrelated_state() -> Result<()> {
    let root = tree()?;
    fs::create_dir_all(root.path().join("xtask/src"))?;
    fs::create_dir_all(root.path().join("xtask/target"))?;
    fs::write(
        root.path().join("xtask/src/lib.rs"),
        "pub fn example() {}\n",
    )?;
    fs::write(root.path().join("xtask/target/build"), "build bytes")?;
    fs::write(root.path().join("mise.toml"), "[tools]\n")?;
    fs::create_dir_all(root.path().join("crates/dotguard/src"))?;
    fs::create_dir_all(root.path().join("crates/dotguard/target"))?;
    fs::write(
        root.path().join("crates/dotguard/src/lib.rs"),
        "pub fn guard() {}\n",
    )?;
    fs::write(
        root.path().join("crates/dotguard/target/build"),
        "private build bytes",
    )?;
    for relative in [
        "Cargo.toml",
        "Cargo.lock",
        "clippy.toml",
        ".chezmoiroot",
        "home/.chezmoiignore",
        "lefthook.yml",
        ".github/workflows/ci.yml",
        "home/dot_config/skill-ops/policy.json",
        "home/dot_config/opencode/plugins/skill-ops.ts",
        "home/dot_config/opencode/plugins/line-endings.ts",
        "docs/skills/decisions/alpha.json",
        "package.json",
        "bun.lock",
        "tsconfig.json",
    ] {
        let path = root.path().join(relative);
        fs::create_dir_all(path.parent().expect("check artifact parent"))?;
        fs::write(path, "check configuration")?;
    }
    fs::write(root.path().join("unrelated-state"), "private")?;
    let parent = tempfile::tempdir()?;
    let output = parent.path().join("snapshot");
    export_checks(root.path(), &output)?;
    assert_eq!(
        fs::read(output.join("xtask/src/lib.rs"))?,
        fs::read(root.path().join("xtask/src/lib.rs"))?
    );
    assert!(!output.join("xtask/target").try_exists()?);
    assert!(!output.join("crates/dotguard/target").try_exists()?);
    assert_eq!(
        fs::read(output.join("crates/dotguard/src/lib.rs"))?,
        b"pub fn guard() {}\n"
    );
    assert!(!output.join("unrelated-state").try_exists()?);
    assert_eq!(
        fs::read(output.join("docs/skills/decisions/alpha.json"))?,
        fs::read(root.path().join("docs/skills/decisions/alpha.json"))?
    );
    assert!(export_checks(root.path(), &output).is_err());
    assert_eq!(validate_tree(&output)?, 1);
    Ok(())
}

#[test]
fn child_checks_cannot_overwrite_the_running_task_executable() -> Result<()> {
    let root = tempfile::tempdir()?;
    let cache = tempfile::tempdir()?;
    assert_eq!(
        xtask::check_build_dir(root.path(), Some(cache.path())),
        cache.path().join("checks")
    );
    assert_eq!(
        xtask::check_build_dir(root.path(), Some(std::path::Path::new("relative-target"))),
        root.path().join("relative-target/checks")
    );
    assert_eq!(
        xtask::check_build_dir(root.path(), None),
        root.path().join("target/checks")
    );
    Ok(())
}
