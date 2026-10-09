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
    shell_caches(home, runner)?;
    agent_integrations(runner, local_gui);
    Ok(())
}

/// Writes the Nushell modules and the zsh caches that each tool generates for its shell initialization under `home`.
///
/// # Errors
///
/// Returns an error when a generator fails or a cache write fails.
pub fn shell_caches(home: &Path, runner: &mut impl Runner) -> Result<()> {
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
            if name == "atuin" {
                bytes = without_startup_index(&String::from_utf8(bytes)?).into_bytes();
            }
            replace(&target, &bytes, false)?;
        } else if !target.exists() {
            replace(&target, b"", false)?;
        }
    }
    zsh_caches(home, runner)
}

/// The directory of the zsh initialization caches, which `.zshrc` sources so a new shell starts no program.
pub const ZSH_CACHE: &str = ".cache/zsh";

/// Writes each tool's zsh initialization into [`ZSH_CACHE`], as the Windows setup caches the PowerShell one.
/// An absent tool keeps its earlier cache, and `.zshrc` skips a cache that doesn't exist.
fn zsh_caches(home: &Path, runner: &mut impl Runner) -> Result<()> {
    let cache = home.join(ZSH_CACHE);
    for (name, tool, arguments) in [
        ("mise", Tool::Mise, args(&["activate", "zsh"])),
        (
            "starship",
            Tool::Starship,
            args(&["init", "zsh", "--print-full-init"]),
        ),
        ("zoxide", Tool::Zoxide, args(&["init", "zsh"])),
        (
            "atuin",
            Tool::Atuin,
            args(&["init", "zsh", "--disable-up-arrow"]),
        ),
        ("fzf", Tool::Fzf, args(&["--zsh"])),
        (
            "ls-colors",
            Tool::Vivid,
            args(&["generate", "tokyonight-night"]),
        ),
    ] {
        if !runner.available(tool) {
            continue;
        }
        let text = String::from_utf8(checked(runner, tool, &arguments)?)?;
        let script = match name {
            "mise" => deferred_mise(&text),
            "ls-colors" => {
                ensure!(
                    !text.contains('\''),
                    "a quote in the generated colors would end the zsh string"
                );
                format!("export LS_COLORS='{}'\n", text.trim_end())
            }
            _ => text,
        };
        replace(&cache.join(format!("{name}.zsh")), script.as_bytes(), false)?;
    }
    Ok(())
}

/// Atuin's Nushell initialization without the top-level block that starts `atuin __internal prepare-search-index` in a background job as the shell starts.
/// Atuin builds its search index on the first search instead, and the hooks that run at each prompt stay.
#[must_use]
pub fn without_startup_index(initialization: &str) -> String {
    let mut kept = String::new();
    let mut block: Vec<&str> = Vec::new();
    for line in initialization.lines() {
        if block.is_empty() && !line.starts_with("if ") {
            kept.push_str(line);
            kept.push('\n');
            continue;
        }
        block.push(line);
        if line == "}" {
            if !block
                .iter()
                .any(|line| line.contains("prepare-search-index"))
            {
                for line in &block {
                    kept.push_str(line);
                    kept.push('\n');
                }
            }
            block.clear();
        }
    }
    for line in block {
        kept.push_str(line);
        kept.push('\n');
    }
    kept
}

/// The mise activation without the lines that run `mise hook-env` and pin the generating process's `PATH` as the shell sources it.
/// The first prompt's hook then computes the environment from the shell's own `PATH`.
#[must_use]
pub fn deferred_mise(activation: &str) -> String {
    activation
        .lines()
        .filter(|line| {
            *line != "_mise_hook --force"
                && !line.starts_with("export PATH='")
                && !line.starts_with("export __MISE_ORIG_PATH='")
                && !line.starts_with("export __MISE_ZSH_ACTIVATE_PATH=")
                && !line.starts_with("export __MISE_ZSH_ACTIVATE_ENV=")
        })
        .fold(String::new(), |mut kept, line| {
            kept.push_str(line);
            kept.push('\n');
            kept
        })
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
    fn the_atuin_module_starts_no_index_job_but_keeps_its_prompt_hooks() {
        let initialization = "if 'ATUIN_SESSION' not-in $env {\n    $env.ATUIN_SESSION = (random uuid -v 7)\n}\nlet _atuin_pre_prompt = {||\n    job spawn { ^atuin history end } | ignore\n}\nif (version).minor >= 104 {\n    with-env { ATUIN_SHELL: nu } {\n        job spawn {\n            atuin __internal prepare-search-index | complete\n        } | ignore\n    }\n}\n$env.config = ($env.config | default [] keybindings)\n";
        assert_eq!(
            without_startup_index(initialization),
            "if 'ATUIN_SESSION' not-in $env {\n    $env.ATUIN_SESSION = (random uuid -v 7)\n}\nlet _atuin_pre_prompt = {||\n    job spawn { ^atuin history end } | ignore\n}\n$env.config = ($env.config | default [] keybindings)\n"
        );
    }

    #[test]
    fn the_zsh_cache_defers_the_mise_hook_to_the_first_prompt() {
        let activation = "export __MISE_ORIG_PATH='/baked'\nexport PATH='/baked'\nif [ -z \"${__MISE_ORIG_PATH:-}\" ]; then\n  export __MISE_ORIG_PATH=\"$PATH\"\nfi\nadd-zsh-hook precmd _mise_hook_precmd\n\n_mise_hook --force\nexport __MISE_ZSH_ACTIVATE_PATH=\"$PATH\"\nexport __MISE_ZSH_ACTIVATE_ENV=\"$(_mise_hook_env_state)\"\n";
        let scope = tempfile::tempdir().unwrap();
        let mut fake = Fake {
            replies: VecDeque::from([Reply {
                success: true,
                bytes: activation.as_bytes().to_vec(),
            }]),
            calls: Vec::new(),
        };
        zsh_caches(scope.path(), &mut fake).unwrap();
        assert_eq!(
            fs::read_to_string(scope.path().join(ZSH_CACHE).join("mise.zsh")).unwrap(),
            "if [ -z \"${__MISE_ORIG_PATH:-}\" ]; then\n  export __MISE_ORIG_PATH=\"$PATH\"\nfi\nadd-zsh-hook precmd _mise_hook_precmd\n\n"
        );
        assert_eq!(fake.calls, [["mise", "activate", "zsh"]]);
        assert!(!scope.path().join(ZSH_CACHE).join("starship.zsh").exists());
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
