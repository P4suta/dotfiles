//! The installed runtime behind every Git hook, setup launcher, and agent hook, and the Windows machine setup.

pub mod body_rules;
pub mod claude_hook;
pub mod claude_timing;
pub mod desktop;
pub mod disk_scan_rules;
pub mod eol_rules;
pub mod health;
pub mod hooks;
pub mod hosts;
pub mod hosts_rules;
pub mod integrations;
pub mod line_endings;
pub mod path_conversion_rules;
pub mod profiles;
pub mod ready_push;
pub mod ready_push_rules;
pub mod secrets;
pub mod setup;
pub mod shell_write_rules;
pub mod start_health_rules;
pub mod terminal;
pub mod timeout_rules;
pub mod transaction;
#[cfg(windows)]
pub mod windows;
pub mod wsl;

pub use model::profile_rules;
pub use platform::paths::{
    SOURCE_STATE, canonical, has_extension, source_state, without_verbatim_prefix,
};
pub use platform::{
    Relayed, aside, build_inputs, freshness, refusal, report, runtime, stale_rules, tool,
};
