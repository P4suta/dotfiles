//! The one place any dotfiles crate creates a process.
//! Every crate starts a program through [`command`], so `clippy.toml` refuses `Command::new` everywhere else.

use std::ffi::OsStr;
use std::process::Command;

/// A process for `program`, which callers name through a registry or a path they verified.
#[expect(
    clippy::disallowed_methods,
    reason = "the workspace's one process constructor"
)]
#[must_use]
pub fn command(program: impl AsRef<OsStr>) -> Command {
    Command::new(program)
}

/// A process that inherits only the explicitly constructed environment.
#[must_use]
pub fn isolated_command<K, V>(
    program: impl AsRef<OsStr>,
    environment: impl IntoIterator<Item = (K, V)>,
) -> Command
where
    K: AsRef<OsStr>,
    V: AsRef<OsStr>,
{
    let mut command = command(program);
    command.env_clear().envs(environment);
    command
}

/// The same registered command and child environment on a terminal.
#[cfg(feature = "pty")]
#[must_use]
pub fn pty_command(command: &Command) -> portable_pty::CommandBuilder {
    let mut terminal = portable_pty::CommandBuilder::new(command.get_program());
    terminal.args(command.get_args());
    if let Some(directory) = command.get_current_dir() {
        terminal.cwd(directory);
    }
    for (name, value) in command.get_envs() {
        if let Some(value) = value {
            terminal.env(name, value);
        } else {
            terminal.env_remove(name);
        }
    }
    terminal
}
