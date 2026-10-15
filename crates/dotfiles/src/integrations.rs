//! The shell integrations each tool generates and the agent integrations a host installs.

use crate::runtime::{
    Runner, args, checked, initialization, initialization_for, optional, replace,
};
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
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
            let mut bytes = initialization(runner, tool, &arguments)?;
            if name == "ls-colors" {
                let colors = String::from_utf8(bytes)?;
                ensure!(
                    !colors.contains("'#"),
                    "unsafe Nushell raw-string delimiter in generated colors"
                );
                bytes = format!("$env.LS_COLORS = r#'{}'#\n", colors.trim_end()).into_bytes();
            }
            if name == "atuin" {
                bytes = without_startup_index(&String::from_utf8(bytes)?)?.into_bytes();
            }
            if tool == Tool::Starship {
                bytes = cached_continuation(
                    runner,
                    rules::shell_cache_rules::Shell::Nu,
                    &String::from_utf8(bytes)?,
                )?
                .into_bytes();
            }
            replace(&target, &bytes, false)?;
        } else if !target.exists() {
            replace(&target, b"", false)?;
        }
    }
    zsh_caches(home, runner)?;
    pwsh_caches(home, runner)
}

/// The directory of the zsh initialization caches, which `.zshrc` sources so a new shell starts no program.
pub const ZSH_CACHE: &str = ".cache/zsh";

