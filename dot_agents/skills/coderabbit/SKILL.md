---
name: coderabbit
description: >-
  Configure CodeRabbit and use its Plan, Triage, PR security reports, and Finishing Touches within the existing subscription.
  Use for CodeRabbit setup or these service workflows; use coderabbit-review for local reviews and applying PR findings.
---

# CodeRabbit service workflow

Use `gh` for GitHub, and the signed-in CodeRabbit UI when no connector or CLI offers the operation.
Use `coderabbit-review` for local analysis and PR findings.
Leave implementation to the user's existing agent.
Honor an owner-imposed CodeRabbit pause for service calls, quota inquiries, review requests, and settings changes until the owner resumes it.

## Fixed-fee setup

Confirm the selected organization, active plan, assigned seat, and billing period in Plans & payments.
Keep Usage-based reviews, CodeRabbit Agent, and CodeRabbit Security Scan off.
Never start a trial, turn on paid usage, change the subscription, or add seats to get around review limits.
Check carryover settings after a trial or plan change, and keep automatic seat assignment off for a single-user account.
Keep the dotfiles CLI guard installed, and read `coderabbit --guard-status` for local capacity.
All local repositories and agents on the Mac share its rolling limits of seven per hour and 168 per day.
It admits at most `floor(capacity * 0.9 * 0.8)` consumed reviews and checks that paid billing stays off before analysis.
The other two native hosts run no managed service reviews or usage queries.
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
Use `coderabbit-review` to finish the current-head automatic review and supported findings.
Read the existing automatic review before any new local review.

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
