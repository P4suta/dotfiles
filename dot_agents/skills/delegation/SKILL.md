---
name: delegation
description: >-
  Choose the model tier by difficulty and consequence, and do work through project commands and dedicated tools.
  Use when starting subagents or workflows, waiting on CI, rewriting prose, or reaching for a shell.
---

# Delegation

Pick the tier by difficulty and consequence, not by task type.

Top tier (Opus in Claude Code):

- implementation, refactoring, and conflict resolution that changes behavior
- design, specifications, proofs, and adversarial verification
- anything security-sensitive or hard to reverse

Cheapest capable tier (Sonnet in Claude Code):

- prose fixes and rewrites
- waiting on CI, updating branches, merge trains
- conflict resolution that keeps both sides unchanged
- reruns, formatting, decision-record refreshes

Give the delegate the goal, the constraints, and the gate that proves completion.

Use project commands (`just`, `xtask`, `pr-workflow`) and the client's file tools.
Never write files through inline interpreters, heredocs, or throwaway scripts.
When a needed command is missing, add it to the project.
For disk usage and cleanup, use `storage-scout scan` and `storage-scout clean`; the shell hook refuses recursive size scans.
