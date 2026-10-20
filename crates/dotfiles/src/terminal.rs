use crate::runtime::{Real, Runner, args, checked};
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use std::ffi::OsString;
use std::io::IsTerminal;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::time::Duration;

/// How long the Herdr server gets to report that it runs: fifty checks a tenth of a second apart.
const SERVER_CHECKS: u32 = 50;
const SERVER_CHECK_INTERVAL: Duration = Duration::from_millis(100);

/// The local sudo authentication file macOS reads beside its own, and the fingerprint module it names.
const SUDO_LOCAL: &str = "/etc/pam.d/sudo_local";
const TOUCH_ID_MODULE: &str = "/usr/lib/pam/pam_tid.so.2";
const HOOK_DIRECTORY: &str = ".config/git/hooks";
/// `r--r--r--`, as macOS ships its authentication files.
#[cfg(unix)]
const PAM_FILE_MODE: u32 = 0o444;

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum Session {
    Herdr,
    Nu,
}
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum Emulator {
    Ghostty,
    Kitty,
}

fn execute(mut command: Command) -> Result<ExitStatus> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(command.exec().into())
    }
    #[cfg(not(unix))]
    {
        Ok(command.status()?)
    }
}

/// Builds the session a terminal window runs, choosing Herdr with its server started, else Nushell, else a login shell.
fn child_command(native: &mut Real, session: Session, arguments: &[OsString]) -> Result<Command> {
    if matches!(session, Session::Herdr) && native.available(Tool::Herdr) {
        if cfg!(target_os = "macos") {
            start_herdr_server(native)?;
        }
        let mut command = native.command(Tool::Herdr);
        command.args(arguments);
        return Ok(command);
    }
    if native.available(Tool::Nu) {
        let mut command = native.command(Tool::Nu);
        command.args(arguments);
        return Ok(command);
    }
    let mut command = native.command(if cfg!(target_os = "macos") {
        Tool::Zsh
    } else {
        Tool::Bash
    });
    command
        .arg("-l")
        .env("DOTFILES_NO_NU", "1")
        .env("DOTFILES_BASH_ONLY", "1");
    Ok(command)
}

