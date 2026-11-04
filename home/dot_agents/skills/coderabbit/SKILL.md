---
name: coderabbit
description: >-
  Configure CodeRabbit and use its Plan, Triage, PR security reports, and Finishing Touches within the existing subscription.
  Use for CodeRabbit setup or these service workflows; use coderabbit-review for local reviews and applying PR findings.
metadata:
  tools: "tools.toml"
---

# CodeRabbit service workflow

Use `gh` for GitHub, and the signed-in CodeRabbit UI when no connector or CLI offers the operation.
Use `coderabbit-review` for local analysis and PR findings.
Leave implementation to the user's existing agent.
Read `coderabbit --guard-status --json` before a service operation and honor its pause scope, including for settings changes, until the owner resumes.

## Fixed-fee setup

Confirm the selected organization, active plan, assigned seat, and billing period in Plans & payments.
Keep Usage-based reviews, CodeRabbit Agent, and CodeRabbit Security Scan off.
Never start a trial, turn on paid usage, change the subscription, or add seats to get around review limits.
Check carryover settings after a trial or plan change, and keep automatic seat assignment off for a single-user account.
Keep the dotfiles CLI guard installed.
It enforces the local CLI budget and inactive paid usage, and `coderabbit --guard-status --json` reports its capacity and pause scope.
The guard leaves GitHub automatic reviews, IDE extensions, browser tasks, and unmanaged clients uncoordinated.
Never bypass the guard or reset its ledger when capacity runs out or verification fails.

Back up the current configuration before editing it, and check proposed YAML with the current official schema.
Put shared behavior in organization defaults, and add a small global override only for settings every repository needs.
A repository file or repository UI setting can replace organization defaults, and inheritance stays opt-in.
Keep repository-specific instructions near their source, and use automatic Code Guidelines discovery for `AGENTS.md`, `CLAUDE.md`, and related files.
Keep private knowledge out of public reviews, and never force global learning scope or unrestricted repository linking.
Keep custom checks advisory until the user authorizes changes to merge gates.
Existing deterministic CI stays authoritative.

## Planning and PR scope

Use CodeRabbit Plan for a large multi-part change when the user asks for planning or the scope needs clarity.
Give the desired behavior, constraints, affected repositories, compatibility expectations, and verification requirements.
Split the generated phases into useful changes with their own verification.
Copy the selected phase's Agent Handoff text into the existing implementation workflow.
Never select the Coding Agent handoff.
Never create issues or post `@coderabbitai plan` without authorization to publish the exact task.

Finish and check a coherent local change before a push or a ready-for-review PR.
Batch related commits into one push, and let the current review finish before another update.
Check review freshness after the final update, because automatic reviews can pause or hit rate limits.
Use the `pull-request` review policy.
Its default routes are no CodeRabbit for `low` or `medium`, guarded CLI for `high`, and PR review for `critical`.
Its owner-local repository policy selects PR review for every level, with no ignore line or critical opt-in label.
Keep severe classifications rare and the selected routes exclusive.
Use `coderabbit-review` for the selected current-diff evidence and existing supported findings.
Shared service settings opt in only `review:critical` and turn off automatic incremental review unless a repository requires PR review for every change.
For that repository, enable automatic reviews, include drafts, and remove positive label requirements in its effective service configuration.
The local policy selects and checks the route.
It doesn't configure the service or establish capacity.
Keep PR and CLI pools and their adaptive thresholds separate, and never substitute CLI capacity for PR capacity.
Inspect the effective configuration after adoption because inherited positive labels can remain active.
Read existing findings before requesting the selected review.

## Triage and PR security

Use Triage to inspect the user's open PRs across connected, unarchived repositories.
Save an explicit filter, because the personal default view can lose its filters.
Show CI state, unresolved feedback, current-head review state, and security risk.
Investigate missing, paused, stale, failed, and rate-limited reviews instead of reading every passing check as approval.
Use Architecture Review and Blast Radius as evidence for changed trust boundaries and affected callers.
A missing security section or a neutral check proves nothing about safety.
Keep merge protections and approval rules unless the task covers changing them.
Never turn on automatic merge, close, or cloud fix rules to organize a queue.

## Delivery

Use Finishing Touches only for a concrete PR task and delivery scope that the user authorized.
Read [delivery and billing](references/finishing-touches.md) before triggering a task.
An enabled feature grants no permission to post its command, open a PR, or commit to a branch.
Confirm that paid Agent usage stays off, and stop when the service asks for paid continuation.
Inspect generated changes and run the repository's relevant checks before calling the task complete.

Use [official references](references/docs.md) to check flags, limits, configuration keys, and billing.
