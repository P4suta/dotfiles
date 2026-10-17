---
date: 2026-10-17
status: proposed
---
# Run spoken Git commands in the matching policy companion

## Context

Commit and push notifications add code to a Git frontend that starts for every command.
The complete implementation exceeds its fixed binary ceiling even after removing redundant allocations and comparing standard compiler settings.
The [existing source-matched companion](renovate-behind-a-source-matched-process.md) already ships beside the frontend.

## Decision

Delegate commit and push to that companion through its existing compiled source-hash comparison.
Share the Git policy and process handling in [one module](../../crates/dotguard/src/git_wrapper.rs).
Its constant parameter enables speech only in the companion.
Other Git commands and the frequent hook gates keep their frontend entry points.
The direct `post-commit` CLI also delegates its cold signing implementation to the companion.

The managed post-commit hook starts its current runtime as a hidden source-matched signing worker.
It keeps the same hook environment and refusal relay.
The worker calls the same signing and rollback implementation without loading the companion.
Git ignores post-commit exit failures, so an incomplete companion installation must never prevent the managed hook from rolling back an unsigned commit.

The companion forms part of the required Git policy implementation for commit and push.
A missing or mismatched companion refuses execution before Git runs.
Never retry Git through a fallback after companion execution.
Notification runtime, queue, and player failures remain nonfatal and preserve Git's result.

Unix replaces the frontend process.
Windows uses the same console handler at both waiting boundaries and passes through the child status and streams.
The existing installation places both binaries and restores the earlier installation when the install command fails.
Files may change individually during installation, so the source-hash comparison also refuses an incomplete update.

## Alternatives

Keeping the notification implementation reachable from the frontend exceeds its unchanged binary ceiling.
Increasing the ceiling weakens the existing admission condition.
Adding another executable duplicates the installation and source identity boundary.

## Consequences

Ordinary commands load the frontend without the speech implementation.
Commit and push require the existing adjacent companion from the same build.
The [wrapper tests](../../crates/dotguard/tests/git_wrapper.rs) cover paired execution and optional notification failures.
The [companion tests](../../crates/dotguard/tests/push_and_signing.rs) cover missing and mismatched implementations before Git execution.
The [managed hook tests](../../crates/dotfiles/tests/hook_postcommit.rs) cover signing and empty-commit rollback without an installed companion.
The [process count tests](../../crates/dotfiles/tests/hook_exec_count.rs) count the actual worker's self-reentry through the production hook dispatcher.
The fixed benchmark and native gates remain the admission checks.
