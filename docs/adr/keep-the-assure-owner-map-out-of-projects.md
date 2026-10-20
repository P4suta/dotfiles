---
date: 2026-10-20
status: accepted
---
# Keep the assure owner map out of projects

## Status

Accepted.
Amends [verification in proportion to code shape](verification-proportional-to-code-shape.md).

## Context

The owner's local conventions must stay on the owner's machines.
Committing `assure.toml` to each project exports those conventions to its contributors.
A verdict must also identify the installed catalog and comparison commit so readers can see which inputs produced it.
Remote-tracking refs can differ between checkouts and do not identify the PR base that GitHub reported.

## Decision

Read owner maps from `~/.config/assure/repos/<owner>__<repo>.toml`.
Resolve the GitHub repository from the checkout's HTTPS, SSH, or scp-style origin URL and lowercase its owner and repository names.
Chezmoi installs this repository's map from `home/dot_config/assure/repos/p4suta__dotfiles.toml` on every profile.
An existing owner-side map supplies the same configuration for both compared revisions.
Its presence prevents reading the project's legacy map, including historical copies.
A malformed owner-side map fails without falling back to project configuration.

Until an owner-side map exists, a checkout that tracks `assure.toml` can still read that file in each surveyed revision.
Each result carries the exact command to create the owner directory, copy the map there, and run `git rm assure.toml`.
The gate prints that command without moving or editing either file itself.
An untracked project map supplies no ownership.

Every outcome records the installed `~/.config/assure/shapes.toml` version and SHA-256 digest, the explicit comparison commit or empty tree, and the source and digest of each owner map it read.
The first unversioned installed catalog is version 1 for migration compatibility.
Refusals identify the rule as coming from the owner's local catalog and include the same inputs.
PR admission passes GitHub's current base SHA directly.
A new branch push compares against the merge base with the upstream default commit advertised by Git, without reading remote-tracking refs.
Unavailable comparison commits refuse with the fetch command rather than silently changing the base.

## Alternatives

Keeping a project map would continue exporting local policy.
Copying a project map automatically would modify owner configuration during a read and obscure the migration boundary.
Using a branch name or a remote-tracking ref would leave the comparison dependent on hidden mutable state.

## Consequences

Projects keep their verification code and inline exemptions while the owner keeps the verification map and budgets.
An owner-side budget applies unchanged to both surveyed revisions; legacy budget changes remain checked during migration.
Map updates require a separate owner configuration change and rollout.
Catalog and map digests identify exact bytes, but do not distribute those private inputs or establish that another reader has installed them.
Origin URLs outside the supported GitHub forms cannot resolve an owner-side map.
The [assure integration tests](../../crates/dotfiles/tests/assure.rs), shape-rule harnesses, and rejecting probes in `policy/obligations.toml` cover the migration and input contracts.
