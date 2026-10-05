# Mark a PR ready only after every check passes

Status: Accepted.

## Context

`pr-workflow ready` moved any open draft with an accepted document and a linked issue to review.
Marking a PR ready starts automatic review, so a draft marked ready while its checks were still running or failing spent review allowance on a head that would change again.
A PR leaves draft only when every required check passes and all of its work is finished, immediately before merge.

## Decision

`ready` reads the head commit's check rollup together with the title, body, and state in the same `gh pr view` request.
A check run counts as passing only when it completed with a `SUCCESS` or `NEUTRAL` conclusion, and a commit status only when its state is `SUCCESS`.
The transition is refused when the rollup is empty or any entry is unfinished or did not pass, including cancelled and skipped runs.
The decision is part of the proved transition core: `checks_state` and `plan` carry the requirement, a Kani harness proves the classification and the ready condition, and a counterexample that marks a draft with an unfinished check ready must fail.

Completion of the remaining work, such as addressed review findings, cannot be observed from GitHub alone, so the pull-request skill keeps that part of the rule as an instruction.

## Alternatives

Reading `gh pr checks` was rejected because it exits unsuccessfully while checks are pending or failing, which the command runner reports as a GitHub failure rather than a refused transition.
Accepting skipped runs was rejected because a required workflow that skips does not establish a passing result.

## Consequences

A PR whose checks are still running cannot be marked ready, so the transition waits for the final head's results.
A repository without any reported check cannot use `ready`; such a destination needs a check before its PRs leave draft.
