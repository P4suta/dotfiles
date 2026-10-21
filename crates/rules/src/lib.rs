//! The pure decision core of every dotfiles tool.
//!
//! The crate has no dependency and links no standard library, so no rule can reach a file, a process, the network, a clock, the environment, or randomness.
//! The tools gather those facts and pass them in as values, and each rule returns its decision as a value.
//! Each module compiles alone under Kani, which proves its harnesses and rejects its counterexamples.
#![no_std]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::unreachable,
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::exit,
    clippy::mem_forget
)]

extern crate alloc;

/// The finite input domains the tests walk, so a test checks a specification over every input Kani checks.
#[cfg(test)]
mod domain;

pub mod adr_rules;
pub mod agent_memory_rules;
pub mod artifact_rules;
pub mod aside_rules;
pub mod benchmark_rules;
pub mod body_rules;
pub mod boundary_rules;
pub mod bounded_rules;
pub mod cfg_rules;
pub mod change_rules;
pub mod contrast_rules;
pub mod cycle_rules;
pub mod directive_rules;
pub mod disk_scan_rules;
pub mod env_expansion_rules;
pub mod eol_rules;
pub mod freshness_rules;
pub mod gate_rules;
pub mod git_ref_rules;
pub mod guard_rules;
pub mod hook_failure_rules;
pub mod hook_rules;
pub mod host_setup_rules;
pub mod hosts_rules;
pub mod ignore_rules;
pub mod incident_rules;
pub mod input_rules;
pub mod instruction_rules;
pub mod issue_guard_rules;
pub mod legacy_rules;
pub mod lint_rules;
pub mod local_state_rules;
pub mod next_action_rules;
pub mod notification_rules;
pub mod notify_chain_rules;
pub mod obligation_rules;
pub mod ocomment_rules;
pub mod order_rules;
pub mod origin_rules;
pub mod path_conversion_rules;
pub mod phase_receipt_rules;
pub mod postcommit_rules;
pub mod pr_ready_rules;
pub mod pr_rules;
pub mod process_rules;
pub mod profile_rules;
pub mod proof_budget_rules;
pub mod proof_image_prepare_rules;
pub mod prose_rules;
pub mod provenance_rules;
pub mod ready_push_rules;
pub mod reaper_rules;
pub mod reply_language_rules;
pub mod repository_rules;
pub mod resource_rules;
pub mod review_completion_rules;
pub mod review_routing_rules;
pub mod review_rules;
pub mod schema_rules;
pub mod shape_rules;
pub mod shell_cache_rules;
pub mod shell_syntax_rules;
pub mod shell_target_rules;
pub mod shell_write_rules;
pub mod signing_rules;
pub mod skill_gate_rules;
pub mod skill_plan_rules;
pub mod skill_rules;
pub mod source_name_rules;
pub mod source_sync_rules;
pub mod stale_rules;
pub mod start_health_rules;
pub mod tailscale_rules;
pub mod target_rules;
pub mod timeout_rules;
pub mod tool_lock_rules;
pub mod tool_rules;
pub mod wip_rules;
