# Mark a PR ready only after every check passes

Status: accepted.

## Context

`pr-workflow ready` moved any open draft with an accepted document and a linked issue to review.
Marking a PR ready starts automatic review, so a draft marked ready while its checks still ran or failed spent review allowance on a head that would change again.

## Decision

`ready` reads the head commit's check rollup with the title, body, and state in one `gh pr view` request.
A check run passes only when it completed with a `SUCCESS` or `NEUTRAL` conclusion, and a commit status only when its state reads `SUCCESS`.
The command refuses the transition when the rollup holds no entries or any entry lacks a pass.
A cancelled or skipped run lacks a pass.
The proved transition core holds this decision in `checks_state` and `plan`.
A Kani harness proves the classification and the ready condition, and a counterexample that marks a draft with an unfinished check ready must fail.

GitHub shows no sign of finished remaining work, such as addressed review findings, so the pull-request skill keeps that part of the rule as an instruction.

## Alternatives

Reading `gh pr checks` lost because it exits with failure while checks run or fail, which the command runner reports as a GitHub failure instead of a refused transition.
Accepting skipped runs lost because a required workflow that skips shows no passing result.

## Consequences

A repository that reports no checks has no way to use `ready`, so such a destination needs a check before its PRs leave draft.
