---
status: proposed
date: 2026-10-11
---
# Preserve tool versions during application

## Context

An apply changes the global mise settings while using the same installation and tracking data as every project on the host.
Enabling paranoid mode invalidates directory-level trust recorded before the apply, so mise 2026.10.0 ignores tracked projects without content-bound trust when computing prunable versions.
Its pruning algorithm also has no running-process protection.
Managed-file rollback cannot restore deleted tool installations.

## Decision

Application installs the selected tools and refreshes shims without pruning or uninstalling tool versions.
Retiring tmux archives its old configuration and keeps its installations.
Space reclamation is a separate, explicitly invoked maintenance operation after checking project pins and running tools.
A trust refresh during application would protect only known configurations and would leave concurrent processes and other projects outside the guarantee.

The [native regression](../../crates/dotfiles/src/windows/setup/tools.rs) registers a real project configuration under non-paranoid settings, enables paranoid mode, confirms that mise would prune its pin, and checks that the production installation preserves it.
The [structural test](../../xtask/tests/structure.rs) rejects version-removal operations in the shared and Windows tool setup adapters.

## Assurance boundary

Config tracking, content-bound trust, filesystem removal and external mise behavior require native integration evidence rather than a theorem about the decision core.
No pure rule changes in this correction.
`cargo test --locked -p dotfiles --lib applying_paranoid_settings_preserves_a_tracked_project_pin` checks the actual Windows adapter, and `cargo test --locked -p xtask --test structure apply_setup_cannot_delete_mise_versions` checks the production syntax and rejecting fixtures on every OS.
The repository's Rust gate runs both suites.
This evidence establishes the absence of version-removal commands in application, with the existing installation boundary still responsible for installing tools.
Changes to the setup adapters or mise installation semantics require renewed evidence.

## Consequences

Old versions consume disk space until separate maintenance removes them.
Project pins and running executables survive application independently of mise's pruning heuristics and operating-system file locking.
