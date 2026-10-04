# Shared Skills and Typed Repository Governance

Status: Accepted.

## Context

The same engineering and release contracts must be available across agent clients and development machines.
Repository settings drift when their policy is repeatedly reconstructed by hand.

## Decision

Keep reusable procedures in the dotfiles-managed `.agents/skills` tree with thin client discovery entries.
Keep personal tool manuals in their owning projects and use shared skills to route to those manuals.
Use Rust for executable automation, with locked dependencies and the same checks before push and in required CI.
Validate portable metadata, resource links, and client aliases mechanically.

Model governance writes as explicit operation variants and recompute a plan from its captured state before applying it.
Bind every write to the live numeric repository identity and verify that existing checks, reviewers, timers, ref restrictions, and stronger rules survive.
Preserve Immutable Releases and separate code integration from signing and publication authorization.

## Consequences

A client integration has one procedure to load, and a missing or drifting discovery entry fails a check.
A settings failure requires a fresh audit and plan rather than a rollback that weakens protection.
Cross-platform checks run on the owner's actual machines through domyjob before push when the changed contract requires them.
