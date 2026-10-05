# Recommend the next workflow step from inspected state

Status: accepted.

## Context

Agents working in this repository recall the order of the workflow from prose and memory.
The order: keep the host prerequisites, branch from the default branch, record skill decisions, commit, rebase, push, open a draft PR, wait for checks, and move it to review.
A refused gate names its own cause but not where the work stands in that order, so an agent still reconstructs the state and chooses the step itself.

## Decision

`just next` inspects the current state and prints the single recommended next step.
It first checks the host.
On Windows, it reports an unset `TEMP`, then an unmounted drive under `TEMP` or `CARGO_TARGET_DIR`, such as the Dev Drive, then a missing `TEMP` directory.
On other hosts, it reports only a missing system temporary directory.
It never checks which volume `TEMP` should use, because the repository defines no such location.
It reads the branch, uncommitted changes, divergence from the base, divergence from the upstream in both directions, and the skill decisions that `just check` requires.
It also reads the push pause in `~/.config/git/push-paused` and the CodeRabbit PR review pause.
The PR review pause combines the `paused` and `paused-pr` markers under `~/.local/state/coderabbit-guard`.
The `paused-cli` marker pauses only CLI reviews and holds no push.
Unless `--offline` applies, it fetches `origin` and reads the branch's PR, its base branch, its draft state, and its check rollup from GitHub.
The base comes from the open PR.
Without an open PR, and with `--offline`, the base comes from `origin/main`, so a stacked branch inspected offline reports the commits of the branch below it.

The output holds one JSON line on stdout with the action, whether it runs, a summary, a command, and the observed state, then a one-line summary on stderr.
The pure function in `xtask/src/next_action_rules.rs` orders the steps.
Kani harnesses prove five properties:

- Host prerequisites come first.
- Neither pause gets bypassed.
- Publication requires committed, decided, current work.
- A force-push replaces only upstream commits the branch once held.
- Each executable step runs only on its own preconditions.

A push to a PR in review starts a CodeRabbit review, so the CodeRabbit pause holds such a push and the move to review.
When both the branch and its upstream have their own commits, the branch's reflog decides between them.
The upstream tip may once have sat on the branch if you rebased or amended a pushed branch.
Then the step forces a push leased on the inspected upstream commit, after a range-diff.
The command exports `ALLOW_FORCE=1`, because the repository's git guard and pre-push gate refuse a rewrite of published history without it.
Otherwise the upstream holds commits the branch never had, such as a push from another clone, and the step rebases onto the upstream, so no recommendation discards them.
An expired or missing reflog yields the rebase, never the force-push.
An empty check rollup means the checks haven't reported yet, unless the repository defines no workflows.

`--execute` runs only a step that writes local state a single command restores.
Those steps create the temporary directory, fast-forward the default branch or a branch to its upstream, or draft a skill decision.
It then reports the step after it.
It refuses any other step and prints its summary and command, because commits, pushes, rebases, and PR transitions need the owner's authorization.

## Alternatives

Encoding the order in skills and agent instructions loses because nothing enforces prose or checks it with the state it describes.
Executing every step loses because the tool can't reverse publication or history rewrites, and it can't verify the authorization they need.
Reporting every pending problem at once loses because an agent needs one ordered step to proceed, and the observed state still carries every fact.

## Consequences

A new workflow prerequisite needs a field in the inspected state, a step in the ordering, and a case in the tests and proofs.
Instructions can point at `just next` instead of restating the workflow order.
