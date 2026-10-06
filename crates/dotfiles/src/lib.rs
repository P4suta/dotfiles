//! The installed runtime behind every Git hook, setup launcher, and agent hook, and the Windows machine setup.

pub mod claude_hook;
pub mod claude_timing;
pub mod desktop;
pub mod health;
pub mod hooks;
pub mod hosts;
pub mod integrations;
pub mod line_endings;
pub mod profiles;
pub mod ready_push;
pub mod secrets;
pub mod setup;
pub mod terminal;
pub mod transaction;
pub use rules::{
    body_rules, disk_scan_rules, eol_rules, hosts_rules, path_conversion_rules, ready_push_rules,
    shell_write_rules, start_health_rules, timeout_rules,
};
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
