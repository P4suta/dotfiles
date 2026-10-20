---
status: accepted
date: 2026-10-20
---
# Admit Git operations that lose no owner work

## Context

The Git guard refused 412 forced operations and saw 143 waivers in the three weeks to 2026-10-20.
Codex deletes its own `refs/codex/turn-diffs/*` refs, test fixtures and probes override signing and hooks in throwaway repositories, and squash-merged branches fail `git branch -d`.
None of these refusals protected the owner's work, and each one cost a round trip or a waiver.

## Decision

`git update-ref -d` passes for a full ref name outside `refs/heads/`, `refs/tags/`, `refs/notes/`, and `refs/stash`.
In a repository whose common Git directory lies under the temporary directory, the guard admits every operation it refuses elsewhere except a push, which leaves the repository.
`DOTGUARD_STRICT_TEMPORARY=1` guards such a repository like any other, which the guard's own tests use.
`pr-workflow prune-merged` deletes local branches whose pull request merged at their exact head, or whose changes the default branch already holds, under the stack lease that the guard admits for `git branch -D`.
`crates/rules/src/git_ref_rules.rs` decides the deletable refs and the throwaway admission, with Kani harnesses and rejecting probes.

## Consequences

Agents delete tool refs, run fixtures, and prune merged branches without a waiver.
A worktree under the temporary directory of an owner repository keeps every refusal, because its common Git directory lies elsewhere.