/// Starts the Herdr server with its SSH agent unless it already runs, waiting up to five seconds for it.
fn start_herdr_server(native: &mut Real) -> Result<()> {
    if native.available(Tool::HerdrAgent) {
        platform::cleanup::report(
            native.run(Tool::HerdrAgent, &args(&["ensure"])),
            "start the Herdr SSH agent",
        );
    }
    let running = |native: &mut Real| -> Result<bool> {
        let server = native.run(Tool::Herdr, &args(&["status", "server"]))?;
        Ok(server.success && String::from_utf8_lossy(&server.bytes).contains("status: running"))
    };
    if running(native)? {
        return Ok(());
    }
    native
        .command(Tool::Nohup)
        .args(["herdr", "server"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    for _ in 0..SERVER_CHECKS {
        if running(native)? {
            break;
        }
        std::thread::sleep(SERVER_CHECK_INTERVAL);
    }
    Ok(())
}

/// Opens a terminal window that runs the session, or runs the session itself when invoked as the window's child.
///
/// # Errors
///
/// Returns an error when the native home can't resolve, the bridge path holds non-UTF-8 text, or the command can't start.
/// It also returns an error when a Kitty window needs Kitty or `gnome-terminal` and the host has neither.
pub fn run(
    emulator: Emulator,
    session: Session,
    child: bool,
    arguments: &[OsString],
) -> Result<ExitStatus> {
    let child = child
        || arguments
            .first()
            .is_some_and(|argument| argument == "--child");
    let arguments = if arguments
        .first()
        .is_some_and(|argument| argument == "--child")
    {
        &arguments[1..]
    } else {
        arguments
    };
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .context("native home is unavailable")?;
    let mut native = Real::new(Path::new(&home))?;
    if child {
        return execute(child_command(&mut native, session, arguments)?);
    }
    let title = if matches!(session, Session::Herdr) {
        "Herdr"
    } else {
        "Nushell"
    };
    let emulator_name = if matches!(emulator, Emulator::Ghostty) {
        "ghostty"
    } else {
        "kitty"
    };
    let session_name = if matches!(session, Session::Herdr) {
        "herdr"
    } else {
        "nu"
    };
    let bridge = Path::new(&home).join(format!(".local/bin/{emulator_name}-{session_name}"));
    let mut command = if matches!(emulator, Emulator::Ghostty) {
        let mut command = if native.available(Tool::Ghostty) {
            native.command(Tool::Ghostty)
        } else {
            native.command_installed(Path::new(crate::desktop::GHOSTTY_BUNDLE_BINARY))
        };
        let path = bridge
            .to_str()
            .context("terminal bridge path is not UTF-8")?
            .replace('\'', "'\\''");
        command
            .arg(format!("--title={title}"))
            .arg(format!("--command='{path}' --child"));
        command
    } else if native.available(Tool::Kitty) {
        let mut command = native.command(Tool::Kitty);
        command
            .args([
                "--class",
                &format!("dotfiles-{session_name}"),
                "--title",
                title,
            ])
            .arg(bridge)
            .arg("--child")
            .args(arguments);
        if std::env::var_os("XDG_SESSION_TYPE").is_some_and(|value| value == "x11") {
            command.env("GLFW_IM_MODULE", "ibus");
        }
        command
    } else {
        ensure!(
            native.available(Tool::GnomeTerminal),
            "neither Kitty nor GNOME Terminal is installed"
        );
        let mut command = native.command(Tool::GnomeTerminal);
        command
            .arg(format!("--title={title}"))
            .arg("--")
            .arg(bridge)
            .arg("--child")
            .args(arguments);
        command
    };
    command.stdin(Stdio::inherit());
    execute(command)
}

/// Prints whether each required and optional terminal component works, and the host drift that [`crate::doctor`] finds.
///
/// # Errors
///
/// Returns an error when the native home can't resolve, a Windows or drift check can't run, or `strict` holds true and the host lacks a required component or drifted.
pub fn doctor(strict: bool) -> Result<()> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .context("native home is unavailable")?;
    let mut native = Real::new(Path::new(&home))?;
    let mut missing = 0;
    let required: &[Tool] = if cfg!(windows) {
        &[
            Tool::Chezmoi,
            Tool::Just,
            Tool::Git,
            Tool::Gh,
            Tool::Mise,
            Tool::Herdr,
            Tool::Claude,
            Tool::Codex,
            Tool::Opencode,
        ]
    } else {
        &[
            Tool::Chezmoi,
            Tool::Mise,
            Tool::Nu,
            Tool::Starship,
            Tool::Herdr,
            Tool::Atuin,
            Tool::Zoxide,
            Tool::Java,
            Tool::Rustc,
            Tool::Go,
            Tool::Opam,
            Tool::Ocaml,
            Tool::Gleam,
            Tool::Erl,
        ]
    };
    for &tool in required {
        let name = tool.program();
        let available = native.available(tool);
        println!("{name}: {}", if available { "ready" } else { "missing" });
        if !available {
            missing += 1;
        }
    }
    if !hooks_current(&mut native)? {
        println!("git:core.hooksPath: missing or changed managed hooks path");
        missing += 1;
    }
    missing += crate::doctor::shadowed(&native)?;
    missing += crate::doctor::source_drift(&mut native)?;
    if cfg!(windows) {
        missing += windows_missing(&mut native)?;
    }
    for tool in [
        Tool::Codex,
        Tool::Claude,
        Tool::Docker,
        Tool::Code,
        Tool::Op,
        Tool::Fcitx5,
    ] {
        println!(
            "{}: {}",
            tool.program(),
            if native.available(tool) {
                "ready"
            } else {
                "optional, absent"
            }
        );
    }
    ensure!(
        !strict || missing == 0,
        "{missing} required components are missing"
    );
    Ok(())
}

/// Prints and counts what Windows lacks among Herdr's agent integrations, the Git wrapper, the signing settings, and current Claude hooks.
fn windows_missing(native: &mut Real) -> Result<usize> {
    let mut missing = 0;
    if native.available(Tool::Herdr) {
        let status = String::from_utf8(checked(
            native,
            Tool::Herdr,
            &args(&["integration", "status"]),
        )?)?;
        for name in ["claude", "codex", "opencode"] {
            if !status
                .lines()
                .any(|line| line.starts_with(&format!("{name}: current ")))
            {
                println!("herdr:{name}: missing integration");
                missing += 1;
            }
        }
    }
    if !native.home.join(".local/bin/git.exe").is_file() {
        println!("git: dotguard wrapper missing");
        missing += 1;
    }
    for key in [
        "commit.gpgsign",
        "tag.gpgsign",
        "gpg.format",
        "user.signingkey",
    ] {
        let reply = native.run(Tool::Git, &args(&["config", "--get", key]))?;
        if !reply.success || reply.bytes.is_empty() {
            println!("git:{key}: missing");
            missing += 1;
        }
    }
    let settings = native.home.join(".claude/settings.json");
    if settings.is_file() {
        let value: serde_json::Value = serde_json::from_slice(&std::fs::read(settings)?)?;
        let hooks = format!("{} {}", value["hooks"], value["statusLine"]);
        if hooks.contains(".orca/") || hooks.contains(".orca\\\\") {
            println!("Claude integration: stale Orca hooks");
            missing += 1;
        }
    }
    Ok(missing)
}

fn hooks_current(native: &mut Real) -> Result<bool> {
    let reply = native.run(
        Tool::Git,
        &args(&["config", "--path", "--get", "core.hooksPath"]),
    )?;
    let configured = String::from_utf8(reply.bytes)?;
    let actual = crate::canonical(Path::new(configured.trim()));
    let expected = crate::canonical(&native.home.join(HOOK_DIRECTORY));
    let managed = actual
        .ok()
        .zip(expected.ok())
        .is_some_and(|(actual, expected)| actual == expected);
    Ok(crate::profile_rules::hooks_current(reply.success, managed))
}

/// Enables fingerprint authentication for `sudo` by creating `/etc/pam.d/sudo_local`, rerunning itself under `sudo` unless the caller passes `elevated`.
///
/// # Errors
///
/// Returns an error when the host runs something other than macOS, `live` stays false, or stdin lacks a terminal.
/// It also returns an error when an existing `sudo_local` lacks a `pam_tid.so` entry or the host lacks `/usr/lib/pam/pam_tid.so.2`.
/// It also returns an error when writing the file or the elevated rerun fails.
pub fn touchid(live: bool, elevated: bool) -> Result<()> {
    ensure!(
        cfg!(target_os = "macos") && live,
        "Touch ID setup requires macOS and --live"
    );
    ensure!(
        std::io::stdin().is_terminal(),
        "Touch ID setup requires an interactive terminal"
    );
    let target = Path::new(SUDO_LOCAL);
    if let Ok(content) = std::fs::read_to_string(target) {
        ensure!(
            content
                .lines()
                .any(|line| line.split_whitespace().next() == Some("auth")
                    && line.contains("pam_tid.so")),
            "sudo_local already exists without Touch ID; refusing to replace it"
        );
        return Ok(());
    }
    ensure!(
        Path::new(TOUCH_ID_MODULE).is_file(),
        "the Touch ID PAM module is missing"
    );
    if elevated {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?;
        file.write_all(b"# Managed by dotfiles.\nauth sufficient pam_tid.so\n")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(PAM_FILE_MODE))?;
        }
    } else {
        let status = Tool::Sudo
            .command()
            .arg(std::env::current_exe()?)
            .args(["touchid", "--live", "--elevated"])
            .status()?;
        ensure!(status.success(), "elevated Touch ID setup failed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_reads_the_effective_git_hooks_path() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let home = fixture.path();
        let managed = home.join(HOOK_DIRECTORY);
        let other = home.join("other-hooks");
        std::fs::create_dir_all(&managed)?;
        std::fs::create_dir_all(&other)?;
        let config = home.join("gitconfig");
        let mut native = Real::new(home)?;
        native.environment.extend([
            ("GIT_CONFIG_GLOBAL".into(), config.as_os_str().to_owned()),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
        ]);
        for (path, current) in [(&managed, true), (&other, false)] {
            std::fs::write(
                &config,
                format!(
                    "[core]\n\thooksPath = {}\n",
                    path.to_string_lossy().replace('\\', "/")
                ),
            )?;
            assert_eq!(hooks_current(&mut native)?, current);
        }
        std::fs::write(config, "")?;
        assert!(!hooks_current(&mut native)?);
        Ok(())
    }
}
