---
status: proposed
date: 2026-10-11
---
# Start mise installations without mise shims

## Context

A mise backend can invoke another managed tool while the installer holds its lock.
A shim for that tool reenters mise and waits for the same lock, so neither process can finish.
Tool-specific settings leave other backends exposed to that dependency cycle.

## Decision

The [shared process boundary](../../crates/platform/src/process.rs) prepares every installation after its command environment is complete.
It removes native and configured mise shim directories from PATH, including filesystem aliases, and returns a command with no environment or argument setters.
Direct tool directories retain their order, and other tool managers' directories remain available.
Setup runners, Windows setup, and code-environment provisioning use that boundary.
Shell, service, development, container, and CI entrypoints use `dotfiles mise-install`.
The real-host candidate runs that entrypoint inside its native shell, so login startup cannot reintroduce shims into the installer.
Before the runtime exists, its source launcher requests only Rust rather than installing every project tool through `mise x`.

The Erlang-specific auto-install setting is unnecessary after installation stops resolving backend programs through shims.
The [GitHub credential resolver](github-credentials-without-mise-shims.md) remains necessary for keyring access and native executable discovery, including a CLI whose installation directory is absent from PATH.
It can inspect configured mise storage without executing a shim or restoring the installer's original PATH.

## Assurance boundary

The [decision core](../../crates/rules/src/process_rules.rs) classifies every combination of configured and native shim facts.
Its Kani harness proves exclusion and direct-path retention, with rejecting probes in [the obligation inventory](../../policy/obligations.toml).
Filesystem canonicalization, operating-system PATH parsing, process inheritance, and external mise backends cross Kani's supported pure-core semantics.
The [native tests](../../crates/platform/tests/mise_install.rs) check the actual adapter, custom directories, path aliases, and the prepared command's structure.
Their backend fixture holds an operating-system file lock and checks that legacy shim resolution blocks while the prepared installation reaches a native executable.
The compile-fail contract refuses restoration of PATH after preparation.
`cargo test --locked -p platform --test mise_install`, platform doctests, and the native real-host candidate gate supply the required evidence for those external boundaries.
These tests establish the adapter and lock dependency rather than claiming a proof of mise's internal locking or every third-party backend.
Changes to mise's shim layout, environment variables, or operating-system executable discovery require renewed native evidence.

## Consequences

Installation needs a direct executable or a dependency supplied by mise instead of recursively invoking a shim.
Canonicalization and invalid PATH errors fail before the installer starts.
Development and container entrypoints build the maintained runtime before invoking its installer.
Keeping a raw shell `mise install` command would leave login-shell PATH changes outside the shared boundary.
