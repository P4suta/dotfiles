# Commands and generation

The dotfiles Rust xtask provides the installed `pr-workflow` executable and the same code under its `pr-workflow` subcommand.
Install it with `mise run install:pr-workflow` from the reviewed dotfiles checkout.
For a personal destination, `check`, `create`, `edit`, and `issue` also need the prose checker from `mise run install:prose`.
Run the installed executable from any destination repository with that host's authenticated `gh` session.
The command always takes an exact `OWNER/REPO`, never infers a publication target, and uses `github.com` even under an inherited `GH_HOST`.

## Issue-first preparation

The maintained executable compiles in the [global policy](../assets/workflow-policy.json), so its ownership rule applies everywhere without a repo-local override.
Changing the personal owners requires reviewing that dotfiles source and reinstalling.
The destination's live owner login and numeric ID must match the policy, and personal forks follow upstream contribution rules.
Administrator or collaborator permissions never make a repository personal.

```console
pr-workflow start --repo P4suta/project --issue 23
pr-workflow start --repo upstream/project
```

Preparation stays read-only and checks the issue before implementation.
Read the problem, scope, and acceptance criteria with `gh issue view`.
The command checks existence, identity, open state, and incomplete content without requiring fixed headings or judging prose quality.
Preparation never opens an issue, and opening one takes its own authorization.
`pr-workflow issue check`, `create`, and `edit` refuse an empty or unfinished issue document before `gh` publishes it.
For a personal non-fork destination, they also refuse a title or body that fails the prose checker.

```console
pr-workflow issue check --repo P4suta/project --title "Keep edits" --body-file issue.md
pr-workflow issue create --repo P4suta/project --title "Keep edits" --body-file issue.md
pr-workflow issue edit --repo P4suta/project --number 23 --title "Keep edits" --body-file issue.md
```

For personal non-fork repositories, pass `--issue NUMBER` to create, edit, ready, and live checks, and put `Closes #NUMBER.` or another GitHub closing reference in the PR body.
Full issue URLs and Markdown links count only for the selected issue in the exact destination.
Comments, code examples, quoted instructions, PR numbers, closed issues, and other repositories' issues fail the prerequisite.
An external repository needs no personal issue, but the command still checks a supplied `--issue` for open state and a closing reference.

## Authentication and quota

Online operations first read the authenticated `user` endpoint through the host's `gh` session, because public repository access alone proves no authentication.
Every API prerequisite response carries quota headers.
Missing or invalid quota evidence, or a remaining count below the policy's nonzero reserve, stops further work.
These reads run serially, authenticate once per command, and add no quota poll or automatic retry.
Offline document checks make no API calls and need no authentication.
Without fork status, an offline check applies the writing standard to every repository of a personal owner.
`create`, `edit`, and the live check apply it only to a personal non-fork destination.

GitHub [API rate limits](https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api) give authenticated usage a larger allowance than anonymous usage and add secondary limits.
Trust actual response headers and diagnostics, never an assumed account-wide remaining count.
The reserve protects only this command's preflight.
`gh` may also use GraphQL, and other consumers can exhaust either resource afterward.
The command never intercepts other tools or replays an uncertain mutation.
Stop on `403`, `429`, `Retry-After`, or exhausted quota, and wait as the provider directs before a later attempt.

## Local authoring

Prepare a UTF-8 body file that follows the destination template or the default `Why`, `Changes`, and `Validation` structure, with actual content outside HTML comments.
The command refuses known unfinished markers: standalone or labelled `TODO`, `TBD`, and `FIXME`, and named fields such as `{{description}}`, `[insert ...]`, and `<description>`.
Fenced and inline code, ordinary mentions of these words, collapsible HTML sections, and GitHub Actions expressions stay valid.
The scanner leaves natural-language completeness to the writer.

```console
pr-workflow check --repo OWNER/REPO --title "fix: preserve edits" --body-file body.md
pr-workflow create --repo OWNER/REPO --head feature --base main --title "fix: preserve edits" --body-file body.md
pr-workflow edit --repo OWNER/REPO --pr 17 --title "fix: preserve edits" --body-file body.md
pr-workflow ready --repo OWNER/REPO --pr 17
pr-workflow check --repo OWNER/REPO --pr 17 --final
```

The default `local` mode always creates a draft, even for a document that passes validation, and no local flag creates a ready PR.
`ready` reads the current title, body, state, and head check rollup from GitHub, and requires an open draft with an accepted document and only passing checks.
`edit` replaces title and body together and leaves draft state unchanged.
Local-file `check` never contacts GitHub, and live `check --pr` only reads.
An offline document check leaves the live issue prerequisite unmet.
Create and edit send an owned temporary copy of the checked body with `gh --body-file`, so later edits to the input file never reach the published bytes.

## Service generation

