//! The installed runtime behind every Git hook, setup launcher, and agent hook, and the Windows machine setup.

pub mod artifacts;
pub mod assure;
pub mod claude_hook;
pub mod claude_timing;
pub mod codex_hooks;
pub mod desktop;
pub mod health;
pub mod hooks;
pub mod host_checks;
pub mod hosts;
pub mod integrations;
pub mod issue_guard;
pub mod legacy;
pub mod line_endings;
pub mod notifications;
pub mod profiles;
pub mod ready_push;
pub mod repository;
pub mod secrets;
pub mod setup;
pub mod terminal;
pub mod transaction;
pub use rules::{
    body_rules, disk_scan_rules, env_expansion_rules, eol_rules, hook_rules, host_setup_rules,
    hosts_rules, legacy_rules, path_conversion_rules, ready_push_rules, repository_rules,
    shape_rules, shell_syntax_rules, shell_write_rules, skill_rules, start_health_rules,
    timeout_rules,
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
