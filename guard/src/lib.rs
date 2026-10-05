//! The library behind the dotguard binaries.
//!
//! The crate builds `dotguard`, the git policy gate behind the global hooks and the `~/.local/bin/git` wrapper.
//! It also builds `reaper`, the two launchd maintenance jobs, and `herdr-agent`, the Herdr key service that loads SSH keys from 1Password.
//! Shared code covers git location, the hook `PATH`, and the bypass ledger.
//!
//! Only the binaries of this crate consume the library, so getters skip `must_use`.
#![allow(clippy::must_use_candidate)]

pub mod attribution;
pub mod bypass;
pub mod doctor;
pub mod gitargv;
pub mod lang;
pub mod lint;
pub mod locate;
pub mod postcommit;
pub mod prepush;
pub mod realgit;
pub mod reaper_rules;
pub mod renovate;
pub mod staged;
