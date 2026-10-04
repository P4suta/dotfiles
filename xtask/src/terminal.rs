use crate::runtime::{Native, Runner, args, checked};
use anyhow::{Context, Result, ensure};
use std::ffi::OsString;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

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
    let mut native = Native::new(Path::new(&home))?;
    if child {
        if matches!(session, Session::Herdr) && native.available("herdr") {
            if cfg!(target_os = "macos") {
                if native.available("herdr-agent") {
                    let _ = native.run("herdr-agent", &args(&["ensure"]));
                }
                let server = native.run("herdr", &args(&["status", "server"]))?;
                if !server.success
                    || !String::from_utf8_lossy(&server.bytes).contains("status: running")
                {
                    native
                        .command("nohup")
                        .args(["herdr", "server"])
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()?;
                    for _ in 0..50 {
                        let server = native.run("herdr", &args(&["status", "server"]))?;
                        if server.success
                            && String::from_utf8_lossy(&server.bytes).contains("status: running")
                        {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
            }
            let mut command = native.command("herdr");
            command.args(arguments);
            return execute(command);
        }
        if native.available("nu") {
            let mut command = native.command("nu");
            command.args(arguments);
            return execute(command);
        }
        let mut command = native.command(if cfg!(target_os = "macos") {
            "/bin/zsh"
        } else {
            "/bin/bash"
        });
        command
            .arg("-l")
            .env("DOTFILES_NO_NU", "1")
            .env("DOTFILES_BASH_ONLY", "1");
        return execute(command);
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
        let binary = if native.available("ghostty") {
            PathBuf::from("ghostty")
        } else {
            PathBuf::from("/Applications/Ghostty.app/Contents/MacOS/ghostty")
        };
        let mut command = native.command(binary);
        let path = bridge
            .to_str()
            .context("terminal bridge path is not UTF-8")?
            .replace('\'', "'\\''");
        command
            .arg(format!("--title={title}"))
            .arg(format!("--command='{path}' --child"));
        command
    } else if native.available("kitty") {
        let mut command = native.command("kitty");
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
            native.available("gnome-terminal"),
            "neither Kitty nor GNOME Terminal is installed"
        );
        let mut command = native.command("gnome-terminal");
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

pub fn doctor(strict: bool) -> Result<()> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .context("native home is unavailable")?;
    let mut native = Native::new(Path::new(&home))?;
    if cfg!(target_os = "macos") {
        let output = checked(
            &mut native,
            "dotguard",
            &args(if strict {
                &["doctor", "--strict"]
            } else {
                &["doctor"]
            }),
        )?;
        std::io::stdout().write_all(&output)?;
        return Ok(());
    }
    let mut missing = 0;
    let required: &[&str] = if cfg!(windows) {
        &[
            "chezmoi", "just", "git", "gh", "mise", "herdr", "claude", "codex", "opencode",
        ]
    } else {
        &[
            "chezmoi", "mise", "nu", "starship", "herdr", "atuin", "zoxide", "java", "rustc", "go",
            "opam", "ocaml", "gleam", "erl",
        ]
    };
    for name in required {
        let available = native.available(name);
        println!("{name}: {}", if available { "ready" } else { "missing" });
        if !available {
            missing += 1;
        }
    }
    if cfg!(windows) {
        if native.available("herdr") {
            let status = String::from_utf8(checked(
                &mut native,
                "herdr",
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
        for key in [
            "commit.gpgsign",
            "tag.gpgsign",
            "gpg.format",
            "user.signingkey",
        ] {
            let reply = native.run("git", &args(&["config", "--get", key]))?;
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
    }
    for name in ["codex", "claude", "docker", "code", "op", "fcitx5"] {
        println!(
            "{name}: {}",
            if native.available(name) {
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

pub fn touchid(live: bool, elevated: bool) -> Result<()> {
    ensure!(
        cfg!(target_os = "macos") && live,
        "Touch ID setup requires macOS and --live"
    );
    ensure!(
        std::io::stdin().is_terminal(),
        "Touch ID setup requires an interactive terminal"
    );
    let target = Path::new("/etc/pam.d/sudo_local");
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
        Path::new("/usr/lib/pam/pam_tid.so.2").is_file(),
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
            file.set_permissions(std::fs::Permissions::from_mode(0o444))?;
        }
    } else {
        let status = Command::new("sudo")
            .arg(std::env::current_exe()?)
            .args(["touchid", "--live", "--elevated"])
            .status()?;
        ensure!(status.success(), "elevated Touch ID setup failed");
    }
    Ok(())
}
