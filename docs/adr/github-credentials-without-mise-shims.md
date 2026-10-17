---
status: proposed
date: 2026-10-11
---
# Read GitHub credentials without mise shims

## Context

A mise installation can hold the lock that a credential command's mise shim needs.
The outer installer waits for its credential command while the shim waits for the installer.
The [mise CLI integration](https://github.com/jdx/mise/blob/v2026.10.0/docs/dev-tools/github-tokens.md) reads `hosts.yml` and cannot retrieve a login stored only in the operating system's keyring.

## Decision

Every profile delegates the requested GitHub host to its home's installed runtime through an absolute `dotfiles github-token` command.
The [resolver](../../crates/platform/src/github.rs) reads PATH and resolves native CLI executables canonically.
For a mise shim directory, it selects an installed `gh` from that directory's adjacent installation store without running mise.
It prefers the store's `latest` alias and otherwise uses an existing version.
Directory aliases and Windows alias files select the same installed CLI, including when other versions remain.
The credential belongs to the CLI login rather than a tool version.
Canonical targets that are mise executables or remain in shim directories cannot supply the credential.

The native CLI receives `MISE_CREDENTIAL_HOST` as a hostname argument and reads its existing configuration and keyring.
The token travels only on stdout to mise, with no file or persistent environment variable.
An unavailable CLI or login returns failure, preserving mise's other credential sources.

## Alternatives

Calling `gh` through PATH permits recursive installation through a mise shim.
Reading `hosts.yml` alone loses keyring logins.
Resolving the CLI through another mise command retains a dependency on the installer that needs the credential.
A fixed version's executable path can disappear after pruning and cannot resolve the native host's CLI from a candidate's empty installation store.

## Consequences

The [credential tests](../../xtask/tests/mise_credentials.rs) render every profile, exercise host-specific keyring login and missing-login behavior through real mise token resolution, and check executable discovery.
Their installer fixture holds an operating system file lock that a reentrant mise shim also needs.
The legacy command remains blocked until the test releases that lock, while the production resolver completes before release.
The fixture imports the production resolver and process constructor.

Executable identity, installation layout, process execution, shell expansion, and keyring access remain external operating system boundaries.
The resolver trusts the installed native CLI and mise's standard installation layout.
Native credential and provisioning runs verify those assumptions; the lock fixture establishes the dependency cycle rather than modeling mise's internal locking implementation.
No pure decision core changes, so the existing proof inventory remains applicable.
Changes to mise's installation layout or supported keyring semantics require renewed native tests.
