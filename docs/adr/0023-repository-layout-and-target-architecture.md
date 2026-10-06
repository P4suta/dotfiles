---
status: accepted
---
# Give the repository a layout that shows what it manages

## Context

The repository root held chezmoi source state, about forty setup scripts, three Rust packages, operations data, and linter settings side by side.
Only the tail of `.chezmoiignore` kept tooling out of a home directory, and most managed files existed twice, as a per-profile leaf and a dispatcher.
A reader couldn't tell from the layout what the repository manages, what builds it, and what checks it.

## Decision

The source state lives under `home/`, which `.chezmoiroot` names, and nothing outside `home/` deploys to a host.
Setup scripts live in `home/.chezmoiscripts/`, one directory per OS family, and each hashes only its own inputs.
Every host renders one Git configuration from one template.
The target layout then moves toward these steps, and the committed profile manifests verify each one:

- One Cargo workspace, with `xtask/` at the root by Cargo convention and the other crates under `crates/`.
- `just` as the only command surface, with fast staged checks for the pre-commit hook.
- Shared data split by concern into typed TOML that a Rust model validates.
- One manifest of which profile receives which target, which generates the ignore and remove listings.
- One source file per target, with per-profile forks only where a target differs per profile.
- One machine configuration schema, derived from the host where possible.
- The installed runtime split from the task runner, under its own name.

[The architecture](../architecture.md) describes the render flow, the hook chain, and the gates.

## Consequences

The profile manifests under `xtask/tests/snapshots/` record what each profile renders, so each step proves that it leaves every host unchanged or explains each difference.
A new root file can't reach a home directory by accident.
Tools that read the source state go through `source_state`, which joins `home/` to the checkout root.
