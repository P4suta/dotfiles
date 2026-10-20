//! Git's signing agent stays independent of the session's push agent.

use crate::signing_rules::{Route, Socket, SocketPath, route, socket_path};
use anyhow::{Context, Result};
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const CONFIG: &str = ".config/dotfiles/signing.toml";
const SSH_KEYGEN: &str = "/usr/bin/ssh-keygen";
const AGENT_SOCKET: &str = "SSH_AUTH_SOCK";
const HOME_PREFIX: &str = "~/";
const RECOVERY: &str = "start and unlock 1Password";

fn resolve_socket(home: &Path, configured: &str) -> Result<PathBuf> {
    let relative = configured.strip_prefix(HOME_PREFIX);
    match socket_path(
        configured.is_empty() || relative == Some(""),
        Path::new(configured).is_absolute(),
        relative.is_some(),
    ) {
        SocketPath::Absolute => Ok(configured.into()),
        SocketPath::HomeRelative => Ok(home.join(relative.context("home prefix is missing")?)),
        SocketPath::Invalid => {
            anyhow::bail!("signing socket must be an absolute path or start with ~/")
        }
    }
}

fn command(home: &Path, arguments: &[OsString]) -> Result<Command> {
    let config: model::data::SshSigning = toml::from_str(
        &fs::read_to_string(home.join(CONFIG)).context("read SSH signing configuration")?,
    )?;
    let socket = resolve_socket(
        home,
        config
            .socket
            .as_deref()
            .context("signing socket is missing from configuration")?,
    )?;
    let signing = arguments.windows(2).any(|pair| pair == ["-Y", "sign"]);
    let state = if signing {
        match fs::metadata(&socket) {
            Ok(metadata) if metadata.file_type().is_socket() => Socket::Present,
            Ok(_) => Socket::NotSocket,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Socket::Missing,
            Err(error) => return Err(error).context("inspect 1Password agent socket"),
        }
    } else {
        Socket::Missing
    };
    let cause = match route(signing, state) {
        Route::PasswordManager => None,
        Route::RefuseMissing => Some("1Password agent socket is missing"),
        Route::RefuseNotSocket => Some("1Password agent path is not a socket"),
    };
    if let Some(cause) = cause {
        return Err(
            crate::refusal::Refusal::new("git.signing-agent", cause, RECOVERY)
                .evidence(format!("socket: {}", socket.display()))
                .into(),
        );
    }
    let mut command = platform::process::command(SSH_KEYGEN);
    command.args(arguments).env(AGENT_SOCKET, socket);
    Ok(command)
}

/// Replaces this process with the platform's `ssh-keygen`.
///
/// # Errors
///
/// Refuses signing without a password manager socket, invalid configuration, or a missing home.
/// Returns an error if `ssh-keygen` can't start.
pub fn exec(arguments: &[OsString]) -> Result<()> {
    let home = platform::home::home().context("native home is unavailable")?;
    Err(command(&home, arguments)?.exec()).context("start platform SSH signing")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_resolution_uses_each_os_default_and_the_host_override() -> Result<()> {
        let data: model::data::Data = toml::from_str(&fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../home/.chezmoidata/signing.toml"),
        )?)?;
        let signing = data.ssh_signing.unwrap();
        let home = Path::new("/home/fixture");
        assert_eq!(
            resolve_socket(home, signing.mac_socket.as_deref().unwrap())?,
            home.join("Library/Group Containers/2BUA8C4S2C.com.1password/t/agent.sock")
        );
        assert_eq!(
            resolve_socket(home, signing.linux_socket.as_deref().unwrap())?,
            home.join(".1password/agent.sock")
        );
        assert_eq!(
            resolve_socket(home, "/custom/agent.sock")?,
            Path::new("/custom/agent.sock")
        );
        for invalid in ["", "~/", "~", "agent.sock"] {
            assert!(resolve_socket(home, invalid).is_err(), "{invalid}");
        }
        Ok(())
    }
}
