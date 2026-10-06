/// The rule that names aside copies of running binaries, shared with the xtask installers.
#[path = "../../../../xtask/src/aside_rules.rs"]
mod aside_rules;
mod download;
pub mod herdr;
#[cfg(windows)]
pub mod keyboard;
pub mod ocomment;
mod ocomment_rules;
pub mod python_stub;
pub mod scoop;
pub mod shell;
pub mod sshd;
pub mod storage_scout;
pub mod tools;
pub mod winget;
