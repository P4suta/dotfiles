---
date: 2026-10-21
status: proposed
---
# Run unbounded verifiers in limited containers

## Status

Proposed under [issue 309](https://github.com/P4suta/dotfiles/issues/309).

## Context

Native model checkers, mutation tools, fuzzers, and solvers can exhaust an owner's machine while other work runs.
macOS provides no cgroup boundary for native commands.
A shell alias or a directory prepended to `PATH` loses to the executable directories that mise selects for project tasks.

## Decision

The [policy](../../home/dot_config/dotfiles/bounded-tools.toml) names protected executable entrypoints and finite default resource limits.
The [bounded runner](../../crates/dotfiles/src/bounded.rs) owns the Docker lifecycle, applies memory, combined memory and swap, CPU, and PID limits, and runs an in-container deadline.
It requires an immutable Linux image and an explicit command.
OrbStack supplies Docker on macOS.
Linux uses its Docker engine.
Project container plans use the same lifecycle through the proof process adapter.
CI retains its native proof route.
The proof campaign requires the Kani subject regressions, keeping unbounded verifier execution out of the ordinary unit-test stage.
The native material preparation leaves the Kani health regression to this bounded campaign on owner machines and preserves the GitHub Actions context in isolated CI commands.

The [installer](../../crates/dotfiles/src/bounded/guard.rs) preserves managed native executables beside their guarded replacements.
Runtime setup and the global mise postinstall hook protect all installed versions, so direct commands and commands selected by project tasks reach the same refusal.
The guard permits execution inside a container or on GitHub Actions.
This protects managed tool entrypoints against accidental native execution.
It is not an operating-system sandbox against an owner deliberately executing a backup, replacing the guard, or changing the CI environment.

The [pure rules](../../crates/rules/src/bounded_rules.rs) validate finite limits, select the execution route, and keep limit stops from providing passing evidence.
Their harnesses and rejecting probes live in the proof inventory.
Docker's cgroups, timeout behavior, filesystem replacement, and process dispatch remain external trust assumptions checked by integration tests.

## Alternatives

Repeated skill instructions do not stop a process.
PATH-only wrappers do not cover mise's task environment.
Replacing the owner's Docker engine or imposing a global VM allocation changes other projects and supplies no per-run boundary.
Disabling native CI verification would duplicate the isolation its dedicated runners already provide.

## Consequences

A limit stop reports the limiting resource and leaves verification inconclusive.
The default allocation is conservative for shared owner machines and has one policy source.
Guarded installations preserve the original tool for container or CI dispatch and survive reinstalling a tool or updating the runtime.
Container images must provide GNU timeout at the runner's required entrypoint.
Container logs rotate within the runner's capture budget.
