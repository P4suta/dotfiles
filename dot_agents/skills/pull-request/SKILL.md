---
name: pull-request
description: >-
  Prepare issue-scoped GitHub changes, create pull requests, update their documents, and move drafts to review through the checked local PR workflow.
  Use before GitHub-bound implementation and for PR authoring, and use coderabbit-review for review findings.
---

# Pull request authoring

Read the destination repository's instructions, contribution rules, and PR template, including inherited organization templates.
Those rules, including title format and required sections, override personal defaults.
Identify the exact repository, base, head, and authorized operation.
An authoring request grants no authority for unrelated publication or account changes.
Honor an owner-imposed CodeRabbit pause before generation, quota inquiries, or review requests, and keep its required review pending until explicit resumption.

Before implementation, run `pr-workflow start --repo OWNER/REPO`.
The [global workflow policy](assets/workflow-policy.json) identifies personal owners by GitHub login and numeric identity, and the command checks live repository identity and fork status.
For a personal non-fork repository, select an existing open issue or create one with `pr-workflow issue create` or `pr-workflow issue edit`, then run `start` with `--issue NUMBER`.
Record the problem, intended behavior, bounded scope, and observable acceptance criteria in the issue before coding.
Reuse a suitable issue instead of creating a duplicate.
External projects and forks follow their own contribution rules and need no upstream issue.
Treat an unknown or inconsistent repository response as a failed prerequisite.
Online operations need live authenticated `gh` access, keep requests bounded and serialized, and stop without retry on authentication failure, low quota, or rate limit.
Never transfer tokens between machines or publish from stale prerequisite caches.

Prefer CodeRabbit title and summary generation when the destination enables it and the service works.
After the local checks that mirror CI and before publishing an initial substantive PR, load `coderabbit-review` for one guarded CLI review when verified included allowance permits.
For an existing PR, use its current review and skip a duplicate CLI review of the same diff.
Admit at most `floor(capacity * 0.9 * 0.8)` consumed reviews for each verified allowance and applicable adaptive activity threshold, without intermediate rounding.
An availability message or elapsed cooldown establishes no numeric budget.
Hold any operation that would start a PR review while its numeric capacity or inactive paid usage stays unverified.
Report the missing evidence and the earliest permitted reconsideration time.
Never schedule automatic publication or poll the service.
For generation, use the standard `@coderabbitai` title and standalone `@coderabbitai summary` body placeholder with `--generation coderabbit`.
Read [commands and generation](references/workflow.md) for the local and CodeRabbit paths.
Write the document locally when the service fails or the destination requires a document it fails to produce.
Local creation always starts as a draft, and moving it to review needs separate authorization.
Run `ready` only when no scoped work remains.

Without destination-specific rules, write an English [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) title: `type(scope)!: description`.
`pr-workflow check` runs the prose checker on the title and body of a personal non-fork destination.
Use `Why`, `Changes`, and `Validation` when no template exists, and state the actual validation outcome.
Rewrite the description around the final change if the scope changed.

Use `pr-workflow start/check/create/edit/ready` instead of the corresponding `gh` mutation.
Personal non-fork publication requires `--issue NUMBER`, a live open issue in the exact destination, and a visible closing reference in the body.
Before publication, compare the diff with the issue's scope and acceptance criteria, and split independent changes into separate issues and PRs.
An external project's custom title format requires a repository-scoped title policy citing its rules.
Confirm validation claims from real command results, and distinguish passing, failing, unavailable, and unrun checks.
The checker proves neither the claims nor that the issue preceded implementation, so verify those from the work.

Read back the published document and draft state with `gh`, and run the final live-document check.
For CodeRabbit generation, observe the automatic review with bounded read-only polling and inspect the generated title and summary.
Pending, skipped, paused, stale, failed, or rate-limited generation leaves the PR incomplete, and a published placeholder never counts as finished.
Load `coderabbit-review` for the current-head review and `ci-budget` before an authorized push or PR update.
When `gh` fails, inspect the remote state before retrying an uncertain mutation.
