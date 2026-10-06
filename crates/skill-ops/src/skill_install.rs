use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

/// The seconds a client waits for a skill-ops hook before it gives up on it.
const HOOK_TIMEOUT_SECONDS: u64 = 10;

/// Registers the skill-ops observe and stop hooks in a Codex or Claude settings value, replacing earlier skill-ops handlers.
///
/// # Errors
///
/// Returns an error when `client` names neither `codex` nor `claude`, or when the settings, `hooks`, a hook event, or a hook group lack the expected JSON shape.
pub fn merge_hooks(mut value: Value, client: &str) -> Result<Value> {
    ensure!(matches!(client, "codex" | "claude"), "unknown hook client");
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
        retained.push(expected);
        *entries = retained;
    }
    Ok(value)
}

/// Installs `binary` as skill-ops, records the approved catalog and policy, and registers the client hooks.
///
/// # Errors
///
/// Returns an error when the source catalog fails its check, when the installed catalog or policy differs from the source, or when `binary` doesn't name a file.
/// Returns an error when copying the binary, writing the approval, or reading or writing a client settings file fails.
pub fn install(source: &Path, binary: &Path, user_directory: &Path) -> Result<()> {
    use crate as ops;
    ops::check_catalog(source)?;
    let expected = ops::catalog(&source.join("home/dot_agents/skills"))?;
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
    mut apply: impl FnMut(&Path) -> Result<()>,
) -> Result<()> {
    use crate::skill_rules::{InstallationStage, installation_stage};
    let mut policy_applied = false;
    let mut runtime_installed = false;
    loop {
        match installation_stage(policy_applied, runtime_installed) {
            InstallationStage::Policy => {
                apply(&source.join("home/dot_config/skill-ops/policy.json"))?;
                policy_applied = true;
            }
            InstallationStage::Runtime => {
                install(source, binary, user_directory)?;
                runtime_installed = true;
            }
            InstallationStage::Adapter => {
                return apply(&source.join("home/dot_config/opencode/plugins/skill-ops.ts"));
            }
        }
    }
}
