//! The shell integrations each tool generates and the agent integrations a host installs.

use crate::runtime::{Runner, args, checked, optional, replace};
use crate::tool::Tool;
use anyhow::{Result, ensure};
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
struct Plugins {
    installed: Vec<Plugin>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Plugin {
    plugin_id: String,
}
#[derive(Deserialize)]
struct Marketplaces {
    marketplaces: Vec<Marketplace>,
}
#[derive(Deserialize)]
struct Marketplace {
    name: String,
}

pub(crate) fn context7(runner: &mut impl Runner, local_gui: bool) -> Result<()> {
    if !crate::profile_rules::context7_needed(local_gui, runner.available(Tool::Codex), false) {
        return Ok(());
    }
    let plugins: Plugins = serde_json::from_slice(&checked(
        runner,
        Tool::Codex,
        &args(&["plugin", "list", "--json"]),
    )?)?;
    let installed = plugins
        .installed
        .iter()
        .any(|plugin| plugin.plugin_id == "context7@context7-marketplace");
    if !crate::profile_rules::context7_needed(local_gui, true, installed) {
        return Ok(());
    }
    let marketplaces: Marketplaces = serde_json::from_slice(&checked(
        runner,
        Tool::Codex,
        &args(&["plugin", "marketplace", "list", "--json"]),
    )?)?;
    if !marketplaces
        .marketplaces
        .iter()
        .any(|marketplace| marketplace.name == "context7-marketplace")
    {
        checked(
            runner,
            Tool::Codex,
            &args(&["plugin", "marketplace", "add", "upstash/context7", "--json"]),
        )?;
    }
    checked(
        runner,
        Tool::Codex,
        &args(&["plugin", "add", "context7@context7-marketplace", "--json"]),
    )?;
    Ok(())
}

pub(crate) fn integrations(home: &Path, runner: &mut impl Runner, local_gui: bool) -> Result<()> {
    let generated = home.join(".config/nushell/generated");
    for (name, tool, arguments) in [
        ("mise", Tool::Mise, args(&["activate", "nu"])),
        ("starship", Tool::Starship, args(&["init", "nu"])),
        ("television", Tool::Television, args(&["init", "nu"])),
        (
            "atuin",
            Tool::Atuin,
            args(&["init", "nu", "--disable-up-arrow"]),
        ),
        ("zoxide", Tool::Zoxide, args(&["init", "nushell"])),
        (
            "ls-colors",
            Tool::Vivid,
            args(&["generate", "tokyonight-night"]),
        ),
    ] {
        let target = generated.join(format!("{name}.nu"));
        if runner.available(tool) {
            let mut bytes = checked(runner, tool, &arguments)?;
            if name == "ls-colors" {
                let colors = String::from_utf8(bytes)?;
                ensure!(
                    !colors.contains("'#"),
                    "unsafe Nushell raw-string delimiter in generated colors"
                );
                bytes = format!("$env.LS_COLORS = r#'{}'#\n", colors.trim_end()).into_bytes();
            }
            replace(&target, &bytes, false)?;
        } else if !target.exists() {
            replace(&target, b"", false)?;
        }
    }
    agent_integrations(runner, local_gui);
    Ok(())
}

/// Installs the herdr integration of each available agent and sets up Context7 for Codex, printing each failure as a warning.
pub(crate) fn agent_integrations(runner: &mut impl Runner, local_gui: bool) {
    if runner.available(Tool::Herdr) {
        for agent in [Tool::Claude, Tool::Codex, Tool::Opencode] {
            if runner.available(agent) {
                optional(
                    runner,
                    Tool::Herdr,
                    &["integration", "install", agent.program()],
                );
            }
        }
    }
    if let Err(error) = context7(runner, local_gui) {
        eprintln!("Warning: Context7 setup failed: {error:#}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::Reply;
    use anyhow::Context;
    use std::collections::VecDeque;
    use std::ffi::OsString;
    use std::fs;

    struct Fake {
        replies: VecDeque<Reply>,
        calls: Vec<Vec<OsString>>,
    }
    impl Runner for Fake {
        fn available(&self, tool: Tool) -> bool {
            matches!(tool, Tool::Codex | Tool::Mise)
        }
        fn run(&mut self, tool: Tool, arguments: &[OsString]) -> Result<Reply> {
            self.calls.push(
                std::iter::once(tool.program().into())
                    .chain(arguments.iter().cloned())
                    .collect(),
            );
            self.replies.pop_front().context("unexpected command")
        }
        fn run_installed(&mut self, program: &Path, _: &[OsString]) -> Result<Reply> {
            anyhow::bail!("unexpected installed program {}", program.display())
        }
    }

    #[test]
    fn context7_is_local_idempotent_and_stops_on_invalid_or_failed_inventory() {
        for bytes in [br"{}".as_slice(), br#"{"installed":{}}"#, b"not JSON"] {
            let mut fake = Fake {
                replies: VecDeque::from([Reply {
                    success: true,
                    bytes: bytes.to_vec(),
                }]),
                calls: Vec::new(),
            };
            context7(&mut fake, false).unwrap();
            assert_eq!(fake.calls, Vec::<Vec<std::ffi::OsString>>::new());
            assert!(context7(&mut fake, true).is_err());
            assert_eq!(fake.calls.len(), 1);
        }
        let mut fake = Fake {
            replies: VecDeque::from([Reply {
                success: true,
                bytes: br#"{"installed":[{"pluginId":"context7@context7-marketplace"}]}"#.to_vec(),
            }]),
            calls: Vec::new(),
        };
        context7(&mut fake, true).unwrap();
        assert_eq!(fake.calls.len(), 1);
        fake.replies.push_back(Reply {
            success: false,
            bytes: Vec::new(),
        });
        assert!(context7(&mut fake, true).is_err());
        assert_eq!(fake.calls.len(), 2);
    }

    #[test]
    fn failed_generators_preserve_previous_output_and_missing_tools_do_not_erase_it() {
        let scope = tempfile::tempdir().unwrap();
        let target = scope.path().join(".config/nushell/generated/mise.nu");
        replace(&target, b"previous", false).unwrap();
        let mut fake = Fake {
            replies: VecDeque::from([Reply {
                success: false,
                bytes: b"partial".to_vec(),
            }]),
            calls: Vec::new(),
        };
        assert!(integrations(scope.path(), &mut fake, false).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"previous");
    }
}
