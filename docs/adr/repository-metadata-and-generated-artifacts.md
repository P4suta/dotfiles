---
status: proposed
date: 2026-10-08
---
# Generate repository artifacts from checked metadata

## Context

README instructions, the GitHub description, and license declarations can disagree when maintained independently.
Issue 145 requires one small metadata file and a drift check.

## Decision

`repository.toml` names the repository, summary, installation, usage, and existing license state.
The typed model rejects unknown fields and invalid required values.
`dotfiles repository generate` writes the minimal README and generates license files only for the owner's existing dual-license state.
Apache-only and Affero General Public License repositories preserve reviewed license files by their root-level paths and SHA-256 digests.
An unlicensed state generates no license files or README license section.
`dotfiles repository check` compares generated artifacts and tracked package manifests with the metadata.
The dual-license state requires the owner's copyright identity, a bot identity, or recorded consent for each other contributor.
Preserving an existing license state requires no new contributor consent record.
The two SPDX operand orders of the existing dual license are equivalent for package manifest checks.

The GitHub description participates when an exact `--remote OWNER/REPO` is supplied.
Generation changes that external description only with that explicit option.
The repository governance audit runs the same checker against a temporary checkout and reports drift without changing repository settings.
The README skill routes to the generator and retains only the decisions needed to write useful metadata.

## Verification

The model and runtime tests exercise malformed metadata, artifact drift, license drift, and contributor consent.
The standalone governance tests require a finding for absent or stale metadata evidence.
`crates/rules/src/repository_rules.rs` owns license and drift decisions, with unit tests, Kani harnesses, and rejecting probes.
The metadata parser has a source-bound cargo-fuzz target in `fuzz/`.
