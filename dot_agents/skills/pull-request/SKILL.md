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

Before implementation, run `pr-workflow start --repo OWNER/REPO` to inspect the destination's scope.
The [global workflow policy](assets/workflow-policy.json) identifies personal owners by GitHub login and numeric identity, and the command checks live repository identity and fork status.
For a personal non-fork repository, select an existing open issue or create one within the authorized GitHub workflow, then run `start` with `--issue NUMBER`.
Read the issue and record the problem, intended behavior, bounded scope, and observable acceptance criteria before coding.
Reuse a suitable issue instead of creating duplicates, and avoid adding unrelated work because an issue already exists.
External projects and forks follow their own contribution rules; personal defaults do not require opening an upstream issue.
An unknown or inconsistent repository response is a failed prerequisite, not permission to apply a different scope.
Online operations require live authenticated `gh` access and inspect REST quota headers with a nonzero reserve.
Keep requests bounded and serialized; authentication failure, low quota, and rate-limit responses stop the operation without an automatic retry.
Use the host's normal `gh auth login` when needed; do not transfer tokens between machines or use stale prerequisite caches for publication.

`coderabbit --guard-status --json` reports whether CodeRabbit PR and CLI reviews are paused.
While PR reviews are paused, `pr-workflow` refuses an operation that would request one and reports that no review is awaited; follow its stated next action.
Otherwise prefer CodeRabbit title and summary generation where it is enabled, and load `coderabbit-review` for one guarded CLI review before an initial substantive PR.
Read [commands and generation](references/workflow.md) for the placeholders, the PR review budget, and the optional configuration example.
Use local authoring when the service is unavailable or the destination requires a document it cannot produce reliably.
Local creation always starts as a draft; moving it to review is a separate authorized operation.
`create` refuses a new PR while its author holds the repository's work-in-progress limit of open PRs, which the workflow policy records; add the work to the PR it names.
`ready` refuses until every head check passes; run it only when no scoped work remains.
Open dependent work with `pr-workflow stack create`; `create` refuses a branch sharing commits or paths with another open PR.
Restack with `pr-workflow stack sync` or `stack rebase`, never a manual rebase or force push, and merge with `pr-workflow merge` or `stack merge` ([stacks and merging](references/workflow.md#stacks-and-merging)).

Without destination-specific rules, write an English [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) title: `type(scope)!: description`.
Scope is optional; include `!` for breaking changes.
Prefer a concise description of the final behavior; length and stylistic preferences are recommendations.
Explain why the change is needed, what users or maintainers can now observe, and the actual validation outcome.
Use `Why`, `Changes`, and `Validation` when no template exists; add migration or other sections only when relevant.
Write each prose sentence on its own source line and rewrite the description around the final change if the scope changed.

Use `pr-workflow start/check/create/edit/ready` for preparation, document validation, and publication rather than directly invoking the corresponding `gh` mutation.
The command enforces title syntax, meaningful body content, known unfinished placeholders, and draft transitions without prescribing body headings.
Personal non-fork publication requires `--issue NUMBER`, a live open issue in the exact destination, and a visible closing reference in the proposed or current body.
Create, edit, ready, and the live final check recheck this prerequisite; CodeRabbit generation does not exempt it.
Compare the actual diff with the issue's scope and acceptance criteria before publication, splitting independent changes into separate issues and PRs.
An external project's custom title format requires a repository-scoped title policy citing its rules.
The dedicated entry point is the initial enforcement boundary; ordinary `gh` is not intercepted.
Confirm validation claims from real command results and distinguish passing, failing, unavailable, and unrun checks.
The checker cannot establish their truth or prove that a description is useful.
It also cannot prove that the issue preceded implementation or that the diff meets its scope; the skill must verify those facts from the work and execution evidence.

Read back the published document and draft state with `gh`, and run the final live-document check.
Its CodeRabbit line states whether a current-head PR review is still owed; when it is, load `coderabbit-review` for that review and its findings.
Load `ci-budget` before an authorized push or PR update.
When `gh` fails, retain its diagnostic and inspect the remote state before retrying an uncertain mutation.
