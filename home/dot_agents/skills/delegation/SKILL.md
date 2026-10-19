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

For an authorized multi-agent task, use `domyjob` to register the coordinator's interactive identity and open a room for that task.
Add every agent you start, pass its identity and the room name in its task, and have interactive delegates join under that identity.
Use the existing MCP chat tools or `domyjob chat --as NAME` with `room create`, `room add`, `send`, `inbox`, `reply`, and `thread`.
Read the room to judge progress, ask questions there, and keep output files for command diagnostics.
Started agents report status, blockers, completion, and shared-resource claims in the room and answer addressed questions there.
This task coordination grants no authority to start unrelated remote sessions or accept client folder trust.

Before sharing a lock, disk allocation, or memory-heavy step, send a claim with a unique `id`, `resource`, `owner` matching the sending agent, and absolute UTC `expires_at`.
The coordinator acknowledges the claim before work starts and uses room messages to serialize competing claims.
Renew an active claim before expiry, and explicitly release it with its `id` and `owner` when work ends.
After expiry, another agent may announce a new claim naming the expired claim it replaces and wait for the coordinator's acknowledgement.
An expired claim permits coordination takeover.
Verify that its old process has stopped before using its resources.
These messages communicate ownership and expiry.
The underlying tool's locks still enforce access.

Use project commands such as `just`, `xtask`, and `pr-workflow`, and the client's file tools.
Never write files through inline interpreters, heredocs, or throwaway scripts.
When a needed command lacks an implementation, add it to the project.
For disk usage and cleanup, use `storage-scout scan` and `storage-scout clean`.
The shell hook refuses recursive size scans.
