# Recommend the next workflow step from inspected state

Status: Accepted.

## Context

Agents working in this repository recall the order of the workflow from prose and memory: keep the host prerequisites, branch from the default branch, record skill decisions, commit, rebase, push, open a draft PR, wait for checks, and move it to review.
A refused gate names its own cause but not where the work stands in that order, so an agent still has to reconstruct the state and choose the step itself.

## Decision

`just next` (`dotfiles-xtask next`) inspects the current state and prints the single recommended next step.
It first checks the host.
On Windows, it reports an unset `TEMP`, then an unmounted drive under `TEMP` or `CARGO_TARGET_DIR`, such as the Dev Drive, then a missing `TEMP` directory.
On other hosts, it reports only a missing system temporary directory.
It does not check which volume `TEMP` should be on, because the repository does not define that location.
It reads the branch, uncommitted changes, divergence from the base, divergence from the upstream in both directions, the skill decisions that `just check` requires, the push pause in `~/.config/git/push-paused`, and the CodeRabbit PR review pause.
The PR review pause is the union of the `paused` and `paused-pr` markers under `~/.local/state/coderabbit-guard`; the `paused-cli` marker pauses only CLI reviews and does not hold a push.
It fetches `origin` and reads the branch's PR, its base branch, its draft state, and its check rollup from GitHub unless `--offline` is given.
The base is the open PR's base branch; without an open PR, and with `--offline`, it is `origin/main`, so a stacked branch inspected offline reports the commits of the branch below it.

The output is one JSON line on stdout with the action, whether it is executable, a summary, a command, and the observed state, followed by a one-line summary on stderr.
The pure function in `xtask/src/next_action_rules.rs` orders the steps.
Kani harnesses prove that host prerequisites come first, that neither pause is bypassed, that publication requires committed, decided, current work, that a force-push replaces only upstream commits the branch once held, and that each executable step is chosen only on its own preconditions.
A push to a PR in review starts a CodeRabbit review, so the CodeRabbit pause holds such a push as well as the move to review.
When both the branch and its upstream have their own commits, the branch's reflog decides between them.
If the upstream tip was once on the branch, as after rebasing or amending a pushed branch, the step is a force-push leased on the inspected upstream commit, after a range-diff.
The command sets `ALLOW_FORCE=1`, because the repository's git guard and pre-push gate refuse a rewrite of published history without it.
Otherwise the upstream holds commits the branch never had, such as a push from another clone, and the step is to rebase onto the upstream, so no recommendation discards them.
An expired or missing reflog yields the rebase, never the force-push.
An empty check rollup means the checks have not reported yet, unless the repository defines no workflows.

`--execute` runs only a step that writes local state a single command restores: creating the temporary directory, fast-forwarding the default branch or a branch to its upstream, or drafting a skill decision.
It then reports the step after it.
Any other step is refused with its summary and command, because commits, pushes, rebases, and PR transitions need the owner's authorization.

## Alternatives

Encoding the order in skills and agent instructions was rejected because prose is neither enforced nor checked against the state it describes.
Executing every step was rejected because publication and history rewrites are not reversible by the tool and need authorization the tool cannot verify.
Reporting every pending problem at once was rejected because a single ordered step is what an agent needs to proceed, and the observed state still carries every fact.

## Consequences

A new workflow prerequisite needs a field in the inspected state, a step in the ordering, and a case in the tests and proofs.
Instructions can point at `just next` instead of restating the workflow order.
