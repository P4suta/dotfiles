# Scope the CodeRabbit Owner Pause

Status: Accepted.
Extends the owner pause of [ADR 0007](0007-reserved-review-capacity.md).

## Context

The CodeRabbit guard's owner pause was one marker that stopped every vendor call.
It could not express paused PR reviews with guarded CLI reviews still available, although the two draw from separate pools.
The PR workflow learned about a pause only from skill prose, so an agent could wait for, or request, a PR review that would not come.

## Decision

Give the pause a scope of `pr`, `cli`, or `all`, recorded as the machine-local markers `paused-pr`, `paused-cli`, and `paused` under `~/.local/state/coderabbit-guard`.
The existing `paused` marker keeps meaning `all`, and the effective scope is the union of the present markers.
`coderabbit --guard-status --json` reports the scope and each pool's state as `reviews.pr` and `reviews.cli`, alongside the rolling counts.
The guard refuses vendor calls only under a CLI pause, so a PR-only pause leaves guarded CLI reviews available.

`pr-workflow` reads the same markers through the guard's library function.
An exclusion made for the pause carries the standalone marker `<!-- coderabbit-pause -->` beside the ignore line; an ignore line without it is a standing exclusion.
While PR reviews are paused, it refuses CodeRabbit generation in create, edit, and ready.
It refuses `ready` unless the body carries the exclusion made for the pause, because the transition otherwise starts an automatic review or leaves the review unowed after resumption.
It refuses an edit of a ready PR that removes the ignore line, or that removes the pause marker from an exclusion made for the pause.
Each refusal names the pause and a runnable next action.
The live check and `ready` print whether a current-head CodeRabbit PR review is required, excluded, or not required during the pause.
After resumption, a PR excluded only for the pause reports the review as required again and names the edit and the authorized `@coderabbitai review` comment that restore it.
A review CodeRabbit itself paused, skipped, or rate-limited remains pending; only the owner's PR pause lifts the requirement.
The skills point at the guard status and these commands instead of restating the pause rules.

The pure scope, CLI admission, request, and requirement rules live in `xtask/src/review_rules.rs` and `xtask/src/pr_rules.rs` with Kani harnesses and rejecting counterexamples beside the existing ones.

## Alternatives

Refusing `ready` throughout a PR pause was rejected because readiness would then wait for a review that the pause makes unnecessary.
Treating every ignore line as pause-scoped was rejected because it would revoke standing exclusions on resumption.
Writing the scope into the existing `paused` file was rejected because the file holds free text, and a new reader would have to reinterpret old content.
Having `pr-workflow` run `coderabbit --guard-status` was rejected because the status read locks the review ledger and fails while a review holds it, while the markers are plain files.
Making the final check verify a completed current-head review was deferred, because CodeRabbit can publish an incremental completion only in its summary comment, and a heuristic gate would refuse finished work.

## Consequences

An owner can pause the PR pool alone and still run guarded CLI reviews.
The pause is machine-local, so `pr-workflow` honors only the markers on the host where it runs.
The PR requirement line reports the obligation; the agent still verifies the current-head review with `coderabbit-review`.
