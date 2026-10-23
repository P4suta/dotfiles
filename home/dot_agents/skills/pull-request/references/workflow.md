# Commands and generation

The dotfiles Rust xtask provides the installed `pr-workflow` executable.
Install it with `just install-pr-workflow`.
`check`, `create`, `edit`, and `issue` also need `just install-prose` for a personal destination.
The command always takes an exact `OWNER/REPO` and uses `github.com` even under an inherited `GH_HOST`.

## Issue-first preparation

The executable compiles in the [global policy](../assets/workflow-policy.json), so changing personal owners means reviewing that source and reinstalling.
The destination's live owner login and numeric ID must match the policy.
Personal forks follow upstream contribution rules, and collaborator permissions never make a repository personal.

```console
pr-workflow start --repo P4suta/project --issue 23
pr-workflow start --repo upstream/project
```

`start` reads GitHub and records checked preparation in owner-local state without opening an issue.
`issue check`, `create`, and `edit` refuse an empty or unfinished document, and for a personal non-fork destination a title or body that fails the prose checker.

```console
pr-workflow issue check --repo P4suta/project --title "Keep edits" --body-file issue.md
pr-workflow issue create --repo P4suta/project --title "Keep edits" --body-file issue.md
pr-workflow issue edit --repo P4suta/project --number 23 --title "Keep edits" --body-file issue.md
```

For personal non-fork repositories, pass `--issue NUMBER` to create, edit, ready, and live checks, and put `Closes #NUMBER.` or another GitHub closing reference in the PR body.
Full issue URLs and Markdown links count only for the selected issue in the exact destination.
Comments, code examples, quoted instructions, PR numbers, closed issues, and other repositories' issues fail the prerequisite.
An external repository needs no personal issue, but the command still checks a supplied `--issue`.

## Authentication and quota

Online operations first read the authenticated `user` endpoint.
Missing or invalid quota headers, or a remaining count below the policy's nonzero reserve, stop further work.
Reads run serially with no quota poll or automatic retry.
Offline document checks need no authentication.
Without fork status, an offline check applies the writing standard to every repository of a personal owner.
`create`, `edit`, and the live check apply it only to a personal non-fork destination.

GitHub [REST rate limits](https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api) include secondary limits.
Trust response headers, never an assumed remaining count.
The reserve protects only this command's preflight, and the command never replays an uncertain mutation.
Stop on `403`, `429`, `Retry-After`, or exhausted quota, and wait as the provider directs.

## Local authoring

Prepare a UTF-8 body file that follows the destination template or the default `Why`, `Changes`, and `Validation` structure.
The command refuses `TODO`, `TBD`, and `FIXME` markers and named fields such as `{{description}}`, `[insert ...]`, and `<description>`.
Code, ordinary mentions, collapsible HTML sections, and GitHub Actions expressions stay valid.

```console
pr-workflow check --repo OWNER/REPO --title "fix: preserve edits" --body-file body.md
pr-workflow create --repo OWNER/REPO --head feature --base main --title "fix: preserve edits" --body-file body.md
pr-workflow edit --repo OWNER/REPO --pr 17 --title "fix: preserve edits" --body-file body.md
pr-workflow ready --repo OWNER/REPO --pr 17
pr-workflow check --repo OWNER/REPO --pr 17 --final
```

The default `local` mode always creates a draft.
Every generation mode starts as a draft before any review request.
`ready` requires an open draft with an accepted document and only passing head checks.
`edit` replaces title and body together and leaves draft state unchanged.
Local-file `check` never contacts GitHub and leaves the live issue prerequisite unmet, and `check --pr` only reads.

## Stacks and merging

The work-in-progress limit counts one open stack as one unit and each independent PR as one unit, with a default of three units.
Normal `start` requires new work to include the person's current open stack tip.
Normal `create` selects that tip as its base and links the new member into the stack.
Existing stack members can receive fixes in place, and adding a member never consumes another unit.
The inventory permits one owned stack and refuses branching stacks, cycles, and incomplete identities.

Declare an unrelated unit with either command:

```console
pr-workflow start --repo OWNER/REPO --issue 23 --path src --independent "Urgent correction unrelated to the open stack"
pr-workflow create --repo OWNER/REPO --issue 23 --head feature --title "fix: preserve edits" --body-file body.md --independent "Urgent correction unrelated to the open stack"
```

