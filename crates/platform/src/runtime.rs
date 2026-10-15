//! The process runner every setup step and hook gate uses, over the managed tool directories.

use anyhow::{Context, Result, ensure};
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

use crate::target_rules::{Target, target};
use crate::tool::Tool;

const RENDER_TERM: &str = "xterm-256color";

#[derive(Debug)]
pub struct Reply {
    pub success: bool,
    pub bytes: Vec<u8>,
}

pub trait Runner {
    fn available(&self, tool: Tool) -> bool;
    /// Run a registered tool with `arguments` and capture its standard output.
    ///
    /// # Errors
    ///
    /// Returns an error when the tool can't start.
    fn run(&mut self, tool: Tool, arguments: &[OsString]) -> Result<Reply>;
    /// Generates initialization without a parent shell's mise session.
    /// Runners with no inherited environment use their ordinary captured reply.
    ///
    /// # Errors
    ///
    /// Returns an error when the generator can't start.
    fn initialization(&mut self, tool: Tool, arguments: &[OsString]) -> Result<Reply> {
        self.run(tool, arguments)
    }
    /// Generates shell-specific output in that shell's rendering context.
    ///
    /// # Errors
    ///
    /// Returns an error when the generator can't start.
    fn initialization_for(
        &mut self,
        tool: Tool,
        arguments: &[OsString],
        _shell: &str,
    ) -> Result<Reply> {
        self.initialization(tool, arguments)
    }
    /// Run a binary this repository installed at a known path, such as a bundle executable it just verified.
    ///
    /// # Errors
    ///
    /// Returns an error when the binary can't start.
    fn run_installed(&mut self, program: &Path, arguments: &[OsString]) -> Result<Reply>;
}

#[derive(Debug)]
pub struct Real {
    pub home: PathBuf,
    pub path: OsString,
    pub environment: Vec<(OsString, OsString)>,
    pub directory: Option<PathBuf>,
}

impl Real {
    /// A runner for `home` whose `PATH` puts the managed tool directories before the inherited `PATH`.
    ///
    /// # Errors
    ///
    /// Returns an error when joining a directory into a `PATH` value fails.
    pub fn new(home: &Path) -> Result<Self> {
        Ok(Self {
            home: home.to_path_buf(),
            path: crate::home::managed_path(home)?,
            environment: Vec::new(),
            directory: None,
        })
    }

    #[must_use]
    pub fn command(&self, tool: Tool) -> Command {
        self.prepare(tool.command(), tool == Tool::Codex)
    }

    /// Prepare an installation after applying the runner's final environment.
    ///
    /// # Errors
    ///
    /// Returns an error when preparing a shim-free installation environment fails.
    pub fn mise_install(&self, arguments: &[OsString]) -> Result<crate::process::MiseInstall> {
        let mut command = self.command(Tool::Mise);
        command.arg("install").args(arguments);
        Ok(crate::process::mise_install(command)?)
    }

    /// A Git hook gate, which keeps the hook's repository variables to inspect the commit in progress.
    pub fn hook_command(&self, tool: Tool) -> Command {
        let mut command = self.prepare(tool.hook_command(), tool == Tool::Codex);
        command.env(
            "CARGO_TARGET_DIR",
            hook_target(
                std::env::var_os("CARGO_TARGET_DIR")
                    .map(PathBuf::from)
                    .as_deref(),
                std::env::var_os("TEMP").map(PathBuf::from).as_deref(),
                &self.home,
            ),
        );
        command
    }

