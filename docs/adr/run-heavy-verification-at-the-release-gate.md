---
date: 2026-10-22
status: accepted
---
# Run heavy verification at the release gate

## Status

The owner accepted this boundary on 2026-10-22 under [issue 324](https://github.com/P4suta/dotfiles/issues/324).
It amends the shared-skill side of [the lightweight CI record](native-verification-with-lightweight-ci.md) and [optional native verification](optional-native-verification-before-publication.md).

## Context

[Optional native verification](optional-native-verification-before-publication.md) removed the three-host campaign from this repository's publication.
The shared skills still told every project to run its heavy checks before each push and on each pull request.
Projects followed them.
domyjob ran its Kani, Quint, and Miri inventory and its full test suite on the owner's Linux and Windows hosts before every push.
sivi and youarel ran every proof and mutation campaign before every push.
Each push and pull request waited for campaigns that a release needs, and daily development stalled.

## Decision

A commit, push, pull request, ready transition, or merge needs only the fast gate: format, lint, type checks, and tests on the current host or one pinned Linux runner.
Cross-compiled lint covers the other supported targets where the toolchain allows it.

The release gate runs the heavy checks on the exact candidate in the candidate verification stage of `release-workflow`.
They are the full proof inventory with model checks and Miri, fuzzing, mutation testing, stress repeats, and tests on every supported OS.
Publication waits for them to pass.
They still run in the project's bounded container or documented hosted runners, as [the bounded-tool rule](https://github.com/P4suta/dotfiles/issues/309) requires.

A change still carries the harnesses, models, and fuzz targets for its behavior, and the `assure` shape gate still requires them.
A developer runs an affected heavy check on demand when its answer guides the work, and that run gates nothing.
A proof that hasn't run is reported as unrun.

The `ci-budget`, `lefthook`, `development-assurance`, `executable-policy`, `verification-tools`, `formal-assurance`, and `mutation-testing` skills state this boundary.
Each project moves its own hooks and workflows in its own pull request.

## Alternatives

Selecting heavy checks by the changed contract kept every push correct against its proofs, and still ran the selected campaigns on most product changes.
Keeping them on pull requests but off hooks moved the wait from the push to the merge.
Dropping them entirely would leave releases without proof, fuzz, mutation, and cross-OS evidence.

## Consequences

Daily changes merge after the fast gate.
A proof failure, fuzz crash, surviving mutant, or platform defect can reach the default branch and surfaces at the next release candidate, which then needs a fix before publication.
A project without a release workflow runs its heavy checks only on demand until it gains one.
