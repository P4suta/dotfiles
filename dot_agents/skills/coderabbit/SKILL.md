---
name: coderabbit
description: >-
  Configure CodeRabbit and use its Plan, Triage, PR security reports, and Finishing Touches within the existing subscription.
  Use for CodeRabbit setup or these service workflows; use coderabbit-review for local reviews and applying PR findings.
---

# CodeRabbit Service Workflow

Read the current task, repository instructions, and existing authorization before operating the service.
Use `gh` for GitHub and the authenticated CodeRabbit UI when no supported connector or CLI provides the operation.
Use the neighboring `coderabbit-review` skill for local analysis and PR findings.
Keep ordinary implementation with the user's existing coding agent.
Honor an owner-imposed CodeRabbit pause for service calls, quota inquiries, review requests, and settings changes until explicitly resumed.

## Fixed-fee setup

Confirm the selected organization, active plan, assigned seat, and actual billing period in Plans & payments.
Keep Usage-based reviews, CodeRabbit Agent, and CodeRabbit Security Scan inactive under the user's fixed-fee policy.
Do not start a new agent trial, activate paid usage, change the subscription, or add seats as a workaround for review limits.
Check carryover settings after a trial or plan change and keep automatic seat assignment off for a single-user account.
Use the included CLI allowance; local implementation still uses the coding agent's normal allowance.
Keep the dotfiles CLI guard installed and inspect `coderabbit --guard-status` when checking local capacity.
Its seven-per-hour and 168-per-day rolling limits are shared by all local repositories and agents on the Mac.
It admits at most `floor(capacity * 0.9 * 0.8)` consumed reviews, using a 90% ceiling with a further 20% margin, and verifies inactive paid billing before analysis.
Managed service reviews and usage queries are disabled on the other two native hosts, and quota probes have their own persistent frequency limit.
It does not coordinate GitHub automatic reviews, IDE extensions, browser tasks, or unmanaged clients.
Do not bypass the guard or reset its ledger when capacity is exhausted or verification fails.

Back up the current configuration before editing it and validate proposed YAML against the current official schema.
Prefer organization defaults for shared behavior and a small global override only for settings deliberately required across repositories.
A repository file or repository UI configuration can supersede organization defaults; inheritance is opt-in.
Keep repo-specific instructions close to their source and use automatic Code Guidelines discovery for `AGENTS.md`, `CLAUDE.md`, and related files.
Keep private knowledge isolated from public reviews; do not force global learning scope or unrestricted repository linking.
Keep custom checks advisory until a user authorizes changes to merge gates.
Custom checks inspect evidence but cannot run the project's test suite or prove reviewer approval.
Keep existing deterministic CI authoritative.

## Plan and PR scope

Use CodeRabbit Plan for a substantial, multi-part change when planning is requested or would resolve unclear scope.
Provide the desired behavior, constraints, affected repositories, compatibility expectations, and verification requirements.
Inspect the generated phases and dependencies, then split work into independently useful changes with their own verification.
Copy the selected phase's Agent Handoff text to the existing implementation workflow.
Do not select cloud Coding Agent handoff under the fixed-fee policy.
Do not create issues or post `@coderabbitai plan` without authorization to publish the exact task.

Finish and validate a coherent local change before a push or ready-for-review PR.
Use draft PRs for active work when creating a draft is authorized.
Batch related commits into one meaningful push and let the current review finish before another update.
Automatic reviews can pause or be rate-limited; check actual review freshness after the final update.
Use `coderabbit-review` to finish the current-head automatic review and supported findings as part of an authorized PR task.
An open PR or passing CI alone does not complete that workflow, and existing automatic review should be read before spending another CLI review.

## Triage and PR security

Use Triage to inspect the user's open PRs across connected, accessible, unarchived repositories.
Save an explicit author filter rather than relying on the personal Requires my action view to persist its filters.
Show CI state, unresolved feedback, current-head review state, and available security risk.
Investigate missing, paused, stale, failed, and rate-limited reviews instead of interpreting every passing check as approval.
Use Architecture Review and Blast Radius as evidence for changed trust boundaries and affected callers.
A missing security section or neutral check does not prove that a change is safe.
Enabling publication of the CodeRabbit Security check is separate from requiring it in GitHub.
Preserve current merge protections and approval rules unless changing them is explicitly in scope.
Do not enable automatic merge, close, or cloud fix rules merely to organize a queue.

## Finishing Touches

Use Finishing Touches only for an authorized, concrete PR task and delivery scope.
Read [delivery and billing](references/finishing-touches.md) before triggering a task.
Enabling a feature is not permission to post its command, open a PR, or commit to a branch.
Initial eligible turns can be free; steering, revision, and follow-up turns can incur agent-minute charges.
Verify that paid Agent usage remains inactive and stop if the service requests paid continuation.
Inspect generated changes and run the repository's relevant checks before treating the task as complete.

Use [official references](references/docs.md) to verify changed flags, limits, configuration keys, and billing behavior instead of preserving stale prices in the workflow.
