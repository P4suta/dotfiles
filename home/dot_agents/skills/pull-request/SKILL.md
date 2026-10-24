---
name: pull-request
description: >-
  Prepare issue-scoped GitHub changes, create pull requests, update their documents, and move drafts to review through the checked local PR workflow.
  Use before GitHub-bound implementation and for PR authoring, and use coderabbit-review for review findings.
metadata:
  tools: "tools.toml"
---

# Pull request authoring

Read the destination repository's instructions, contribution rules, and PR template, including inherited organization templates.
Those rules, including title format and required sections, override personal defaults.
Identify the exact repository, base, head, and authorized operation.
An authoring request grants no authority for unrelated publication or account changes.

Before GitHub-bound implementation, run `pr-workflow start --repo OWNER/REPO`.
The [global workflow policy](assets/workflow-policy.json) identifies personal owners by GitHub login and numeric identity, and the command checks live repository identity and fork status.
For a personal non-fork repository, select an existing open issue or create one with `pr-workflow issue create` or `pr-workflow issue edit`, then run `start` with `--issue NUMBER --path PATH`.
Record the problem, intended behavior, bounded scope, and observable acceptance criteria in the issue before coding.
Reuse a suitable issue instead of creating a duplicate.
External projects and forks follow their own contribution rules and need no upstream issue.
Treat an unknown or inconsistent repository response as a failed prerequisite.
Online operations need live authenticated `gh` access, keep requests bounded and serialized, and stop without retry on authentication failure, low quota, or rate limit.
Never transfer tokens between machines or publish from stale prerequisite caches.

Before a remote repository exists, use the explicit local provider.
Run `pr-workflow local init`, create an issue with `local create`, and register its implementation scope with `local select --issue NUMBER --path PATH` before editing.
These commands need no GitHub executable, token, or authentication.
Run `local check` for the issue prerequisite and the project's normal commands for its required verification.
Local issue numbers and owner-local records grant no publication authority.
When publication becomes authorized, link the actual local history with checked remote `start --promote-local` and keep the normal live issue, signing, checks, and owner authorization.
The new remote start supplies no retroactive authority for earlier local edits.

Assess every PR using the [importance criteria](references/importance.md) before publication.
The checked workflow requires an authenticated current-diff assessment and routes `low` and `medium` to no CodeRabbit review, `high` to the guarded CLI, and `critical` to PR review.
The workflow keeps its assessments, pause records, and issue contexts in owner-local state, and published documents and tracked files never carry them.
Keep severe levels exceptional and explain concrete consequences rather than escalating by filename or diff size.
`coderabbit --guard-status --json` reports the separate PR and CLI pause scopes.
A paused required route remains pending and never changes its classification.
Use `coderabbit-review` for the selected route and existing supported findings.
Keep the standalone PR description exclusion for all noncritical levels, and permit CodeRabbit generation only for the PR route.
Read [commands and generation](references/workflow.md) for the placeholders, the PR review budget, and the optional configuration example.
Write the document locally when the service fails or the destination requires a document it fails to produce.
Local creation always starts as a draft, and moving it to review needs separate authorization.
The workflow policy caps work units at three per person and repository by default.
One open stack counts as one unit, and each declared independent PR counts as one unit.
Normal `start` and `create` use the person's open stack tip, and `create` links the new member into that stack.
Existing members can receive fixes in place.
Declare unrelated work with `start --independent <reason>` or `create --independent <reason>`.
The reason stays in owner-local state and never enters the PR body.
A declaration from `start` carries into `create` for the same source, branch, issue, and descendant history.
At the cap, work can join the existing stack but can't add another unit.
`ready` refuses until every head check passes, so run it only when no scoped work remains.
Use `pr-workflow stack create` to name the open stack tip explicitly.
The dependency check skips PRs opened by bots identified by an `app/` login prefix, a `[bot]` login suffix, or the `Bot` account type.
It also skips stale PRs that already conflict with their own base and reports every skipped PR with its reason.
For every PR outside the selected parent chain, `create` refuses shared commits and merges overlapping heads in memory using their merge base.
A clean merge permits creation.
A dependency names the PR to stack on, or requires resolution before extending an existing stack.
Restack with `pr-workflow stack sync` or `stack rebase`.
Never use a manual rebase or force push.
Merge with `pr-workflow merge` or `stack merge`.
The section [stacks and merging](references/workflow.md#stacks-and-merging) describes both.

Use `pr-workflow link-issue --repo OWNER/REPO --pr NUMBER --issue NUMBER` to add a closing reference to an existing PR as the authenticated numeric repository owner.
The operation checks the open issue, PR source network, head, base, and current document.
It preserves existing assessment markers and review metadata while appending one reference or leaving a matching reference unchanged.
The normal assessment and review gates continue to govern source acceptance.
Inspect an uncertain write before another attempt.
The command checks state before and after its write and assumes no unobserved concurrent edit during that interval.

Without destination-specific rules, write an English [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) title: `type(scope)!: description`.
`pr-workflow check` runs the prose checker on the title and body of a personal non-fork destination.
Write one sentence per line in commit messages, issue and PR bodies, comments, documents, and replies in English and Japanese.
Never wrap a sentence across lines or put two sentences on one line.
Split long sentences into shorter ones when the destination limits line length.
Headings, labels ending in a colon, lists, tables, code, quotes, and front matter keep their structural line rules.
The shell guard checks literal GitHub body, body-file, close and reopen comment, and nested JSON body inputs through the publication prose channel.
An unreadable file, unresolved expansion, ambiguous standard input, or unsupported publishing command supplies no completed check and blocks the command.
Use a readable UTF-8 body file with a direct command in its known execution directory, and keep issue and PR documents on the checked workflow route.
Use `Why`, `Changes`, and `Validation` when no template exists, and state the actual validation outcome.
Rewrite the description around the final change if the scope changed.
Write every PR, issue, review, and comment in the owner's voice: flat, short, and about the change or the decision.
Leave out who or what wrote it, the local workflow, and notes addressed to the owner.

Use `pr-workflow start/check/create/edit/ready` instead of the corresponding `gh` mutation.
Personal non-fork publication requires `--issue NUMBER`, a live open issue in the exact destination, and a visible closing reference in the body.
Before publication, compare the diff with the issue's scope and acceptance criteria, and split independent changes into separate issues and PRs.
An external project's custom title format requires a repository-scoped title policy citing its rules.
Confirm validation claims from real command results, and distinguish passing, failing, unavailable, and unrun checks.
The checker proves neither the claims nor that the issue preceded implementation, so verify those from the work.

Read back the published document and draft state with `gh`, and run the final live-document check.
Its CodeRabbit line reports the selected route and outstanding evidence for the current diff.
Load `ci-budget` before an authorized push or PR update.
When `gh` fails, keep its diagnostic and inspect the remote state before retrying an uncertain mutation.
