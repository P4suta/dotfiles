---
status: proposed
date: 2026-10-11
---
# Verify application with native tool state

## Context

The real-host candidate previously applied files with every script excluded, then installed tools into empty mise storage.
That sequence couldn't expose deletion of tools already installed for other projects or a trust change during application.
The Windows tool step pruned tracked project pins after application enabled paranoid mode, although the real-host gate had passed.

## Decision

The candidate copies native mise installations, configuration, tracking, trust, caches and Rust toolchain state into owned scratch storage before selecting its renderer.
Installation files have independent contents, including tools linked to another directory.
Internal links, cycles and links to absent optional platform packages stay within the copied installation store.
Tracking links continue to identify project configurations, while references to the replaced global configuration identify its scratch copy.
The remote bootstrap captures the native mise locations before changing HOME or configuration variables.

The candidate performs the same trust prerequisite as `just apply`, applies the candidate configuration, executes the production mise provisioning adapter in the native shell, converges the rendered files and verifies them.
The normal setup and candidate share the installer environment, including native build options and the configured mise job count.
It shares the [managed-file and state transaction](../../crates/dotfiles/src/transaction.rs) with the native profile apply.
The installation inventory must retain every pre-existing tool version after provisioning.
The running renderer also belongs to the copied installation store.

OS provisioning steps that change registry settings, services, installed desktop applications or scheduled tasks retain their separate native checks.
Executing those steps directly from a scratch HOME would still change host resources.
The tool-state candidate therefore executes the production mise portion of setup, with every installer destination relocated into scratch.

## Assurance boundary

Filesystem copy semantics, symbolic links, external mise configuration tracking and native process execution cross the decision core's verifier boundary.
No pure rule changes in this correction.
The [native adapter tests](../../crates/dotfiles/src/host_checks.rs) reproduce the trust transition with existing project tools and verify preservation of both the candidate installations and the fixture host.
A negative control executes real pruning, requires candidate failure and checks managed-file and state rollback while the fixture host retains its original files.
The [storage tests](../../crates/dotfiles/src/host_checks/storage.rs) check independent contents and captured-directory precedence.
`cargo test --locked -p dotfiles --lib host_checks` and the native real-host candidate gate supply evidence for these external boundaries.
The repository's Rust gate runs the adapter tests.
Changing mise's directory layout, trust records or tool installation semantics requires renewed native evidence.

## Consequences

The real-host stage needs space and time to copy the existing tool state.
Installations and tracking from the host can expose defects that an empty candidate environment hid.
Copies preserve host tools even when candidate provisioning fails or its negative control deletes a scratch installation.
