---
name: pull-request
description: >-
  Prepare issue-scoped GitHub changes, create pull requests, update their documents, and move drafts to review through the checked local PR workflow.
  Use before GitHub-bound implementation and for PR authoring, and use coderabbit-review for review findings.
---

# Pull request authoring

Read the destination repository's instructions, contribution rules, and PR template, including inherited organization templates.
Those rules, including title format and required sections, override personal defaults.
Identify the exact repository, base, head, and authorized operation, and preserve existing commit, push, comment, and review-request boundaries.
An authoring request grants no authority for unrelated publication or account changes.
Honor an owner-imposed CodeRabbit pause before generation, quota inquiries, or review requests, and keep its required review pending until explicit resumption.

Before implementation, run `pr-workflow start --repo OWNER/REPO` to inspect the destination's scope.
The [global workflow policy](assets/workflow-policy.json) identifies personal owners by GitHub login and numeric identity, and the command checks live repository identity and fork status.
For a personal non-fork repository, select an existing open issue or create one, then run `start` with `--issue NUMBER`.
Read the issue and record the problem, intended behavior, bounded scope, and observable acceptance criteria before coding.
Reuse a suitable issue instead of creating a duplicate, and keep unrelated work out of an existing issue.
Write an issue with `pr-workflow issue create` or `pr-workflow issue edit`, which check its document before `gh` publishes it.
External projects and forks follow their own contribution rules, and personal defaults require no upstream issue.
Treat an unknown or inconsistent repository response as a failed prerequisite, never as permission for a different scope.
Online operations require live authenticated `gh` access and inspect API quota headers with a nonzero reserve.
Keep requests bounded and serialized.
Authentication failure, low quota, and rate-limit responses stop the operation without an automatic retry.
Use the host's normal `gh auth login` when needed.
Never transfer tokens between machines or publish from stale prerequisite caches.

Prefer CodeRabbit title and summary generation when the destination enables it and the service works.
After the local checks that mirror CI and before publishing an initial substantive PR, load `coderabbit-review` for one guarded command-line review when verified included allowance permits.
Address supported findings and rerun affected local checks before publishing.
For an existing PR, use its current review and skip a duplicate command-line review of the same diff.
Check each separate service pool through its official usage method.
Admit at most `floor(capacity * 0.9 * 0.8)` consumed reviews for each verified allowance and applicable adaptive activity threshold, without intermediate rounding.
An availability message or elapsed cooldown establishes no numeric budget.
Hold any operation that would start a PR review while its numeric capacity or inactive paid usage stays unverified.
Report the missing evidence and the earliest permitted reconsideration time.
Never schedule automatic publication or poll the service.
For generation, use the standard `@coderabbitai` title and standalone `@coderabbitai summary` body placeholder with `--generation coderabbit`.
Keep required template content and actual validation results around the summary request.
Read [commands and generation](references/workflow.md) for the local and CodeRabbit paths and the optional configuration example.
Write the document locally when the service fails or the destination requires a document it fails to produce.
Local creation always starts as a draft, and moving it to review needs separate authorization.
`ready` refuses until every head check passes.
Run it only when no scoped work remains.

Without destination-specific rules, write an English [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) title: `type(scope)!: description`.
Scope stays optional.
Add `!` for breaking changes.
For a personal non-fork destination, `pr-workflow check` runs the prose checker on the title and body.
Rewrite every sentence it reports.
Explain why the change matters, what users or maintainers now observe, and the actual validation outcome.
Use `Why`, `Changes`, and `Validation` when no template exists, and add migration or other sections only when relevant.
Rewrite the description around the final change if the scope changed.

Use `pr-workflow start/check/create/edit/ready` for preparation, validation, and publication instead of the corresponding `gh` mutation.
The command enforces title syntax, meaningful body content, known unfinished placeholders, the writing standard, and draft transitions without prescribing body headings.
Personal non-fork publication requires `--issue NUMBER`, a live open issue in the exact destination, and a visible closing reference in the proposed or current body.
Create, edit, ready, and the live final check recheck this prerequisite, and CodeRabbit generation grants no exemption.
Before publication, compare the actual diff with the issue's scope and acceptance criteria, and split independent changes into separate issues and PRs.
An external project's custom title format requires a repository-scoped title policy citing its rules.
Only the dedicated entry point enforces these rules, and ordinary `gh` runs unchecked.
Confirm validation claims from real command results, and distinguish passing, failing, unavailable, and unrun checks.
The checker proves neither their truth nor the description's usefulness.
It also leaves unproven that the issue preceded implementation and that the diff fits its scope, so verify those facts from the work and execution evidence.

Read back the published document and draft state with `gh`, and run the final live-document check.
For CodeRabbit generation, observe the automatic review with bounded read-only polling and inspect the generated title and summary before completing the PR task.
Pending, skipped, paused, stale, failed, or rate-limited generation leaves the PR incomplete, and a published placeholder never counts as finished.
Load `coderabbit-review` for the current-head review and supported findings, and `ci-budget` before an authorized push or PR update.
When `gh` fails, keep its diagnostic and inspect the remote state before retrying an uncertain mutation.
