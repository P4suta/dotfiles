---
status: proposed
date: 2026-10-08
---
# Generate repository artifacts from checked metadata

## Context

README instructions, the GitHub description, and license declarations can disagree when maintained independently.
Issue 145 requires one small metadata file and a drift check.

## Decision

`repository.toml` names the repository, summary, installation, usage, copyright holder, and dual license.
The typed model rejects unknown fields and invalid required values.
`dotfiles repository generate` writes the minimal README and both license files from embedded templates.
`dotfiles repository check` compares those artifacts and every tracked package manifest with the metadata.
Contributor identities require the owner's copyright identity, a bot identity, or a recorded consent.

The GitHub description participates when an exact `--remote OWNER/REPO` is supplied.
Generation changes that external description only with that explicit option.
The repository governance audit runs the same checker against a temporary checkout and reports drift without changing repository settings.
The README skill routes to the generator and retains only the decisions needed to write useful metadata.

## Verification

The model and runtime tests exercise malformed metadata, artifact drift, license drift, and contributor consent.
The standalone governance tests require a finding for absent or stale metadata evidence.
`crates/rules/src/repository_rules.rs` owns license and drift decisions, with unit tests, Kani harnesses, and rejecting probes.
The metadata parser has a source-bound cargo-fuzz target in `fuzz/`.
