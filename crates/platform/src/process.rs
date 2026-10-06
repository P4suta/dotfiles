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
