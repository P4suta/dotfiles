//! The full profile gate on the repository's Linux verification host.

use crate::skill_proofs::{self, PROOF_IMAGE};
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use std::path::Path;

const GATE: &str = "set -o pipefail; tar -C /source --exclude=.git --exclude=node_modules --exclude=target -cf - . | tar -C /work -xf -; mise trust --quiet; mise install chezmoi shellcheck nu starship bat; export MISE_AUTO_INSTALL=false; exec mise x -- cargo xtask profiles";

/// Runs every rendered profile and its shell checks in an owned Linux container.
///
/// # Errors
///
/// Returns an error when the image, tools, or profile gate fails.
pub fn check(root: &Path) -> Result<()> {
    let root = crate::canonical(root)?;
    let image = skill_proofs::proof_image(&root)?;
    skill_proofs::ensure_image(&root, &root.join(PROOF_IMAGE), &image)?;
    let volume = skill_proofs::container_target_volume(&root);
    let mut arguments = skill_proofs::container_run_arguments(&root, &image, &volume);
    *arguments
        .last_mut()
        .context("the container command has a gate")? = GATE.into();
    arguments.splice(
        2..2,
        [
            "--volume".into(),
            format!("{volume}-mise:/root/.local/share/mise").into(),
        ],
    );
    let status = Tool::Docker.command().args(arguments).status()?;
    ensure!(status.success(), "the Linux profile gate failed: {status}");
    Ok(())
}
