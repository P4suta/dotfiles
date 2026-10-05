---
name: delegation
description: >-
  Route routine supervision to the cheapest capable model and do work through project commands and dedicated tools.
  Use when starting subagents or workflows, waiting on CI, rewriting prose, or reaching for a shell.
---

# Delegation

Delegate to the cheapest capable model tier (Sonnet in Claude Code):

- prose fixes and rewrites
- waiting on CI, updating branches, merge trains
- conflict resolution that keeps both sides
- reruns, formatting, decision-record refreshes

Keep design, judgment, and adversarial verification on the top tier.
Give the delegate the goal, the constraints, and the gate that proves completion.

Use project commands (`just`, `xtask`, `pr-workflow`) and the client's file tools.
Never write files through inline interpreters, heredocs, or throwaway scripts.
When a needed command is missing, add it to the project.
