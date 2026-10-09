//! The library behind the dotguard binaries.
//!
//! The crate builds `dotguard`, the git policy gate behind the global hooks and the `~/.local/bin/git` wrapper.
//! It also builds `reaper`, the two launchd maintenance agents, and `herdr-agent`, the ssh-agent for Herdr that loads SSH keys from 1Password.
//!
//! Only the binaries of this crate consume the library, so getters skip `must_use`.
#![expect(
    clippy::must_use_candidate,
    reason = "the binaries in this crate are the only readers of its getters"
)]

pub mod attribution;
pub mod bypass;
pub mod clock;
pub mod gitargv;
pub mod lang;
pub mod locate;
pub mod postcommit;
pub mod prepush;
pub mod real_git;
pub use platform::refusal;
pub use rules::{gate_rules, guard_rules, postcommit_rules, reaper_rules};
pub mod renovate;
pub mod stack;
pub mod staged;
