---
status: accepted
date: 2026-10-20
---
# Keep workflow evidence on the owner's machines

## Context

The PR workflow authenticated its decisions by publishing them.
`pr-workflow assess` wrote a signed statement that each PR body carried as an HTML comment, with the rationale, finding reasons, the committer's email address, and an SSH signature.
`merge` inserted a pause marker comment into the body, and `pr-workflow start` committed `.pr-workflow/contexts.json` with the issue, branch, scopes, and hashes of the owner's uncommitted files.
Local rules serve the owner and the agents on the owner's machines, and their records don't belong in public documents or tracked files.

## Decision

The signed statement lives in owner-local state at `~/.local/state/pr-workflow/assessments`, keyed by the repository, head, and diff digest, with an index from each PR to the binding last verified for it.
Every command that needs the statement loads it from there and keeps the same signature, binding, and source checks.
A PR document that carries an assessment line or a pause marker gets refused.

The pause exclusion lives in owner-local state at `~/.local/state/pr-workflow/pauses`.
The standing CodeRabbit ignore line stays in noncritical PR bodies, because the service reads it and no label-gated configuration covers every repository yet.

Issue contexts live only in owner-local state that every worktree of a repository shares, and the repository ignores `.pr-workflow/`.
A remote host that checks a pushed candidate skips the issue context, which the originating host checked before the push.

A machine without the local record reassesses the PR instead of reading evidence from GitHub.

## Consequences

Published documents carry only what a reader of the project needs.
Merged PR bodies lost their assessment and pause lines on 2026-10-20, and GitHub's edit history still holds the earlier text.
Another machine can't complete a PR from evidence that the first machine produced, so it reassesses.