The reason stays in owner-local state and never enters the published document.
A declaration from `start` carries into `create` for the same repository identities, login, branch, issue, and descendant history.
`create` binds the declaration to the created PR number.
Reusing a merged branch name doesn't reuse its old declaration.
On another machine, declare an existing owned independent PR with `start --independent` from its branch before opening more work.
Every listed independent unit names its recorded reason, and a new unit at the cap refuses with the current inventory.
The flag keeps the issue, signing, review, dependency, and check requirements.

The owner's shared mise tools pin `gh-stack` on every profile.
pr-workflow resolves that installation outside the target repository and verifies its version.
The stack commands run it in a checkout whose `origin` matches the target.

```console
pr-workflow stack create --repo OWNER/REPO --issue 23 --head feature --base parent --title "feat: ..." --body-file body.md
pr-workflow stack sync --repo OWNER/REPO
pr-workflow stack rebase --repo OWNER/REPO [--continue | --abort]
pr-workflow stack merge --repo OWNER/REPO --pr 18
pr-workflow merge --repo OWNER/REPO --pr 17 --pr 19
```

`create` checks dependencies on every PR outside the selected parent chain, including when appending a stack member.
It skips PRs opened by bots identified by an `app/` login prefix, a `[bot]` login suffix, or the `Bot` account type.
It also skips stale PRs whose `mergeable` state is `CONFLICTING` with their own base.
Every skipped PR appears in the output with its reason.
For every remaining PR, `create` refuses a head that shares commits with it.
The refusal names `stack create` on that PR, or requires resolution before extending an existing stack.
When paths overlap, it runs `git merge-tree --write-tree` on the two heads using their merge base without changing the worktree or index.
A clean merge permits creation.
A conflict refuses creation and names the conflicting PR and the required next step.
Git execution failures stop creation, and no flag overrides a conflict.
`stack create` opens a draft on the parent's open PR and links the chain down to the trunk.
`stack sync` and `stack rebase` hold the repository's stack lease, the only way dotguard admits a rebase or leased force push of a published branch besides an `ALLOW_FORCE` waiver.
`merge` takes independent PRs in order.
It fetches the head and base before checking evidence, updates a behind head by merge, and waits for every head check.
It enforces the assessed review route, marks a draft ready, and merges the verified head with `--match-head-commit`.
`stack merge` restacks instead of updating and merges the stack up to the PR together.
When the trunk's active rulesets require a merge queue, both merge commands enqueue the verified heads and wait for every member to merge.
Queue admission alone grants no merge completion.
The structured result reports `merged`, `ejected`, `timed_out`, `head_changed`, or `closed`, and a stopped queue entry is never resubmitted automatically.
The head bound to the merge request must match the verified head.
Both stop with a structured refusal on a failed check, conflict, timeout, or missing evidence for the assessed CodeRabbit route.

## Importance assessment

Use the [importance criteria](importance.md) for every PR.
Prepare a bounded JSON assessment of the actual committed diff and sign it with the existing trusted Git identity:

```console
pr-workflow assess --repo OWNER/REPO --base origin/main --assessment-file importance.json
```

The command stores the signed statement in owner-local state, and every later command reads it from there.
Keep it out of the PR body, which refuses it.
Another machine reassesses instead of reading evidence from GitHub.
Pass the remote base, because a stale local `main` widens the bound diff.
The signed statement binds the exact repository, merge base, head, diff, and assessment.
Fork contributions also bind the verified source and destination repository identities.
Use the source owner's qualified head when publishing to an upstream repository.
Preparation can assess local commits before their authorized push, while publication verifies the actual published source branch.
It contains no private CLI output.
A base update or restack that preserves the exact diff and source identity carries the authenticated assessment to the new binding, signed again by the same trusted identity.
Any changed diff still requires a fresh assessment.

An ordinary bounded behavior change can use this input shape:

```json
{
  "level": "medium",
  "model": "The actual assessing model",
  "rationale": "The changed operation is bounded and recoverable under its existing checks.",
  "evidence": [{"path": "src/operation.rs", "change": "Describe the concrete changed behavior and effective recovery."}],
  "severe": null
}
```

Evidence paths must occur in the current diff.
For `high` and `critical`, replace `severe` with the required concrete ground, consequence, likelihood, blast radius, rollback reasons, and their closed categories.
The executable rejects unknown fields, unsupported levels, and an assessment that fails the severe threshold.
The assessment's factual accuracy remains the accountable agent's responsibility.

For a required `high` review, pass the guard's signed private receipt with `--cli-receipt PATH` to the publication command.
Produce it on the managed review executor with:

```console
coderabbit review-committed --base-commit FULL_MERGE_BASE_SHA --receipt NEW_PRIVATE_FILE
```

