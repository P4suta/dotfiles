# Commands and Generation

The dotfiles Rust xtask provides the installed `pr-workflow` executable and the same implementation under its `pr-workflow` subcommand.
Install through the maintained `mise run install:pr-workflow` task from the reviewed dotfiles checkout.
Use the installed native executable from any destination repository; it requires the destination host's own authenticated `gh` session.
The command always takes an exact `OWNER/REPO` and never infers a publication target.
Its GitHub host is `github.com`, including when another `GH_HOST` is inherited.

## Issue-first preparation

The [global policy](../assets/workflow-policy.json) is compiled into the maintained executable, so its ownership rule applies across repositories without a repo-local override.
Changing the personal owners requires reviewing that dotfiles source and reinstalling the executable.
The destination's live owner login and numeric ID must agree with the policy; fork status keeps personal forks under upstream contribution rules.
Admin or collaborator permissions do not make a repository personal.

```console
pr-workflow start --repo P4suta/project --issue 23
pr-workflow start --repo upstream/project
```

Preparation is read-only and checks the issue before implementation.
Use `gh issue view` to read its problem, scope, and acceptance criteria; the command checks existence, identity, open state, and mechanically incomplete content.
It does not require fixed issue headings or assess the quality of the prose.
Issue creation is a separate authorized `gh issue create` operation; the checker never opens an issue automatically.

For personal non-fork repositories, pass `--issue NUMBER` to create, edit, ready, and live checks, and put `Closes #NUMBER.` or an equivalent GitHub closing reference in the PR body.
Full issue URLs and Markdown links are accepted only for the selected issue in the exact destination.
Comments, code examples, quoted instructions, PR numbers, closed issues, and another repository's issue do not satisfy the prerequisite.
An external repository needs no personal issue prerequisite; if `--issue` is supplied, that selected open issue and closing reference are still checked.

## Authentication and API budget

Online operations first read the authenticated `user` endpoint through the host's existing `gh` session.
Successful access to a public repository alone does not establish authentication.
Every REST prerequisite response includes headers; missing or invalid quota evidence and a remaining count below the policy's nonzero reserve stop further work.
These reads run serially, authenticate once per command, and never add a separate quota poll or automatic retry.
Offline document checks use no API calls or authentication.

GitHub's [REST rate limits](https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api) distinguish authenticated usage from the smaller anonymous allowance and also impose secondary limits.
Use actual response headers and diagnostics rather than assuming an exact account-wide remaining count.
The reserve protects this command's REST preflight; `gh` may also use GraphQL, and other consumers or concurrent operations can exhaust either resource afterward.
The command does not promise immunity from rate limits, intercept other tools, or replay an uncertain mutation.
Stop on `403`, `429`, `Retry-After`, or exhausted quota, and follow the provider's indicated wait before a deliberate later attempt.

## Local authoring

Prepare a UTF-8 body file following the destination template or the default `Why`, `Changes`, and `Validation` structure.
The body must contain actual content outside HTML comments.
Known unfinished markers such as standalone or labelled `TODO`, `TBD`, and `FIXME`, named fields such as `{{description}}`, `[insert ...]`, and `<description>` are refused; fill them in before publishing.
The scanner identifies these markers mechanically and does not assess natural-language completeness.
Fenced and inline code examples, ordinary mentions of these words, collapsible HTML sections, and GitHub Actions expressions remain valid.

```console
pr-workflow check --repo OWNER/REPO --title "fix: preserve edits" --body-file body.md
pr-workflow create --repo OWNER/REPO --head feature --base main --title "fix: preserve edits" --body-file body.md
pr-workflow edit --repo OWNER/REPO --pr 17 --title "fix: preserve edits" --body-file body.md
pr-workflow ready --repo OWNER/REPO --pr 17
pr-workflow check --repo OWNER/REPO --pr 17 --final
```

Creation always uses draft in the default `local` mode, even when the document already passes validation.
There is no local flag to create a ready PR directly.
`ready` reads the current title, body, and state from GitHub and requires an open draft with an accepted document.
`edit` replaces title and body together; it does not change draft state.
Local-file `check` never contacts GitHub, and live `check --pr` is read-only.
An offline document check does not satisfy the live issue prerequisite.
Create and edit send an owned temporary copy of the checked body with `gh --body-file`, so a later change to the input file cannot alter the published bytes.

## CodeRabbit generation

Complete local checks before one budget-qualified CLI review of an initial substantive change.
An owner-imposed CodeRabbit pause stops generation, service reviews, and quota inquiries until explicitly resumed.
The CLI pause marker is machine-local and preserved across installation; `coderabbit --guard-status` remains a local read.
Run reviews through the installed guard on the Mac; the Windows and Linux hosts perform the heavy native checks.
The guard admits at most `floor(capacity * 0.9 * 0.8)` consumed reviews: 90% is the effective ceiling, and 20% of that ceiling remains unused.
Only the final integer budget is rounded down, so ten reviews yield seven usable reviews.
Its Advanced local ceiling is seven attempts per rolling hour and 168 per rolling day.
Usage queries have independent matching rolling limits and a one-minute minimum gap; these are local traffic limits, not a claimed provider API allowance.
Failures consume reservations, concurrent invocations refuse, and missing or inconsistent evidence refuses review.
The guard performs the numeric service preflight itself; do not call `usage` in a loop or query it immediately before each review.
An unavailable or exhausted CLI allowance is a skipped local review with its actual reason, not a clean review or permission to bypass the separate PR publication gate.

