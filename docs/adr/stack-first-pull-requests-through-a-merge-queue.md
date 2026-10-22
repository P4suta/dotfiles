---
status: proposed
date: 2026-10-22
---
# Make one stack the unit of open work and merge it through a merge queue

## Context

pr-workflow caps each person at three open pull requests per repository and merges each pull request on its own.
The personal repository baseline makes required checks strict, so each merge leaves every other open pull request behind its base.
Each update then reruns the full CI before the next merge can land.
In aozora, a run of ten small hygiene pull requests needed about two full CI runs each, and the cap held finished work back.

GitHub merges a stack of pull requests as one atomic operation and applies the rules of the stack base to every member.
A workflow that runs on pull requests targeting the default branch runs for every member of a stack.
A merge queue accepts a whole stack in order, and its workflows run on the `merge_group` event.
Stacks need no setting, but they don't support automatic merging or the legacy synchronous merge endpoint.
`pr-workflow stack` already creates, syncs, rebases, and merges stacks through the pinned gh-stack, yet it resolves gh-stack from a pin in each target repository, which most repositories lack.
The dotfiles policy in `policy/github.toml` already names a merge queue without strict checks, and its CI runs on `merge_group`.
The live dotfiles ruleset still requires strict checks without a queue, and no pr-workflow path merges through one.

The same aozora run showed three defects in the merge path.
An automatic update by merge from the base invalidated the signed assessment, even when the pull request's diff stayed the same.
`pr-workflow merge` then failed on a missing revision, because it didn't fetch the updated head.
It also refused while checks were still running, instead of waiting for them.

## Decision

Each person keeps at most one open stack per repository, and new work joins its top by default.
`pr-workflow start` expects a new branch to start from the top of that person's open stack, and `create` adds it to the stack.
With no open stack, `create` opens the first member of a new one.
An unrelated change may open as an independent pull request only through `create --independent`, which records its reason in owner-local state and in the pull request body.
The work-in-progress limit counts units instead of pull requests, with a default of three per person and repository.
The open stack counts as one unit, and each independent pull request counts as one unit.
A member that can't pass gets fixed in place, moved, or removed, and `stack merge --pr` lands the passing members below it.

`pr-workflow stack merge` restacks the stack, fetches every updated head, and waits for running checks up to its timeout.
It merges through the merge queue when the base requires one, and merges the stack directly otherwise.
When a restack or base update keeps a member's diff, its signed assessment carries over to the new head.
Any change to a member's diff still needs a new assessment.
pr-workflow runs the owner-provisioned gh-stack in every repository, and the shared tool list pins it.

The personal repository baseline requires a squash merge queue on a default branch that requires status checks.
With the queue, the ruleset stops requiring an up-to-date branch, because the queue tests each change on the current base.
`repo-settings` reports a workflow that reports a required check without a `merge_group` trigger, because the queue would never receive that check.
It adds the queue only after every such workflow has the trigger.

## Consequences

A stack merges all or nothing, so one failing member blocks every later member until someone fixes, moves, or removes it.
Related changes line up in one stack, and each member still gets its own review and checks.
An urgent or unrelated change can leave that coupling only with a recorded reason, so independent pull requests stay visible exceptions.
Stack members need no workflow change, but a repository needs a `merge_group` trigger before it turns the queue on.
The queue ends the update-and-rerun cycle for every merge, and the cap stops holding finished work back.
`pr-workflow merge` keeps working for a stack of one.

## Alternatives

A merge queue without stacks removes the update cycle, but keeps the per-pull-request cap and leaves dependent changes to manual rebases.
Stacks with no escape keep one line of work, but make an urgent fix wait behind every failing member below it.
A higher cap keeps one CI cycle per merge and the update cycle that strict checks cause.
One combined pull request runs CI once, but reviewers lose the split by change and the link from each issue to its own pull request.
