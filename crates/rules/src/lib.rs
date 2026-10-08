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
mod domain {
    use alloc::vec::Vec;

    /// Every word over `alphabet` up to `longest` letters, shortest first.
    pub fn words(alphabet: &[u8], longest: usize) -> Vec<Vec<u8>> {
        let mut found = alloc::vec![Vec::new()];
        let mut start = 0;
        for _ in 0..longest {
            let end = found.len();
            for index in start..end {
                for &letter in alphabet {
                    let mut word = found.get(index).cloned().unwrap_or_default();
                    word.push(letter);
                    found.push(word);
                }
            }
            start = end;
        }
        found
    }

    /// Every array of `N` flags.
    pub fn flags<const N: usize>() -> impl Iterator<Item = [bool; N]> {
        (0..1u32 << N).map(|bits| core::array::from_fn(|index| bits & (1 << index) != 0))
    }
}

pub mod adr_rules;
pub mod aside_rules;
pub mod body_rules;
pub mod boundary_rules;
pub mod change_rules;
pub mod directive_rules;
pub mod disk_scan_rules;
pub mod env_expansion_rules;
pub mod eol_rules;
pub mod gate_rules;
pub mod hosts_rules;
pub mod ignore_rules;
pub mod incident_rules;
pub mod input_rules;
pub mod instruction_rules;
pub mod legacy_rules;
pub mod lint_rules;
pub mod next_action_rules;
pub mod obligation_rules;
pub mod ocomment_rules;
pub mod order_rules;
pub mod path_conversion_rules;
pub mod pr_ready_rules;
pub mod pr_rules;
pub mod profile_rules;
pub mod proof_budget_rules;
pub mod prose_rules;
pub mod ready_push_rules;
pub mod reaper_rules;
pub mod repository_rules;
pub mod review_rules;
pub mod shape_rules;
pub mod shell_syntax_rules;
pub mod shell_write_rules;
pub mod skill_rules;
pub mod source_name_rules;
pub mod stale_rules;
pub mod start_health_rules;
pub mod target_rules;
pub mod timeout_rules;
pub mod wip_rules;
