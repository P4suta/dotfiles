---
name: delegation
description: >-
  Choose the model tier by difficulty and consequence, and do work through project commands and dedicated tools.
  Use when starting subagents or workflows, waiting on CI, rewriting prose, or reaching for a shell.
---

# Delegation

Pick the tier by difficulty and consequence, not by task type.

The top tier, Opus in Claude Code, takes:

- implementation, refactoring, and conflict resolution that changes behavior
- design, specifications, proofs, and adversarial verification
- anything security-sensitive or hard to reverse

The cheapest capable tier, Sonnet in Claude Code, takes:

- prose fixes and rewrites
- waiting on CI, updating branches, merge trains
- conflict resolution that keeps both sides unchanged
- reruns, formatting, decision-record refreshes

Give the delegate the goal, the constraints, and the gate that proves completion.

Use project commands such as `just`, `xtask`, and `pr-workflow`, and the client's file tools.
Never write files through inline interpreters, heredocs, or throwaway scripts.
When a needed command lacks an implementation, add it to the project.
For disk usage and cleanup, use `storage-scout scan` and `storage-scout clean`.
The shell hook refuses recursive size scans.