Complete local checks before one budget-qualified command-line review of an initial substantive change.
An owner-imposed CodeRabbit pause stops generation, service reviews, and quota inquiries until explicit resumption.
The command-line pause marker stays machine-local across installation, and `coderabbit --guard-status` only reads local state.
Run reviews through the installed guard on the Mac, and run the heavy native checks on the Windows and Linux hosts.
The guard admits at most `floor(capacity * 0.9 * 0.8)` consumed reviews and rounds down only the final integer, so ten reviews yield seven.
Its Advanced local ceiling caps attempts at seven per rolling hour and 168 per rolling day.
Usage queries have their own matching rolling limits and need at least one minute between queries.
These local traffic limits claim no provider API allowance.
Failures consume reservations, concurrent invocations refuse, and missing or inconsistent evidence refuses review.
The guard runs the numeric service preflight itself, so never call `usage` in a loop or before each review.
An unavailable or exhausted command-line allowance counts as a skipped local review with its actual reason.
It never counts as a clean review or bypasses the separate PR publication gate.

Command-line reviews, IDE reviews, PR reviews, chat, and paid add-ons have separate usage pools.
Check PR quota with the [official guidance](https://docs.coderabbit.ai/management/rate-limits), including an authorized `@coderabbitai rate limit` comment, which starts no review.
Request at most one such status per hour, read the response with bounded reads, and never trigger a review to discover capacity.
Only a fresh numeric observation for the correct developer, organization, and pool establishes a reserve, and a message about available reviews establishes none.
The Review Usage dashboard shows historical rates and reserves no current capacity.
The Advanced plan's 0–49-review activity band sets the PR refill rate.
It sets no weekly PR allowance and excludes the separate command-line pool.
Apply the same layered budget to a verified adaptive threshold and the hourly quota.
A confirmed highest-band threshold of 49 gives 35 events per rolling 168 hours.
Confirm the applicable window and current counts, because the official policy can use 24-hour or seven-day activity.
Hold ready publication, generation, and pushes that would start an automatic PR review while the required numeric capacity stays unknown.
Report the reason and reconsideration time.
After a one-hour hold, recheck once instead of assuming a refill or pushing on a timer.
Keep paid usage inactive and honor an owner-imposed hold until explicit resumption.
The command-line guard enforces its managed entry points, but these publication decisions remain manual, and this PR command leaves ordinary Git and GitHub clients unchecked.
Other clients can spend capacity after a snapshot, so account-wide limits need service enforcement or a common reservation gateway.

Use the standard placeholders from the [official configuration reference](https://docs.coderabbit.ai/reference/configuration).
Put the summary request on its own line and keep the required template fields and factual validation results.
The explicit mode permits only the exact title placeholder and the standard standalone summary request beside otherwise completed content.

```console
pr-workflow check --repo OWNER/REPO --generation coderabbit --title "@coderabbitai" --body-file generation.md
pr-workflow create --repo OWNER/REPO --generation coderabbit --head feature --base main --title "@coderabbitai" --body-file generation.md
pr-workflow ready --repo OWNER/REPO --generation coderabbit --pr 17
pr-workflow check --repo OWNER/REPO --pr 17 --final
```

CodeRabbit creation opens the PR for review and generation.
Pass `--draft` when the authorized task requires a draft.
For an existing draft with the standard requests, explicit CodeRabbit `ready` lets the requests start the normal review workflow.
This transition starts generation and skips final validation.
The final check rejects remaining placeholders, even with `--generation coderabbit --final`.
Check the generated document for destination rules, intent, and truthful validation results, then correct it with checked `edit` when authorized.
Use [the configuration example](../assets/coderabbit.yaml) only for authorized configuration work, merged with the destination's existing rules.
Publishing the PR or installing this skill grants no authority over organization or account settings.
The example puts the summary in the description.
If the destination puts it in the walkthrough comment instead, copy that summary into the body with checked `edit` before final validation.

## Destination title exception

When an upstream project requires another format, pass `--title-policy policy.json` to every relevant command.
The JSON names one repository and cites the rules that justify turning off the Conventional Commits check.
Nonempty single-line titles and unfinished-placeholder checks still apply.
The exception enforces no replacement grammar, so check the upstream format by hand.

```json
{
  "repository": "upstream/project",
  "source": "https://github.com/upstream/project/blob/main/CONTRIBUTING.md",
  "reason": "The contribution guide requires a sentence title without a type prefix."
}
```

## Boundaries

GitHub authorization, remote concurrency, service generation, and the truth of validation claims stay outside these checks.
GitHub can change an issue or repository after the pre-mutation inspection, so read back the result and claim no remote atomic transaction.
Avoid concurrent document edits during `ready`, because GitHub offers no compare-and-swap for the inspected title and body.
Report any divergence or incomplete generation.
