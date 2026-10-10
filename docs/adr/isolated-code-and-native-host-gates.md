---
status: accepted
date: 2026-10-10
---
# Isolate code gates and retain native provisioning checks

## Context

A gate run binds evidence to a source tree, but inherited tools, configuration, and credentials change its result.
Isolation alone also hides defects in the profile that provisions the host.

## Decision

The hosts gate runs a code stage and a native host stage for each required OS family.
The code stage runs `just check` through the [shared process constructor](../../crates/platform/src/process.rs), with an empty environment populated by the [environment builder](../../crates/platform/src/code_environment.rs).
The builder resolves the checkout's exact mise pins without global or system mise configuration.
It uses their installation directories, the OS base directories, and a directory containing only the mise bootstrap executable.
Rust uses the pinned toolchain's actual bin directory rather than a directory of shared rustup proxies.
Home, user configuration, caches, Git configuration, and canonical temporary storage belong to the run.
The installation store supplies binaries and the Rust toolchain, never credentials or user configuration.

The native stage applies the candidate profile's files to an owned scratch destination and verifies them there.
Native and project tool installation use that destination's HOME and Git global configuration, the host's PATH and shell, and its system Git, without an SSH agent or interactive input.
Installer state and caches stay in scratch storage.
Public repository fetches exercise the transport paths that exposed the original leaks.
The currently applied profile is verified read-only against its configured source, with chezmoi state and cache in scratch storage.
The gate never applies the candidate to the real home; the normal apply after merge remains its only rollout.
Provisioning and network failures remain host failures and require corrections to dotfiles source.
Each selected check runs after another fails, and the final report preserves every failure.
Git notes bind both stages to the checked commit and tree.

## Alternatives

Containers alone cannot exercise native provisioning or host-specific process behavior.
Per-command configuration overrides leave descendants exposed to the next inherited setting.
Adding host directories to the code PATH reintroduces application and tool precedence leaks.

## Consequences

The [fixture](../../crates/dotfiles/tests/hosts.rs) checks the production entry point against clean and contaminated host environments, including a descendant process.
The [pure pin decision](../../crates/rules/src/hosts_rules.rs) has an exhaustive input test and Kani harness with rejecting probes in the proof inventory.
Process inheritance, filesystem resolution, Git transport selection, and tool execution remain OS boundaries covered by fixtures and the three native host runs.
Fresh configuration and caches can make a code run cost more than one that reuses a user's home.
