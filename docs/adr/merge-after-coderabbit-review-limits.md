---
status: accepted
date: 2026-11-13
---
# Merge after CodeRabbit review limits

## Context

CodeRabbit documents that a [rate-limited review](https://docs.coderabbit.ai/management/rate-limits#when-a-review-is-rate-limited) posts a passing check even though no review ran.
The [explicit-merge policy](honor-explicit-merges-during-optional-review-pauses.md) still requires a separate decision for each affected PR.

## Decision

For an already authorized personal non-fork PR merge, proceed when the current-head CodeRabbit PR review is rate-limited without asking again.
Verify the provider's actual quota notice, exact head, passing required checks, and resolved supported findings.
Use normal GitHub admission with [head matching](https://cli.github.com/manual/gh_pr_merge), preserving repository-required reviews, checks, merge queues, and publication holds.
Other unavailable or failed analysis remains outside the exception.

## Alternatives

Waiting for fresh analysis could reveal more defects but delays authorized merges until quota refills.

## Consequences

Missing optional analysis might leave defects undetected.
Report the missing review without granting review completion or manufacturing a provider status.
Don't enable paid usage, retry reviews, or use CLI analysis to complete a quota-limited merge.
Reconsider if a destination requires the analysis or the permission changes.

## Confirmation

The user accepted the standing quota exception on 2026-11-13.
[PR #727](https://github.com/P4suta/dotfiles/pull/727) exercised normal GitHub admission after its required checks passed while CodeRabbit reported a review limit.
