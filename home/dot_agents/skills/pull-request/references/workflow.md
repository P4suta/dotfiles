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

## Existing PR issue references

The authenticated numeric destination repository owner can add an open issue reference to an existing PR, including a contributor's fork PR.

```console
pr-workflow link-issue --repo OWNER/REPO --pr 17 --issue 23
```

The command checks the exact open issue, PR, source network, head, and base.
It keeps the current title and body prefix, draft state, creator, labels, review requests, assessment markers, and CodeRabbit directives.
A matching reference leaves the document unchanged.
The command adds only one closing-reference paragraph through a body-only update.
It rechecks state before writing and requires a matching acknowledgement and fresh readback.
An uncertain write stops without retrying and names the PR to inspect.
The write has no atomic comparison condition, so concurrent edits between the final read and the write remain an external assumption.
Source approval, assessments, review completion, readiness, and merging continue through their existing gates.

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
It marks a draft ready and merges the verified head with `--match-head-commit`.
It stops when required CodeRabbit review evidence is missing.
`stack merge` restacks instead of updating and merges the stack up to the PR together.
When the trunk's active rulesets require a merge queue, both merge commands enqueue the verified heads and wait for every member to merge.
Queue admission alone grants no merge completion.
The structured result reports `merged`, `ejected`, `timed_out`, `head_changed`, or `closed`, and a stopped queue entry is never resubmitted automatically.
The head bound to the merge request must match the verified head.
Both stop on a failed check, conflict, timeout, or missing CodeRabbit review.

## Source assessment

Use the [risk criteria](importance.md) to describe the actual changed behavior.
The checked tool keeps its existing repository, source, merge-base, head, and diff binding in owner-local state.
That assessment never selects whether CodeRabbit runs.

```console
pr-workflow assess --repo OWNER/REPO --base origin/main --assessment-file importance.json
```

A bounded assessment uses the existing input shape:

```json
{
  "level": "medium",
  "model": "The actual assessing model",
  "rationale": "The changed operation is bounded and recoverable.",
  "evidence": [{"path": "src/operation.rs", "change": "Describe the changed behavior."}],
  "severe": null
}
```

Keep assessment records, private diagnostics, and local operating instructions out of PR descriptions.
Use a qualified source head for a verified fork contribution.
A changed diff needs a current assessment, and supported earlier findings remain obligations.

## Automatic CodeRabbit reviews

Every importance level uses CodeRabbit PR review.
Enable automatic reviews for ready PRs and updates without positive label filters.
Leave draft review off unless the repository needs early feedback.
Read back the effective service configuration because local instructions don't configure the provider.

Ready admission waits for unfinished, failed, or missing required checks.
A repository that reports and requires no checks can accept a ready PR.
The review that readiness starts can finish afterward.
Final validation and merge still require the review of the current head and resolved supported findings.

An owner pause keeps the affected operation pending without inserting a command into the PR description.
Keep subscription limits, separate PR and CLI pools, and existing billing controls.
Use a guarded CLI review for an explicit local review request.

Use the prepared [configuration](../assets/coderabbit.yaml) for authorized configuration work.
The [automatic review controls](https://docs.coderabbit.ai/configuration/auto-review) define eligibility, labels, and updates.
Check the current [rate limits](https://docs.coderabbit.ai/management/rate-limits) for the actual developer, organization, and pool.
Never start a review merely to discover capacity or enable paid usage to continue.

## CodeRabbit generation

Finish local checks and prepare a complete document before requesting generation.
Honor the owner's pause and verify the applicable capacity.
The standard placeholders follow the [configuration reference](https://docs.coderabbit.ai/reference/configuration).

```console
pr-workflow check --repo OWNER/REPO --generation coderabbit --title "@coderabbitai" --body-file generation.md
pr-workflow create --repo OWNER/REPO --generation coderabbit --head feature --base main --title "@coderabbitai" --body-file generation.md
pr-workflow ready --repo OWNER/REPO --generation coderabbit --pr 17
pr-workflow check --repo OWNER/REPO --pr 17 --final
```

Creation starts as a draft.
Check generated text for destination rules, scope, and actual validation.
Correct it with `edit` when authorized.
A generation failure supplies no completed document.

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
