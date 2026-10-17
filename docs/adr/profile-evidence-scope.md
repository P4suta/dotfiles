---
status: proposed
date: 2026-10-11
---
# Bind profile evidence to repositories that own the profile runtime

## Status

This decision amends the global applicability of [the original hosts gate](0020-os-family-push-evidence.md).

## Context

The [isolated code and native host stages](isolated-code-and-native-host-gates.md) build the dotfiles runtime and provision its chezmoi profile.
Other repositories have their own native gates and cannot execute this profile worker from their source trees.
Applying that worker's admission policy to domyjob rejects its dynamic CI matrix before its own pre-push checks run.

## Decision

Profile evidence applies to a commit containing both `.chezmoiroot` and `crates/dotfiles/Cargo.toml`.
The [Git adapter](../../crates/dotfiles/src/hosts.rs) resolves the commit and reads its tree before interpreting workflow labels or notes.
Uncommitted additions or removals cannot change this scope.
An invalid commit remains an error in every repository.
Both pre-push and PR readiness use this same admission boundary.
Every required family still needs passing code and native stage evidence for profile commits.
Other repositories continue through their existing verification, review, and repository hooks.

The [pure scope decision](../../crates/rules/src/hosts_rules.rs) has exhaustive tests and a Kani harness.
Its rejecting probe is in the [proof inventory](../../policy/obligations.toml).
The [integration fixture](../../crates/dotfiles/tests/hosts.rs) checks immutable scope, missing commits, and a dynamic matrix in a repository without the profile runtime.
These adapter checks trust Git to resolve the named commit and return its tracked paths.

## Alternatives

Generalizing both profile stages into a host runner for arbitrary repositories requires a new provisioning contract and repository-specific gate selection.
It is separate from repairing this profile worker's applicability.
Reading the working tree would let an uncommitted deletion evade admission for an unchanged profile commit.
Skipping the pre-push hook would also omit the repository's own checks.

## Consequences

This gate no longer claims to run arbitrary repositories' native checks.
Their own hooks and CI retain that responsibility.
The dotfiles profile retains the same three-family stage requirements.
If the profile runtime or source-root paths move, this scope predicate and its fixtures must change together.
