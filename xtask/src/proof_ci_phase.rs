//! Fixed partitions of the complete required Linux proof campaign.

use crate::checks::Checks;
use crate::obligations;
use anyhow::{Result, ensure};
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum Phase {
    Kani,
    Runtime,
    MetricsModels,
}

impl Phase {
    pub const ALL: [Self; 3] = [Self::Kani, Self::Runtime, Self::MetricsModels];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Kani => "kani",
            Self::Runtime => "runtime",
            Self::MetricsModels => "metrics-models",
        }
    }
}

/// Run one mandatory partition without altering the contracts or verification inventory it owns.
///
/// # Errors
///
/// Returns every failure in the selected partition, or refuses an unsupported host.
pub fn verify(root: &Path, phase: Phase) -> Result<()> {
    ensure!(
        cfg!(target_os = "linux") && !dotfiles::bounded::requires_container(),
        "proof partitions require Linux CI or a bounded container; use `just proofs` or `dotfiles bounded run`"
    );
    crate::elapsed_refinement::with_source(root, || {
        crate::quality::with_locked_workspaces(root, || verify_prepared(root, phase))
    })
}

fn verify_prepared(root: &Path, phase: Phase) -> Result<()> {
    let mut checks = Checks::default();
    match phase {
        Phase::Kani => {
            checks.run("Kani", || {
                crate::skill_proofs::verify_kani(root, &obligations::manifest(root)?)
            });
        }
        Phase::Runtime => {
            return crate::skill_proofs::with_fuzz_compilation(
                || crate::skill_proofs::compile_fuzz_targets(root),
                || {
                    checks.run("Miri", || crate::skill_proofs::verify_miri(root));
                    checks.run("fuzz", || crate::skill_proofs::verify_fuzz(root));
                    checks.finish()
                },
            );
        }
        Phase::MetricsModels => {
            checks.run("fixed Linux benchmarks", || {
                crate::phase_evidence::run(
                    root,
                    "fixed Linux benchmarks",
                    crate::benchmarks::FUNCTIONS.len(),
                    || crate::benchmarks::verify_in_container(root),
                )
            });
            let manifest = obligations::manifest(root);
            let manifest = || {
                manifest
                    .as_ref()
                    .map_err(|error| anyhow::anyhow!("{error:#}"))
            };
            checks.run("Quint", || {
                let manifest = manifest()?;
                let workload = manifest
                    .obligations
                    .iter()
                    .filter(|obligation| matches!(obligation, obligations::Obligation::Quint(_)))
                    .count();
                crate::phase_evidence::run(root, "Quint models", workload, || {
                    crate::quint::verify(root, manifest)
                })
            });
            checks.run("Alloy", || {
                let manifest = manifest()?;
                let workload = manifest
                    .obligations
                    .iter()
                    .filter(|obligation| matches!(obligation, obligations::Obligation::Alloy(_)))
                    .count();
                crate::phase_evidence::run(root, "Alloy models", workload, || {
                    crate::alloy::verify(root, manifest)
                })
            });
            checks.run("Lean", || {
                let manifest = manifest()?;
                let workload = manifest
                    .obligations
                    .iter()
                    .filter(|obligation| matches!(obligation, obligations::Obligation::Lean(_)))
                    .count();
                crate::phase_evidence::run(root, "Lean packages", workload, || {
                    crate::lean::verify(root, manifest)
                })
            });
        }
    }
    checks.finish()
}
