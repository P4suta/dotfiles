use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::tool::Tool;

pub struct Reply {
    pub success: bool,
    pub bytes: Vec<u8>,
}

pub trait Runner {
    fn available(&self, tool: Tool) -> bool;
    fn run(&mut self, tool: Tool, arguments: &[OsString]) -> Result<Reply>;
    /// Run a binary this repository installed at a known path, such as a bundle executable it just verified.
    fn run_installed(&mut self, program: &Path, arguments: &[OsString]) -> Result<Reply>;
}

pub struct Native {
    pub home: PathBuf,
    pub path: OsString,
    pub environment: Vec<(OsString, OsString)>,
    pub directory: Option<PathBuf>,
}

impl Native {
    pub fn new(home: &Path) -> Result<Self> {
        let mut paths = vec![
            home.join(".local/bin"),
            home.join(".cargo/bin"),
            home.join(".nix-profile/bin"),
            home.join(if cfg!(windows) {
                "AppData/Local/mise/shims"
            } else {
                ".local/share/mise/shims"
            }),
        ];
        if cfg!(target_os = "macos") {
            paths.extend(["/opt/homebrew/bin".into(), "/opt/homebrew/sbin".into()]);
        }
        if cfg!(windows) {
            paths.extend([
                home.join("scoop/shims"),
                home.join("AppData/Local/Programs/Herdr/bin"),
            ]);
        }
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        paths.retain(|path| !path.ends_with(".local/share/dotfiles/bin"));
        Ok(Self {
            home: home.to_path_buf(),
            path: std::env::join_paths(paths)?,
            environment: Vec::new(),
            directory: None,
        })
    }

    pub fn command(&self, tool: Tool) -> Command {
        self.prepare(tool.command(), tool == Tool::Codex)
    }

    /// A Git hook gate, which keeps the hook's repository variables to inspect the commit in progress.
    pub fn hook_command(&self, tool: Tool) -> Command {
        self.prepare(tool.hook_command(), tool == Tool::Codex)
    }

    /// A binary this repository installed at a known path, run with the same environment as registered tools.
    pub fn command_installed(&self, program: &Path) -> Command {
        self.prepare(crate::tool::external(program), false)
    }

    fn prepare(&self, mut command: Command, codex: bool) -> Command {
        if cfg!(windows) && codex {
            command.env("GIT_CONFIG_GLOBAL", "NUL");
        }
        command
            .env("PATH", &self.path)
            .env("MISE_AUTO_INSTALL", "false")
            .envs(self.environment.iter().cloned());
        if let Some(directory) = &self.directory {
            command.current_dir(directory);
        }
        command
    }
}

impl Runner for Native {
    fn available(&self, tool: Tool) -> bool {
        let name = tool.program();
        if Path::new(name).is_absolute() {
            return executable(Path::new(name));
        }
        std::env::split_paths(&self.path).any(|directory| {
            executable(&directory.join(name))
                || (cfg!(windows)
                    && ["exe", "cmd", "bat"]
                        .iter()
                        .any(|extension| directory.join(format!("{name}.{extension}")).is_file()))
        })
    }

    fn run(&mut self, tool: Tool, arguments: &[OsString]) -> Result<Reply> {
        let name = tool.program();
        let output = self
            .command(tool)
            .args(arguments)
            .stdin(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .with_context(|| format!("start {name}"))?;
        Ok(Reply {
            success: output.status.success(),
            bytes: output.stdout,
        })
    }

    fn run_installed(&mut self, program: &Path, arguments: &[OsString]) -> Result<Reply> {
        let output = self
            .command_installed(program)
            .args(arguments)
            .stdin(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .with_context(|| format!("start {}", program.display()))?;
        Ok(Reply {
            success: output.status.success(),
            bytes: output.stdout,
        })
    }
}

pub fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

fn executable(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub fn checked(runner: &mut impl Runner, tool: Tool, arguments: &[OsString]) -> Result<Vec<u8>> {
    let reply = runner.run(tool, arguments)?;
    ensure!(reply.success, "{} failed", tool.program());
    Ok(reply.bytes)
}

pub fn optional(runner: &mut impl Runner, tool: Tool, arguments: &[&str]) {
    if runner.available(tool)
        && let Err(error) = checked(runner, tool, &args(arguments))
    {
        eprintln!("Warning: {error:#}");
    }
}

pub fn replace(path: &Path, bytes: &[u8], executable: bool) -> Result<()> {
    if fs::read(path).is_ok_and(|current| current == bytes) {
        return Ok(());
    }
    let parent = path.parent().context("generated file parent is missing")?;
    fs::create_dir_all(parent)?;
    let mut candidate = tempfile::NamedTempFile::new_in(parent)?;
    candidate.write_all(bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        candidate
            .as_file()
            .set_permissions(fs::Permissions::from_mode(if executable {
                0o755
            } else {
                0o644
            }))?;
    }
    #[cfg(not(unix))]
    let _ = executable;
    candidate.as_file().sync_all()?;
    candidate.persist(path)?;
    Ok(())
}

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

pub fn context7(runner: &mut impl Runner, local_gui: bool) -> Result<()> {
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

pub fn integrations(home: &Path, runner: &mut impl Runner, local_gui: bool) -> Result<()> {
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
    agent_integrations(runner, local_gui)
}

pub fn agent_integrations(runner: &mut impl Runner, local_gui: bool) -> Result<()> {
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

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
        for bytes in [br#"{}"#.as_slice(), br#"{"installed":{}}"#, b"not JSON"] {
            let mut fake = Fake {
                replies: VecDeque::from([Reply {
                    success: true,
                    bytes: bytes.to_vec(),
                }]),
                calls: Vec::new(),
            };
            context7(&mut fake, false).unwrap();
            assert!(fake.calls.is_empty());
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