/// Writes each tool's zsh initialization into [`ZSH_CACHE`], as the Windows setup caches the PowerShell one.
/// An absent tool keeps its earlier cache, and `.zshrc` skips a cache that doesn't exist.
fn zsh_caches(home: &Path, runner: &mut impl Runner) -> Result<()> {
    let cache = home.join(ZSH_CACHE);
    for (name, tool, arguments) in [
        ("brew", Tool::Brew, args(&["shellenv"])),
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
        let text = String::from_utf8(initialization(runner, tool, &arguments)?)?;
        let script = match name {
            "mise" => deferred_mise(&text),
            "atuin" => without_atuin_zsh_index(&text),
            "starship" => cached_continuation(runner, rules::shell_cache_rules::Shell::Zsh, &text)?,
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

fn continuation_literal(shell: rules::shell_cache_rules::Shell, text: &str) -> Result<String> {
    use rules::shell_cache_rules::{Escape, Shell, literal_escape};
    use std::fmt::Write as _;
    let mut literal = String::from(match shell {
        Shell::Nu => "\"",
        Shell::Zsh => "$'",
    });
    for character in text.chars() {
        match literal_escape(shell, character) {
            Escape::Verbatim => literal.push(character),
            Escape::Quote | Escape::Slash => {
                literal.push('\\');
                literal.push(character);
            }
            Escape::Control => match shell {
                Shell::Nu => write!(literal, "\\u{{{:x}}}", u32::from(character))?,
                Shell::Zsh => write!(literal, "\\u{:04x}", u32::from(character))?,
            },
        }
    }
    literal.push(match shell {
        Shell::Nu => '"',
        Shell::Zsh => '\'',
    });
    Ok(literal)
}

fn cached_continuation(
    runner: &mut impl Runner,
    shell: rules::shell_cache_rules::Shell,
    script: &str,
) -> Result<String> {
    use rules::shell_cache_rules::Shell;
    let span = rules::shell_cache_rules::continuation(shell, script.as_bytes())
        .context("unknown or repeated Starship continuation assignment")?;
    let prompt = String::from_utf8(initialization_for(
        runner,
        Tool::Starship,
        &args(&["prompt", "--continuation"]),
        match shell {
            Shell::Nu => "nu",
            Shell::Zsh => "zsh",
        },
    )?)?;
    let end = rules::shell_cache_rules::prompt_end(shell, prompt.as_bytes());
    let prompt = prompt.get(..end).context("continuation prompt boundary")?;
    let literal = continuation_literal(shell, prompt)?;
    let assignment = match shell {
        Shell::Nu => format!("    PROMPT_MULTILINE_INDICATOR: {literal}\n"),
        Shell::Zsh => format!("PROMPT2={literal}\n"),
    };
    let before = script
        .get(..span.start)
        .context("continuation start boundary")?;
    let after = script
        .get(span.end..)
        .context("continuation end boundary")?;
    Ok(format!("{before}{assignment}{after}"))
}

fn pwsh_caches(home: &Path, runner: &mut impl Runner) -> Result<()> {
    for (name, tool, arguments) in [
        ("mise", Tool::Mise, args(&["activate", "pwsh"])),
        (
            "starship",
            Tool::Starship,
            args(&["init", "powershell", "--print-full-init"]),
        ),
        ("zoxide", Tool::Zoxide, args(&["init", "powershell"])),
    ] {
        if runner.available(tool) {
            let text = String::from_utf8(initialization(runner, tool, &arguments)?)?;
            let script = if name == "mise" {
                deferred_mise(&text)
            } else {
                text
            };
            replace(
                &home.join(format!(".cache/pwsh/{name}.ps1")),
                script.as_bytes(),
                false,
            )?;
        }
    }
    Ok(())
}

/// Atuin's Nushell initialization without the top-level block that starts `atuin __internal prepare-search-index` in a background job as the shell starts.
/// Atuin builds its search index on the first search instead, and the hooks that run at each prompt stay.
///
/// # Errors
///
/// Returns an error if a selected job doesn't end at a character boundary.
pub fn without_startup_index(initialization: &str) -> Result<String> {
    let Some(span) = rules::shell_cache_rules::startup_index(initialization.as_bytes()) else {
        return Ok(initialization.to_owned());
    };
    let before = initialization
        .get(..span.start)
        .context("the index job starts at a character boundary")?;
    let after = initialization
        .get(span.end..)
        .context("the index job ends at a character boundary")?;
    Ok(format!("{before}{after}"))
}

/// The mise activation without the lines that run `mise hook-env` and pin the generating process's `PATH` as the shell sources it.
/// The first prompt's hook then computes the environment from the shell's own `PATH`.
#[must_use]
pub fn deferred_mise(activation: &str) -> String {
    kept_lines(activation, rules::shell_cache_rules::keep_mise_line)
}

/// Atuin's zsh initialization with its detached index job deferred to the first search.
#[must_use]
pub fn without_atuin_zsh_index(initialization: &str) -> String {
    kept_lines(initialization, rules::shell_cache_rules::keep_atuin_line)
}

fn kept_lines(initialization: &str, keep: fn(&[u8]) -> bool) -> String {
    initialization
        .lines()
        .filter(|line| keep(line.as_bytes()))
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
        let initialization = "if 'ATUIN_SESSION' not-in $env {\n    $env.ATUIN_SESSION = (random uuid -v 7)\n}\nlet _atuin_pre_prompt = {||\n    job spawn { ^atuin history end } | ignore\n}\nif (version).minor >= 104 or (version).major > 0 {\n    with-env { ATUIN_SHELL: nu } {\n        job spawn {\n            atuin __internal prepare-search-index | complete\n        } | ignore\n    }\n}\n$env.config = ($env.config | default [] keybindings)\n";
        assert_eq!(
            without_startup_index(initialization).unwrap(),
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

    #[derive(Default)]
    struct Starship {
        failed_prompt: bool,
        calls: Vec<Vec<OsString>>,
        shells: Vec<String>,
    }

    impl Runner for Starship {
        fn available(&self, tool: Tool) -> bool {
            tool == Tool::Starship
        }

        fn run(&mut self, tool: Tool, arguments: &[OsString]) -> Result<Reply> {
            assert_eq!(tool, Tool::Starship);
            self.calls.push(arguments.to_vec());
            let bytes = match arguments.first().and_then(|value| value.to_str()) {
                Some("prompt") => b"quote ' \" $() `backtick` \\ newline\n'#\x1b[0m".to_vec(),
                Some("init") if arguments.get(1).is_some_and(|value| value == "nu") => {
                    b"export-env { load-env {\n    PROMPT_MULTILINE_INDICATOR: (\n        ^'/native path/starship' prompt --continuation\n    )\n    PROMPT_COMMAND: {|| ^starship prompt }\n}}\n".to_vec()
                }
                Some("init") if arguments.get(1).is_some_and(|value| value == "zsh") => {
                    b"PROMPT='$('/native path/starship' prompt)'\nPROMPT2=\"$(/native/path/starship prompt --continuation)\"\n".to_vec()
                }
                Some("init") => b"unchanged powershell initialization\n".to_vec(),
                other => anyhow::bail!("unexpected Starship command {other:?}"),
            };
            Ok(Reply {
                success: !(self.failed_prompt
                    && arguments.first().is_some_and(|value| value == "prompt")),
                bytes,
            })
        }

        fn run_installed(&mut self, program: &Path, _: &[OsString]) -> Result<Reply> {
            anyhow::bail!("unexpected installed program {}", program.display())
        }

        fn initialization_for(
            &mut self,
            tool: Tool,
            arguments: &[OsString],
            shell: &str,
        ) -> Result<Reply> {
            self.shells.push(shell.to_owned());
            self.initialization(tool, arguments)
        }
    }

    #[test]
    fn starship_continuation_is_computed_when_caches_are_generated() {
        let home = tempfile::tempdir().unwrap();
        let mut starship = Starship {
            failed_prompt: false,
            calls: Vec::new(),
            ..Starship::default()
        };
        shell_caches(home.path(), &mut starship).unwrap();
        let nu =
            fs::read_to_string(home.path().join(".config/nushell/generated/starship.nu")).unwrap();
        let zsh = fs::read_to_string(home.path().join(ZSH_CACHE).join("starship.zsh")).unwrap();
        assert!(!nu.contains(" prompt --continuation"));
        assert!(!zsh.contains(" prompt --continuation"));
        assert!(nu.contains("PROMPT_COMMAND: {|| ^starship prompt }"));
        assert!(zsh.contains("PROMPT='$('/native path/starship' prompt)'"));
        assert_eq!(
            starship
                .calls
                .iter()
                .filter(|call| call.first().is_some_and(|value| value == "prompt"))
                .count(),
            2
        );
        assert_eq!(starship.shells, ["nu", "zsh"]);
        assert_eq!(
            fs::read_to_string(home.path().join(".cache/pwsh/starship.ps1")).unwrap(),
            "unchanged powershell initialization\n"
        );
    }

    #[test]
    fn a_failed_continuation_query_preserves_the_previous_cache() {
        let home = tempfile::tempdir().unwrap();
        let cache = home.path().join(".config/nushell/generated/starship.nu");
        replace(&cache, b"previous successful cache", false).unwrap();
        let mut starship = Starship {
            failed_prompt: true,
            calls: Vec::new(),
            ..Starship::default()
        };
        assert!(shell_caches(home.path(), &mut starship).is_err());
        assert_eq!(fs::read(cache).unwrap(), b"previous successful cache");
    }

    #[test]
    fn unknown_and_repeated_continuation_shapes_never_query_or_replace() {
        use rules::shell_cache_rules::Shell;
        let mut starship = Starship::default();
        for (shell, script) in [
            (
                Shell::Nu,
                "PROMPT_MULTILINE_INDICATOR: {|| ^starship prompt }\n",
            ),
            (
                Shell::Zsh,
                "PROMPT2=\"$(/native/starship prompt --unknown)\"\n",
            ),
            (
                Shell::Zsh,
                "PROMPT2=\"$(/native/starship prompt --continuation)\"\nPROMPT2=\"$(/native/starship prompt --continuation)\"\n",
            ),
        ] {
            assert!(cached_continuation(&mut starship, shell, script).is_err());
            assert_eq!(starship.calls, Vec::<Vec<OsString>>::new());
        }
    }

    fn prompt_literal_data() -> String {
        let mut text: String = ('\0'..='\u{1f}').chain('\u{7f}'..='\u{9f}').collect();
        text.push_str("quote ' \" $() `backtick` \\ '# delimiter 日 😀 \u{2028} newline\n\n");
        text
    }

    #[test]
    fn nushell_reads_cached_prompt_data_without_expanding_commands() -> Result<()> {
        use rules::shell_cache_rules::Shell;
        let text = prompt_literal_data();
        let literal = continuation_literal(Shell::Nu, &text)?;
        let output = Tool::Nu
            .command()
            .args(["--no-config-file", "-c"])
            .arg(format!("{literal} | to json --raw"))
            .output()?;
        ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual: String = serde_json::from_slice(&output.stdout)?;
        assert_eq!(actual, text);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn zsh_reads_cached_prompt_data_without_expanding_commands() -> Result<()> {
        use rules::shell_cache_rules::Shell;
        let text = prompt_literal_data();
        let literal = continuation_literal(Shell::Zsh, &text)?;
        let output = Tool::Zsh
            .command()
            .args(["-f", "-c"])
            .arg(format!("PROMPT2={literal}; print -rn -- \"$PROMPT2\""))
            .output()?;
        ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, text.as_bytes());
        Ok(())
    }
}
