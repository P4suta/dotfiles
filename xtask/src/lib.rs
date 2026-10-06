//! The repository's development tasks: the gates, the generated files, the proofs, and the rehearsal.
//! No hook or launcher runs this package, because the installed tools build from the crates under `crates/`.

pub mod adr;
pub mod adr_rules;
pub mod agent_memory;
pub mod change;
pub mod change_rules;
pub mod ignore_audit;
pub mod ignore_rules;
pub mod input_audit;
pub mod input_rules;
pub mod instruction_audit;
pub mod instruction_rules;
pub mod launchers;
pub mod lint;
pub mod lint_rules;
pub mod next_action;
pub mod next_action_rules;
pub mod profile_snapshot;
pub mod profiles;
pub mod proof_budget_rules;
pub mod quality;
pub mod rehearsal;
pub mod skill_proofs;

pub use model::profile_rules;
pub use platform::paths::{SOURCE_STATE, canonical, has_extension, source_state};
pub use platform::{build_inputs, refusal, report, tool};
pub use skill_ops::split_frontmatter;

use std::path::{Path, PathBuf};

#[must_use]
pub fn check_build_dir(root: &Path, configured: Option<&Path>) -> PathBuf {
    let target = match configured {
        Some(target) => root.join(target),
        None => root.join("target"),
    };
    target.join("checks")
}
