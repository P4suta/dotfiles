# Shared skills and typed repository governance

Status: accepted.

## Context

Every client and development machine needs the same engineering and release contracts.
Repository settings drift when someone rebuilds their policy by hand.

## Decision

Keep reusable procedures in the dotfiles-managed `.agents/skills` tree with thin client discovery entries.
Keep personal tool manuals in their owning projects, and route to them from shared skills.
Write automation in Rust with locked dependencies, and run the same checks before push and in required CI.
Check portable metadata, resource links, and client aliases in code.

Model governance writes as explicit operation variants, and recompute a plan from its captured state before applying it.
Bind every write to the live numeric repository identity, and verify that existing checks, reviewers, timers, ref restrictions, and stronger rules survive.
Keep Immutable Releases, and keep signing and publication authorization separate from code integration.

## Consequences

A client loads one procedure, and a missing or drifting discovery entry fails a check.
A settings failure calls for a fresh audit and plan, not a rollback that weakens protection.
When a changed contract needs cross-platform checks, they run on the owner's machines through domyjob before push.
