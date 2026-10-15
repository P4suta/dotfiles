use anyhow::{Context, Result, ensure};
use model::Profile;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

/// The seconds a client waits for a skill-ops hook before it gives up on it.
const HOOK_TIMEOUT_SECONDS: u64 = 10;
const TARGETS_DATA: &str = "home/.chezmoidata/targets.toml";
const PROFILES_DATA: &str = "home/.chezmoidata/profiles.toml";
const SKILL_TARGET_PREFIX: &str = ".agents/skills/";

/// Registers the skill-ops observe and stop hooks in a Codex or Claude settings value, replacing earlier skill-ops handlers.
///
/// # Errors
///
/// Returns an error when `client` names neither `codex` nor `claude`, or when the settings, `hooks`, a hook event, or a hook group lack the expected JSON shape.
pub fn merge_hooks(value: Value, client: &str) -> Result<Value> {
    ensure!(matches!(client, "codex" | "claude"), "unknown hook client");
    let mut value = remove_hooks(value)?;
    let object = value
        .as_object_mut()
        .context("client settings must be a JSON object")?;
    let hooks = object
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("native hooks must be an object")?;
    for (event, action) in [
        ("PostToolUse", format!("observe --client {client}")),
        ("Stop", "stop".into()),
    ] {
        let entries = hooks
            .entry(event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .context("native hook event must be an array")?;
        let expected = if event == "PostToolUse" {
            json!({"matcher":"Read|read|Skill|skill|Bash|bash|exec_command", "hooks":[{"type":"command", "command":format!("skill-ops {action}"),"timeout":HOOK_TIMEOUT_SECONDS}]})
        } else {
            json!({"hooks":[{"type":"command", "command":format!("skill-ops {action}"),"timeout":HOOK_TIMEOUT_SECONDS}]})
        };
        entries.push(expected);
    }
    Ok(value)
}

/// Removes only the owned skill-ops commands, preserving mixed hook groups.
///
/// # Errors
///
/// Returns an error when settings or hook groups have an invalid JSON shape.
pub fn remove_hooks(mut value: Value) -> Result<Value> {
    let object = value
        .as_object_mut()
        .context("client settings must be a JSON object")?;
    let Some(hooks) = object.get_mut("hooks") else {
        return Ok(value);
    };
    for entries in hooks
        .as_object_mut()
        .context("native hooks must be an object")?
        .values_mut()
    {
        let entries = entries
            .as_array_mut()
            .context("native hook event must be an array")?;
        let mut retained = Vec::new();
        for mut entry in std::mem::take(entries) {
            let handlers = entry["hooks"]
                .as_array_mut()
                .context("native hook group must contain handlers")?;
            let before = handlers.len();
            handlers.retain(|handler| {
                !handler["command"].as_str().is_some_and(|command| {
                    matches!(
                        command,
                        "skill-ops observe --client codex"
                            | "skill-ops observe --client claude"
                            | "skill-ops stop"
                    )
                })
            });
            if !handlers.is_empty() || handlers.len() == before {
                retained.push(entry);
            }
        }
        *entries = retained;
    }
    Ok(value)
}

/// Removes native skill-ops hook registrations without changing skill state.
///
/// # Errors
///
/// Returns an error if reading or updating an existing settings file fails.
pub fn remove_native_hooks(user_directory: &Path) -> Result<()> {
    for relative in [".codex/hooks.json", ".claude/settings.json"] {
        let path = user_directory.join(relative);
        if path.try_exists()? {
            let value: Value = crate::read_json(&path)?;
            let next = remove_hooks(value.clone())?;
            if next != value {
                crate::write_json(&path, &next, true)?;
            }
        }
    }
    Ok(())
}

/// Installs `binary` as skill-ops, records the approved catalog for `profile` and policy, and registers the client hooks.
///
/// # Errors
///
/// Returns an error when the source catalog fails its check, when the installed catalog or policy differs from the source, or when `binary` doesn't name a file.
/// Returns an error when copying the binary, writing the approval, or reading or writing a client settings file fails.
pub fn install(
    source: &Path,
    binary: &Path,
    user_directory: &Path,
    profile: Profile,
) -> Result<()> {
    use crate as ops;
    if !platform::incidents::enabled_at(user_directory) {
        return remove_native_hooks(user_directory);
    }
    ops::check_catalog(source)?;
    let expected = profile_catalog(source, profile)?;
    let installed = ops::catalog_owned(&user_directory.join(".agents/skills"), &expected)?;
    ensure!(
        expected == installed,
        "installed skill catalog differs; review and apply only the intended native paths"
    );
    let policy: ops::Policy =
        ops::read_json(&source.join("home/dot_config/skill-ops/policy.json"))?;
    let native_policy: ops::Policy =
        ops::read_json(&user_directory.join(".config/skill-ops/policy.json"))?;
    ensure!(
        policy == native_policy,
        "installed skill operations policy differs"
    );
    ensure!(binary.is_file(), "built skill-ops executable is missing");
    let directory = user_directory.join(".local/bin");
    fs::create_dir_all(&directory)?;
    let temporary = tempfile::NamedTempFile::new_in(&directory)?;
    fs::copy(binary, temporary.path())?;
    temporary.as_file().sync_all()?;
    let (_, staged) = temporary.keep()?;
    crate::aside::replace(
        &staged,
        &directory.join(format!("skill-ops{}", std::env::consts::EXE_SUFFIX)),
    )?;
    #[cfg(unix)]
    fs::File::open(&directory)?.sync_all()?;
    ops::write_json(
        &user_directory.join(".config/skill-ops/approved.json"),
        &ops::Approved {
            catalog_hash: ops::hash(&expected)?,
            policy_hash: ops::hash(&policy)?,
            engine_hash: ops::engine_hash()?,
            reviewed: ops::adoptable_revisions(
                &expected,
                &ops::tracked_decisions(&source.join(ops::DECISIONS))?,
            ),
            catalog: expected,
        },
        true,
    )?;
    for (client, path) in [
        ("codex", user_directory.join(".codex/hooks.json")),
        ("claude", user_directory.join(".claude/settings.json")),
    ] {
        let value: Value = if path.try_exists()? {
            ops::read_json(&path)?
        } else {
            json!({})
        };
        let next = merge_hooks(value.clone(), client)?;
        if next != value {
            ops::write_json(&path, &next, true)?;
        }
    }
    println!(
        "Installed native skill-ops and collector definitions; local observation history is preserved"
    );
    println!(
        "Verify the clients' normal hook trust and activation before claiming live collection"
    );
    Ok(())
}

/// Applies the skill-ops policy with `apply`, installs the runtime, and then applies the `OpenCode` adapter.
///
/// # Errors
///
/// Returns the error of `apply` or [`install`] at the stage that fails.
pub fn deploy(
    source: &Path,
    binary: &Path,
    user_directory: &Path,
    profile: Profile,
    mut apply: impl FnMut(&Path) -> Result<()>,
) -> Result<()> {
    use crate::skill_rules::{InstallationStage, installation_stage};
    if !platform::incidents::enabled_at(user_directory) {
        return remove_native_hooks(user_directory);
    }
    let mut policy_applied = false;
    let mut runtime_installed = false;
    loop {
        match installation_stage(policy_applied, runtime_installed) {
            InstallationStage::Policy => {
                apply(&source.join("home/dot_config/skill-ops/policy.json"))?;
                policy_applied = true;
            }
            InstallationStage::Runtime => {
                install(source, binary, user_directory, profile)?;
                runtime_installed = true;
            }
            InstallationStage::Adapter => {
                return apply(&source.join("home/dot_config/opencode/plugins/skill-ops.ts"));
            }
        }
    }
}

fn profile_catalog(source: &Path, profile: Profile) -> Result<crate::Catalog> {
    let targets = model::data::parse(
        TARGETS_DATA,
        &fs::read_to_string(source.join(TARGETS_DATA))?,
    )
    .map_err(anyhow::Error::msg)?
    .targets
    .context("skill installation requires target declarations")?;
    let profiles = model::data::parse(
        PROFILES_DATA,
        &fs::read_to_string(source.join(PROFILES_DATA))?,
    )
    .map_err(anyhow::Error::msg)?
    .profiles
    .context("skill installation requires profile sets")?;
    let mut expected = crate::catalog(&source.join("home/dot_agents/skills"))?;
    for (target, set) in targets {
        if let Some(name) = target.strip_prefix(SKILL_TARGET_PREFIX) {
            let members = profiles.get(&set).with_context(|| {
                format!("skill target {target} names unknown profile set {set}")
            })?;
            if !members.iter().any(|member| member == profile.name()) {
                expected.skills.remove(name);
            }
        }
    }
    let available: std::collections::BTreeSet<_> = expected.skills.keys().cloned().collect();
    for skill in expected.skills.values_mut() {
        skill.references.retain(|name| available.contains(name));
    }
    Ok(expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_installation_removes_only_owned_handlers_without_assessing_skills() -> Result<()> {
        let home = tempfile::tempdir()?;
        let marker = home.path().join(platform::incidents::DISABLED);
        fs::create_dir_all(marker.parent().unwrap())?;
        fs::write(&marker, "owner stop")?;
        let value = json!({
            "other": {"keep": true},
            "hooks": {
                "PreToolUse": [{"matcher":"Bash", "other":7, "hooks":[
                    {"type":"command", "command":"dotfiles claude-bash"},
                    {"type":"command", "command":"skill-ops observe --client claude"},
                    {"type":"command", "command":"skill-ops-custom stop"}
                ]}],
                "Stop": [{"hooks":[{"type":"command", "command":"skill-ops stop"}]}],
                "PostToolUse": [{"hooks":[{"type":"command", "command":"line-endings"}]}]
            }
        });
        let expected = json!({
            "other": {"keep": true},
            "hooks": {
                "PreToolUse": [{"matcher":"Bash", "other":7, "hooks":[
                    {"type":"command", "command":"dotfiles claude-bash"},
                    {"type":"command", "command":"skill-ops-custom stop"}
                ]}],
                "Stop": [],
                "PostToolUse": [{"hooks":[{"type":"command", "command":"line-endings"}]}]
            }
        });
        for relative in [".codex/hooks.json", ".claude/settings.json"] {
            crate::write_json(&home.path().join(relative), &value, true)?;
        }
        let history = home.path().join(".local/state/skill-ops/history.json");
        fs::create_dir_all(history.parent().unwrap())?;
        fs::write(&history, "unchanged history")?;
        let missing = home.path().join("missing source and binary");
        install(&missing, &missing, home.path(), Profile::Mac)?;
        deploy(&missing, &missing, home.path(), Profile::Mac, |_| {
            panic!("a disabled deployment cannot apply policy or adapters")
        })?;
        for relative in [".codex/hooks.json", ".claude/settings.json"] {
            let actual: Value = crate::read_json(&home.path().join(relative))?;
            assert_eq!(actual, expected);
            assert_eq!(remove_hooks(actual.clone())?, actual);
        }
        fs::remove_file(&marker)?;
        install(&missing, &missing, home.path(), Profile::Mac)?;
        deploy(&missing, &missing, home.path(), Profile::Mac, |_| {
            panic!("a default-off deployment cannot apply policy or adapters")
        })?;
        assert_eq!(fs::read_to_string(history)?, "unchanged history");
        assert!(!home.path().join(".config/skill-ops/approved.json").exists());
        assert!(!home.path().join(".local/bin").exists());
        assert!(remove_hooks(json!({"hooks":{"Stop":[{"hooks":false}]}})).is_err());
        Ok(())
    }

    #[test]
    fn profile_scope_excludes_only_declared_skills_and_preserves_missing_skill_errors() -> Result<()>
    {
        let fixture = tempfile::tempdir()?;
        let source = fixture.path().join("source");
        let native = fixture.path().join("native");
        for name in ["common", "mac-only"] {
            let skill = source.join("home/dot_agents/skills").join(name);
            fs::create_dir_all(&skill)?;
            fs::write(
                skill.join("SKILL.md"),
                format!("---\nname: {name}\ndescription: Fixture skill.\n---\nRead `mac-only`.\n"),
            )?;
        }
        fs::create_dir_all(source.join("home/.chezmoidata"))?;
        fs::write(
            source.join("home/.chezmoidata/targets.toml"),
            "[targets]\n\".agents/skills/mac-only\" = \"mac\"\n",
        )?;
        fs::write(
            source.join("home/.chezmoidata/profiles.toml"),
            "[profiles]\nmac = [\"mac\"]\n",
        )?;
        fs::create_dir_all(native.join("common"))?;
        fs::copy(
            source.join("home/dot_agents/skills/common/SKILL.md"),
            native.join("common/SKILL.md"),
        )?;
        for profile in Profile::ALL {
            let expected = profile_catalog(&source, profile)?;
            if profile == Profile::Mac {
                assert!(expected.skills.contains_key("mac-only"));
                assert!(crate::catalog_owned(&native, &expected).is_err());
            } else {
                assert_eq!(expected.skills.len(), 1);
                assert_eq!(crate::catalog_owned(&native, &expected)?, expected);
            }
        }
        fs::remove_file(native.join("common/SKILL.md"))?;
        let expected = profile_catalog(&source, Profile::Windows)?;
        assert!(crate::catalog_owned(&native, &expected).is_err());
        Ok(())
    }
}