Keep the receipt outside the reviewed checkout and write it to a new destination file.
Don't embed that receipt, raw diagnostics, or authentication data in the PR body.
Missing or stale required review evidence holds readiness and merge.
`critical` uses actual current-head PR review evidence and doesn't accept a CLI receipt as a substitute.
Reassess a published high route with `assess --pr NUMBER --prior-cli-receipt PATH` before changing its route.
Keep that private prior receipt available to later checks.
Its authenticated findings remain obligations while its old completion supplies no credit for the new diff.
Put every returned finding's ID, resolved or unsupported disposition, concrete reason, and current-source evidence in the signed assessment.

## CodeRabbit generation

Complete local checks and the authenticated importance assessment before the selected review.
`coderabbit --guard-status --json` reports the owner's pause scope: `reviews.pr` and `reviews.cli` each read `paused` or `allowed`.
Keep the standalone `@coderabbitai ignore` description line for `low`, `medium`, and `high`, with no `review:critical` opt-in label.
Only the current `critical` route permits CodeRabbit generation and PR opt-in.
An owner pause blocks the selected required route rather than lifting its obligation.
The live check and `ready` report the current route and any missing evidence.
The guard preserves its CLI budget and pause.
An unavailable allowance leaves a required CLI review pending.

CLI reviews, IDE reviews, PR reviews, chat, and paid add-ons have separate usage pools.
Check PR quota with the [official guidance](https://docs.coderabbit.ai/management/rate-limits), including an authorized `@coderabbitai rate limit` comment, which starts no review.
Request at most one such status per hour and never trigger a review to discover capacity.
Only a fresh numeric observation for the correct developer, organization, and pool establishes a reserve, and a response that says reviews exist gives none.
The Review Usage dashboard shows historical rates, not current capacity.
The Advanced plan's 0–49-review activity band sets the PR refill rate, not a weekly PR allowance, and excludes the CLI pool.
Apply the same layered budget to a verified adaptive threshold and the hourly quota.
A confirmed highest-band threshold of 49 gives 35 events per rolling 168 hours.
Confirm the applicable window and current counts, because the official policy can use 24-hour or seven-day activity.
Hold ready publication, generation, and pushes that would start an automatic PR review while the required capacity stays unknown, and report the reason and reconsideration time.
After a one-hour hold, recheck once instead of pushing on a timer.
Keep paid usage inactive and honor an owner-imposed pause until the owner resumes.
The CLI guard and checked PR workflow enforce their managed entry points.
Ordinary Git and GitHub clients can bypass them.
Other clients can spend capacity after a snapshot, so account-wide limits need service enforcement or a common reservation gateway.

Use the standard placeholders from the [official configuration reference](https://docs.coderabbit.ai/reference/configuration).
Put the summary request on its own line beside otherwise completed content.

```console
pr-workflow check --repo OWNER/REPO --generation coderabbit --title "@coderabbitai" --body-file generation.md
pr-workflow create --repo OWNER/REPO --generation coderabbit --head feature --base main --title "@coderabbitai" --body-file generation.md
pr-workflow ready --repo OWNER/REPO --generation coderabbit --pr 17
pr-workflow check --repo OWNER/REPO --pr 17 --final
```

CodeRabbit creation also starts as a draft with a current critical assessment.
A permitted ready transition must meet the selected route and the destination's checks.
For `critical`, readiness can precede the PR review, but final validation and merge require its observed completion.
Add the critical opt-in label or request a full review only after verifying the corresponding capacity and authorization.
The final check rejects remaining placeholders.
Check the generated document for destination rules, intent, and truthful validation results, then correct it with `edit` when authorized.
Use [the configuration example](../assets/coderabbit.yaml) only for authorized configuration work, merged with the destination's existing rules.
The example puts the summary in the description.
If the destination uses the walkthrough comment, copy that summary into the body with `edit` before final validation.

## Destination title exception

When an upstream project requires another format, pass `--title-policy policy.json` to every relevant command.
The JSON names one repository and cites the rules that justify turning off the Conventional Commits check.
Nonempty single-line titles and placeholder checks still apply.

```json
{
  "repository": "upstream/project",
  "source": "https://github.com/upstream/project/blob/main/CONTRIBUTING.md",
  "reason": "The contribution guide requires a sentence title without a type prefix."
}
```

## Boundaries

GitHub can change an issue or repository after the pre-mutation inspection, so read back the result and claim no remote atomic transaction.
Avoid concurrent document edits during `ready`, because GitHub offers no compare-and-swap for the inspected title and body.
