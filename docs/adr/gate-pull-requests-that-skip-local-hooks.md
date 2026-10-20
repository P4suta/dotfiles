---
date: 2026-10-20
status: accepted
---
# Gate pull requests that skip the local hooks

## Status

The owner accepted this gate on 2026-10-20 under [issue 241](https://github.com/P4suta/dotfiles/issues/241).
It amends the hosted boundary in [the lightweight CI record](native-verification-with-lightweight-ci.md) and [optional native verification](optional-native-verification-before-publication.md).

## Context

Hosted CI reports the checked-out source and runs no implementation check, because the owner's local pre-push gate checks every owner change.
Renovate pull requests never pass through those hooks.
Renovate merged [#196](https://github.com/P4suta/dotfiles/pull/196) after a hosted run that checked nothing, and `just check` failed on the Mac because files that name the Rust release still named the old one.
An automatic merge that trusts a check of nothing trusts nothing.

## Decision

The [workflow](../../.github/workflows/ci.yml) gains a `dependencies` job on one Linux runner, and the `required` job waits for it.
For a pull request whose author isn't the repository owner, the job installs the pinned mise release and `rust` and `just` from `mise.lock`, then runs `just dependency-gate`.
That recipe compiles the workspace with `cargo check --locked --workspace --all-targets` and runs [the pin tests](../../xtask/tests/pins.rs), which refuse an update that moves a pin without the files that name it.
For an owner pull request and the merge queue, the job reports that the local gate covers the change and succeeds.
The ruleset keeps its required status names, so the rule needs no change.
The [workflow checker](../../xtask/src/quality.rs) and [its regressions](../../xtask/tests/workflows.rs) protect the new job as they protect the rest of the contract.

## Alternatives

Running the gate on every pull request repeats the local gate for the owner's changes.
Renovate's post-upgrade commands could update the dependent files, but the hosted Renovate app doesn't run them.
A schedule that checks main after a merge finds the breakage only after the merge.

## Consequences

A Renovate update that compiles and keeps every pin consistent merges on its own, and any other update waits for the owner.
A Rust update now fails the gate until the owner updates the native materials catalog and the fixtures that name the release.
The gate costs a few Linux minutes on each pull request from someone else, and nothing on the owner's.