    /// A binary this repository installed at a known path, run with the same environment as registered tools.
    #[must_use]
    pub fn command_installed(&self, program: &Path) -> Command {
        self.prepare(crate::process::command(program), false)
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

impl Runner for Real {
    fn available(&self, tool: Tool) -> bool {
        let program = tool.executable();
        if program.is_absolute() {
            return executable(&program);
        }
        let name = tool.program();
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
        let mut command = self.command(tool);
        command
            .args(arguments)
            .stdin(Stdio::inherit())
            .stderr(Stdio::inherit());
        let output = if tool == Tool::Mise
            && arguments
                .first()
                .is_some_and(|argument| argument == "install")
        {
            crate::process::mise_install(command)?.output()
        } else if tool == Tool::Pwsh {
            let cache = crate::process::PowerShellCache::new()?;
            cache.apply(&mut command).output()
        } else {
            command.output()
        }
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

    fn initialization(&mut self, tool: Tool, arguments: &[OsString]) -> Result<Reply> {
        let mut command = self.command(tool);
        for name in MISE_SESSION {
            command.env_remove(name);
        }
        initialization_reply(command, tool, arguments)
    }

    fn initialization_for(
        &mut self,
        tool: Tool,
        arguments: &[OsString],
        shell: &str,
    ) -> Result<Reply> {
        let mut command = self.command(tool);
        for name in MISE_SESSION {
            command.env_remove(name);
        }
        command
            .env("STARSHIP_SHELL", shell)
            .env("TERM", RENDER_TERM);
        initialization_reply(command, tool, arguments)
    }
}

fn initialization_reply(mut command: Command, tool: Tool, arguments: &[OsString]) -> Result<Reply> {
    let output = command
        .args(arguments)
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("generate {} initialization", tool.program()))?;
    Ok(Reply {
        success: output.status.success(),
        bytes: output.stdout,
    })
}

/// A cached initialization belongs to its future shell rather than the generating session.
pub const MISE_SESSION: &[&str] = &[
    "__MISE_DIFF",
    "__MISE_SESSION",
    "__MISE_ORIG_PATH",
    "MISE_SHELL",
];

/// The Cargo target directory every hook gate shares, so a linked worktree never keeps its own build.
#[must_use]
pub fn hook_target(configured: Option<&Path>, temp: Option<&Path>, home: &Path) -> PathBuf {
    let volume = temp.and_then(|temp| {
        let mut parts = temp.components();
        match (parts.next(), parts.next()) {
            (Some(prefix @ Component::Prefix(_)), Some(root @ Component::RootDir)) => {
                Some(PathBuf::from_iter([prefix, root]))
            }
            _ => None,
        }
    });
    let absolute = configured.filter(|path| path.is_absolute());
    match (
        target(absolute.is_some(), volume.is_some()),
        absolute,
        volume,
    ) {
        (Target::Configured, Some(path), _) => path.to_path_buf(),
        (Target::Development, _, Some(volume)) => volume.join("cargo/target"),
        _ => home.join(".cache/dotfiles-target"),
    }
}

#[must_use]
pub fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

/// The execute bits for the owner, the group, and others.
#[cfg(unix)]
const ANY_EXECUTE: u32 = 0o111;
/// `rwxr-xr-x` for an executable and `rw-r--r--` for any other generated file.
#[cfg(unix)]
const EXECUTABLE_MODE: u32 = 0o755;
#[cfg(unix)]
const FILE_MODE: u32 = 0o644;

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
        metadata.permissions().mode() & ANY_EXECUTE != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Runs `tool` and returns its standard output when it succeeds.
///
/// # Errors
///
/// Returns an error when the tool can't start or fails.
pub fn checked(runner: &mut impl Runner, tool: Tool, arguments: &[OsString]) -> Result<Vec<u8>> {
    let reply = runner.run(tool, arguments)?;
    ensure!(reply.success, "{} failed", tool.program());
    Ok(reply.bytes)
}

/// Captures a successful initialization without the parent shell's mise session.
///
/// # Errors
///
/// Returns an error when the generator can't start or fails.
pub fn initialization(
    runner: &mut impl Runner,
    tool: Tool,
    arguments: &[OsString],
) -> Result<Vec<u8>> {
    let reply = runner.initialization(tool, arguments)?;
    ensure!(reply.success, "{} initialization failed", tool.program());
    Ok(reply.bytes)
}

/// Captures registered initialization in the specified shell's rendering context.
///
/// # Errors
///
/// Returns an error when the generator can't start or fails.
pub fn initialization_for(
    runner: &mut impl Runner,
    tool: Tool,
    arguments: &[OsString],
    shell: &str,
) -> Result<Vec<u8>> {
    let reply = runner.initialization_for(tool, arguments, shell)?;
    ensure!(reply.success, "{} initialization failed", tool.program());
    Ok(reply.bytes)
}

pub fn optional(runner: &mut impl Runner, tool: Tool, arguments: &[&str]) {
    if runner.available(tool)
        && let Err(error) = checked(runner, tool, &args(arguments))
    {
        eprintln!("Warning: {error:#}");
    }
}

/// Writes `bytes` to `path` through a synced temporary file, unless it already holds them.
///
/// # Errors
///
/// Returns an error when `path` has no parent, or creating, writing, or renaming the temporary file fails.
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
                EXECUTABLE_MODE
            } else {
                FILE_MODE
            }))?;
    }
    #[cfg(not(unix))]
    let _ = executable;
    candidate.as_file().sync_all()?;
    candidate.persist(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_initialization_sets_only_child_context_and_clears_mise_session() -> Result<()> {
        let home = tempfile::tempdir()?;
        let mut runner = Real::new(home.path())?;
        runner
            .environment
            .push(("STARSHIP_SHELL".into(), "parent-shell".into()));
        runner.environment.push(("TERM".into(), "dumb".into()));
        for name in MISE_SESSION {
            runner
                .environment
                .push(((*name).into(), "parent-session".into()));
        }
        let environment = runner.environment.clone();
        let script = "[$env.STARSHIP_SHELL $env.TERM (('__MISE_DIFF' in $env) or ('__MISE_SESSION' in $env) or ('__MISE_ORIG_PATH' in $env) or ('MISE_SHELL' in $env))] | to json --raw";
        let output = initialization_for(
            &mut runner,
            Tool::Nu,
            &args(&["--no-config-file", "-c", script]),
            "zsh",
        )?;
        let actual: serde_json::Value = serde_json::from_slice(&output)?;
        assert_eq!(actual, serde_json::json!(["zsh", RENDER_TERM, false]));
        assert_eq!(runner.environment, environment);
        Ok(())
    }

    #[test]
    fn a_dumb_parent_cannot_supply_the_cached_starship_continuation() -> Result<()> {
        let home = tempfile::tempdir()?;
        let config = home.path().join("starship.toml");
        fs::write(
            &config,
            "continuation_prompt = '[owned-continuation](bold red)'\n",
        )?;
        let mut runner = Real::new(home.path())?;
        runner.environment.extend([
            ("TERM".into(), "dumb".into()),
            ("STARSHIP_CONFIG".into(), config.into_os_string()),
            (
                "STARSHIP_CACHE".into(),
                home.path().join("cache").into_os_string(),
            ),
        ]);
        let parent = runner.environment.clone();
        for shell in ["nu", "zsh"] {
            let mut direct = runner.command(Tool::Starship);
            let disabled = direct
                .args(["prompt", "--continuation"])
                .env("STARSHIP_SHELL", shell)
                .output()?;
            ensure!(disabled.status.success(), "dumb Starship control failed");
            assert!(
                String::from_utf8_lossy(&disabled.stdout)
                    .contains("Starship disabled due to TERM=dumb")
            );
            let expected = runner
                .command(Tool::Starship)
                .args(["prompt", "--continuation"])
                .env("STARSHIP_SHELL", shell)
                .env("TERM", RENDER_TERM)
                .output()?;
            ensure!(
                expected.status.success(),
                "interactive Starship control failed"
            );
            let actual = initialization_for(
                &mut runner,
                Tool::Starship,
                &args(&["prompt", "--continuation"]),
                shell,
            )?;
            assert_eq!(actual, expected.stdout);
            assert!(String::from_utf8_lossy(&actual).contains("owned-continuation"));
            assert!(!String::from_utf8_lossy(&actual).contains("TERM=dumb"));
        }
        assert_eq!(runner.environment, parent);
        Ok(())
    }
}
