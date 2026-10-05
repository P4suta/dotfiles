---
name: pull-request
description: >-
  Prepare issue-scoped GitHub changes, create pull requests, update their documents, and move drafts to review through the checked local PR workflow.
  Use before GitHub-bound implementation and for PR authoring; use coderabbit-review for review findings.
---

# Pull Request Authoring

Read the destination repository's instructions, contribution rules, and PR template before preparing a document.
Include applicable organization templates when the repository inherits them.
Those rules take precedence over personal defaults, including title format and required sections.
Identify the exact repository, base, head, and authorized operation; preserve existing commit, push, comment, and review-request boundaries.
An authoring request does not authorize unrelated publication or account changes.
Honor an owner-imposed CodeRabbit pause before generation, quota inquiries, or review requests; keep its required review pending until explicit resumption.

Before implementation, run `pr-workflow start --repo OWNER/REPO` to inspect the destination's scope.
The [global workflow policy](assets/workflow-policy.json) identifies personal owners by GitHub login and numeric identity, and the command checks live repository identity and fork status.
For a personal non-fork repository, select an existing open issue or create one within the authorized GitHub workflow, then run `start` with `--issue NUMBER`.
Read the issue and record the problem, intended behavior, bounded scope, and observable acceptance criteria before coding.
Reuse a suitable issue instead of creating duplicates, and avoid adding unrelated work because an issue already exists.
Write an issue with `pr-workflow issue create` or `pr-workflow issue edit`, which check its document before `gh` publishes it.
External projects and forks follow their own contribution rules; personal defaults do not require opening an upstream issue.
An unknown or inconsistent repository response is a failed prerequisite, not permission to apply a different scope.
Online operations require live authenticated `gh` access and inspect REST quota headers with a nonzero reserve.
Keep requests bounded and serialized; authentication failure, low quota, and rate-limit responses stop the operation without an automatic retry.
Use the host's normal `gh auth login` when needed; do not transfer tokens between machines or use stale prerequisite caches for publication.

Prefer CodeRabbit title and summary generation when it is enabled and confirmed operational for the destination repository.
After CI-equivalent local checks and before publishing an initial substantive PR, load `coderabbit-review` for one guarded CLI review when verified included allowance permits.
Address supported findings and rerun affected local checks before a coherent publication; use an existing PR's current review first and skip duplicate CLI analysis of the same unchanged diff.
Check the intended publication's separate service pools through their official supported usage methods.
Admit at most `floor(capacity * 0.9 * 0.8)` consumed reviews for each verified allowance and applicable adaptive activity threshold, retaining 20% of a 90% ceiling without intermediate rounding.
An availability message or elapsed cooldown does not establish this numeric budget.
Hold an operation that would start a PR review when its numeric capacity or inactive paid usage cannot be established.
Report the specific missing evidence and earliest permitted reconsideration time without scheduling automatic publication or repeatedly querying the service.
Use its standard `@coderabbitai` title and standalone `@coderabbitai summary` body placeholder with `--generation coderabbit`.
Keep required template content and actual validation results around the summary request.
Read [commands and generation](references/workflow.md) for the local and CodeRabbit paths and the optional configuration example.
Use local authoring when the service is unavailable or the destination requires a document it cannot produce reliably.
Local creation always starts as a draft; moving it to review is a separate authorized operation.
`ready` refuses until every head check passes; run it only when no scoped work remains.

Without destination-specific rules, write an English [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) title: `type(scope)!: description`.
Scope is optional; include `!` for breaking changes.
For a personal non-fork destination, `pr-workflow check` runs the prose checker on the title and body.
Rewrite every sentence it reports.
Explain why the change is needed, what users or maintainers can now observe, and the actual validation outcome.
Use `Why`, `Changes`, and `Validation` when no template exists; add migration or other sections only when relevant.
Rewrite the description around the final change if the scope changed.

Use `pr-workflow start/check/create/edit/ready` for preparation, document validation, and publication rather than directly invoking the corresponding `gh` mutation.
The command enforces title syntax, meaningful body content, known unfinished placeholders, the writing standard, and draft transitions without prescribing body headings.
Personal non-fork publication requires `--issue NUMBER`, a live open issue in the exact destination, and a visible closing reference in the proposed or current body.
Create, edit, ready, and the live final check recheck this prerequisite; CodeRabbit generation does not exempt it.
Compare the actual diff with the issue's scope and acceptance criteria before publication, splitting independent changes into separate issues and PRs.
An external project's custom title format requires a repository-scoped title policy citing its rules.
The dedicated entry point is the initial enforcement boundary; ordinary `gh` is not intercepted.
Confirm validation claims from real command results and distinguish passing, failing, unavailable, and unrun checks.
The checker cannot establish their truth or prove that a description is useful.
It also cannot prove that the issue preceded implementation or that the diff meets its scope; the skill must verify those facts from the work and execution evidence.

Read back the published document and draft state with `gh`, and run the final live-document check.
For CodeRabbit generation, observe the existing automatic review with bounded read-only polling and inspect the generated title and summary before completing the PR task.
Pending, skipped, paused, stale, failed, or rate-limited generation remains incomplete; do not treat publishing a placeholder as a finished PR.
Load `coderabbit-review` for the current-head review and supported findings, and `ci-budget` before an authorized push or PR update.
When `gh` fails, retain its diagnostic and inspect the remote state before retrying an uncertain mutation.
