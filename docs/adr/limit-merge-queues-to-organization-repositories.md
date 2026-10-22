---
status: accepted
date: 2026-10-23
---
# Limit merge queues to organization-owned repositories

## Context

This record partly supersedes the queue baseline in [stack-first pull requests through a merge queue](stack-first-pull-requests-through-a-merge-queue.md).
The stack workflow and its other decisions remain in force.

GitHub [offers merge queues](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue) for public organization-owned repositories and private organization-owned repositories on GitHub Enterprise Cloud.
Repositories owned by personal accounts cannot use them.
Adding a `merge_queue` rule to P4suta/aozora returned HTTP 422 with `Invalid rule 'merge_queue'`.
The personal baseline therefore cannot require a queue.

## Decision

Merge queues apply only where GitHub offers them.
`repo-settings audit` records the repository owner's type as `User` or `Organization`.
An older snapshot without that field defaults to `User`, so it never authorizes a queue update without ownership evidence.
Audit rejects missing or unsupported owner types in live API responses.

`repo-settings plan` proposes a queue only for an organization-owned repository with required checks whose reporting workflows support `merge_group`.
The queue update drops strict checks and preserves the rest of the existing ruleset.
For a user-owned repository, the plan leaves the ruleset and its strict check policy unchanged and reports no queue findings.

Personal repositories keep strict required checks and rely on stacked pull requests, which [merge atomically](https://docs.github.com/en/pull-requests/reference/stacked-pull-requests#merge-methods).
The baseline in `policy/github.toml` follows that rule for P4suta/dotfiles.

## Consequences

Personal repositories no longer receive queue updates that GitHub rejects.
A stack still lands all or nothing, and strict checks can require a rebase when its base advances.
Organization-owned repositories retain the queue readiness checks and the existing squash queue configuration.
An old snapshot of an organization-owned repository needs a fresh audit before the plan can add a queue.

## Alternatives

Requiring queues for personal repositories keeps proposing an unsupported API request.
Dropping strict checks without a queue removes the check on the current base.
Moving repositories to an organization changes their ownership to satisfy a workflow preference.
