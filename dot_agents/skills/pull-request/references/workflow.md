# Commands and generation

The dotfiles Rust xtask provides the installed `pr-workflow` executable.
Install it with `mise run install:pr-workflow`.
`check`, `create`, `edit`, and `issue` also need `mise run install:prose` for a personal destination.
The command always takes an exact `OWNER/REPO` and uses `github.com` even under an inherited `GH_HOST`.

## Issue-first preparation

The executable compiles in the [global policy](../assets/workflow-policy.json), so changing personal owners means reviewing that source and reinstalling.
The destination's live owner login and numeric ID must match the policy.
Personal forks follow upstream contribution rules, and collaborator permissions never make a repository personal.

```console
pr-workflow start --repo P4suta/project --issue 23
pr-workflow start --repo upstream/project
```

`start` only reads and never opens an issue.
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
`ready` requires an open draft with an accepted document and only passing head checks.
`edit` replaces title and body together and leaves draft state unchanged.
Local-file `check` never contacts GitHub and leaves the live issue prerequisite unmet, and `check --pr` only reads.

## CodeRabbit generation

The CLI pause marker stays machine-local across installation, and `coderabbit --guard-status` only reads local state.
Run reviews through the installed guard on the Mac, and run the heavy native checks on the Windows and Linux hosts.
The guard rounds down only the final integer of `floor(capacity * 0.9 * 0.8)`, so ten reviews yield seven.
Its Advanced local ceiling caps attempts at seven per rolling hour and 168 per rolling day.
Usage queries have matching rolling limits and need at least one minute between queries.
Failures consume reservations, concurrent invocations refuse, and missing or inconsistent evidence refuses review.
The guard runs the numeric preflight itself, so never call `usage` in a loop or before each review.
An unavailable or exhausted CLI allowance counts as a skipped local review with its actual reason, never as a clean review.

CLI reviews, IDE reviews, PR reviews, chat, and paid add-ons have separate usage pools.
Check PR quota with the [official guidance](https://docs.coderabbit.ai/management/rate-limits), including an authorized `@coderabbitai rate limit` comment, which starts no review.
Request at most one such status per hour and never trigger a review to discover capacity.
Only a fresh numeric observation for the correct developer, organization, and pool establishes a reserve.
The Advanced plan's 0–49-review activity band sets the PR refill rate, not a weekly PR allowance, and excludes the CLI pool.
Apply the same layered budget to a verified adaptive threshold and the hourly quota.
A confirmed highest-band threshold of 49 gives 35 events per rolling 168 hours.
Confirm the applicable window, because the official policy can use 24-hour or seven-day activity.
Hold ready publication, generation, and pushes that would start an automatic PR review while the required capacity stays unknown, and report the reason and reconsideration time.
After a one-hour hold, recheck once instead of pushing on a timer.
Keep paid usage inactive.
Other clients can spend capacity after a snapshot, so account-wide limits need service enforcement or a common reservation gateway.

Use the standard placeholders from the [official configuration reference](https://docs.coderabbit.ai/reference/configuration).
Put the summary request on its own line beside otherwise completed content.

```console
pr-workflow check --repo OWNER/REPO --generation coderabbit --title "@coderabbitai" --body-file generation.md
pr-workflow create --repo OWNER/REPO --generation coderabbit --head feature --base main --title "@coderabbitai" --body-file generation.md
pr-workflow ready --repo OWNER/REPO --generation coderabbit --pr 17
pr-workflow check --repo OWNER/REPO --pr 17 --final
```

CodeRabbit creation opens the PR for review.
Pass `--draft` when the task requires a draft.
Explicit CodeRabbit `ready` on a draft starts generation and skips final validation.
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
