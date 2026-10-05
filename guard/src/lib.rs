//! dotguard — the local policy gate, as a library.
//!
//! The crate builds three binaries: `dotguard` (the git policy gate behind the global hooks and the `~/.local/bin/git` wrapper), `reaper` (the two launchd maintenance agents, installed under their historical names so the plists never notice the migration), and `herdr-agent` (Herdr's own ssh-agent, loaded from 1Password once).
//! Everything they share — git location, the hook PATH, the bypass ledger — lives here once.
//!
//! This library has no consumers outside the crate (`publish = false`), so `must_use` annotations on every getter would be ceremony without a reader; the binaries are the readers, and they read attentively.
#![expect(
    clippy::must_use_candidate,
    reason = "the binaries in this crate are the only readers of its getters"
)]

pub mod attribution;
pub mod bypass;
pub mod doctor;
pub mod gate_rules;
pub mod gitargv;
pub mod lang;
pub mod lint;
pub mod locate;
pub mod postcommit;
pub mod prepush;
pub mod realgit;
pub mod reaper_rules;
pub mod refusal;
pub mod renovate;
pub mod stack;
pub mod staged;