CLI, IDE, PR reviews, chat, and paid add-ons have separate usage boundaries.
Use [official PR quota guidance](https://docs.coderabbit.ai/management/rate-limits), including an authorized `@coderabbitai rate limit` comment which does not start a review.
Request at most one such status per hour, inspect the existing response with bounded reads, and never trigger a review to discover capacity.
Only a fresh numeric observation for the correct developer, organization, and pool can establish a reserve; a response saying reviews are available does not provide it.
The documented Review Usage dashboard contains observed historical rates and is not a reservation of current capacity.
The Advanced plan's 0–49-review activity band determines the PR refill rate, not a weekly PR allowance, and does not include the separate CLI pool.
Apply the same layered budget to a verified adaptive threshold as well as the hourly quota: a confirmed highest-band threshold of 49 gives a budget of 35 events per rolling 168 hours.
Confirm the actual applicable window and current counts; the official policy can use 24-hour or seven-day activity, so the seven-day calculation alone cannot guarantee an unchanged refill rate.
Hold ready publication, generation, and pushes that would start an automatic PR review when the required numeric capacity is unknown.
Report the reason and reconsideration time; after a one-hour hold, recheck once rather than assuming that the allowance has refilled or automatically pushing.
Keep paid usage inactive and honor an owner-imposed hold until explicitly resumed.
The CLI guard enforces its managed entry points, while these publication decisions remain an agent obligation; ordinary Git and GitHub clients are not intercepted by this PR command.
Other clients can spend service capacity after a snapshot, so account-wide guarantees require service-enforced limits or a common reservation gateway.

Use the standard placeholders documented in the [official configuration reference](https://docs.coderabbit.ai/reference/configuration).
Prepare the summary request as its own line and retain the required template fields and factual validation results.
The explicit mode permits only the exact title placeholder and standard standalone summary request, alongside otherwise completed content.

```console
pr-workflow check --repo OWNER/REPO --generation coderabbit --title "@coderabbitai" --body-file generation.md
pr-workflow create --repo OWNER/REPO --generation coderabbit --head feature --base main --title "@coderabbitai" --body-file generation.md
pr-workflow ready --repo OWNER/REPO --generation coderabbit --pr 17
pr-workflow check --repo OWNER/REPO --pr 17 --final
```

CodeRabbit creation normally opens the PR for review and generation; use `--draft` if the authorized task requires a draft.
For an existing draft containing the standard requests, explicit CodeRabbit `ready` permits the requests to initiate the normal review workflow.
It is a generation transition, not final validation.
The final check rejects remaining placeholders, including with `--generation coderabbit --final`.
Inspect the generated document for destination rules, intent, and truthful validation results, then use checked `edit` for authorized corrections.
Use [the configuration example](../assets/coderabbit.yaml) only when configuration work is authorized, merging it with the destination's existing rules.
Publishing the PR or installing this skill does not authorize changing organization or account settings.
The example places the summary in the description.
If the destination instead places it in the walkthrough comment, inspect that generated summary and transfer it into the body with checked `edit` before final validation.

## Destination title exception

When an upstream project requires another format, pass `--title-policy policy.json` to every relevant command.
The JSON is limited to the named repository and cites the rules that justify disabling the Conventional Commits check.
Nonempty single-line titles and unfinished-placeholder checks remain required.
The agent must check the upstream format itself; the exception does not mechanically enforce a replacement grammar.

```json
{
  "repository": "upstream/project",
  "source": "https://github.com/upstream/project/blob/main/CONTRIBUTING.md",
  "reason": "The contribution guide requires a sentence title without a type prefix."
}
```

## Evidence and effects

The shared transition core has source-bound Kani proofs for rejection, local draft creation, and ready eligibility.
Native process tests exercise argument boundaries, checked file copies, invalid documents, custom destination rules, live PR parsing, and failed `gh` operations.
The production issue-scope and publication decisions have Kani proofs, including refusal in both generation modes and rejection controls.
Native tests exercise real CLI/process parsing with isolated GitHub fixtures and verify that rejected prerequisites never invoke a PR mutation.
GitHub authorization, remote concurrency, service generation, and validation-claim truth remain external boundaries.
Live metadata is inspected immediately before a mutation; GitHub can still change an issue or repository afterward, so read back the result and do not claim a remote atomic transaction.
Avoid concurrent document edits during `ready`; `gh` and GitHub do not provide a compare-and-swap transition for the inspected title and body.
Read back the result and report any divergence or incomplete generation.
