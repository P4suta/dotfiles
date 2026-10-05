---
name: coderabbit
description: >-
  Configure CodeRabbit and use its Plan, Triage, PR security reports, and Finishing Touches within the existing subscription.
  Use for CodeRabbit setup or these service workflows; use coderabbit-review for local reviews and applying PR findings.
---

# Review service workflow

Read the task, the repository instructions, and the existing authorization before operating the service.
Use `gh` for GitHub, and the signed-in CodeRabbit web interface when no connector or command-line tool offers the operation.
Use `coderabbit-review` for local analysis and pull request findings.
Leave implementation to the user's existing coding assistant.
Honor an owner-imposed CodeRabbit pause for service calls, quota inquiries, review requests, and settings changes until the owner resumes it.

## Fixed-fee setup

Confirm the selected organization, active plan, assigned seat, and billing period in `Plans & payments`.
Keep Usage-based reviews, `CodeRabbit Agent`, and CodeRabbit Security Scan off under the fixed-fee policy.
Never start a trial, turn on paid usage, change the subscription, or add seats to get around review limits.
Check carryover settings after a trial or plan change, and keep automatic seat assignment off for a single-user account.
Use the included command-line allowance, and note that local implementation spends the coding assistant's own allowance.
Keep the dotfiles command-line guard installed, and read `coderabbit --guard-status` for local capacity.
All local repositories and assistants on the Mac share its rolling limits of seven per hour and 168 per day.
It admits at most `floor(capacity * 0.9 * 0.8)` consumed reviews, a 90% ceiling with a further 20% margin, and checks that paid billing stays off before analysis.
The other two native hosts run no managed service reviews or usage queries, and quota probes have their own persistent frequency limit.
The guard leaves GitHub automatic reviews, editor extensions, browser tasks, and unmanaged clients uncoordinated.
Never bypass the guard or reset its ledger when capacity runs out or verification fails.

Back up the current configuration before editing it, and check proposed YAML with the current official schema.
Put shared behavior in organization defaults, and add a small global override only for settings every repository needs.
A repository file or repository interface setting can replace organization defaults, and inheritance stays opt-in.
Keep repository-specific instructions near their source, and use automatic Code Guidelines discovery for `AGENTS.md`, `CLAUDE.md`, and related files.
Keep private knowledge out of public reviews, and never force global learning scope or unrestricted repository linking.
Keep custom checks advisory until the user authorizes changes to merge gates.
Custom checks inspect evidence but run no test suite and prove no reviewer approval.
Existing deterministic CI stays authoritative.

## Planning and pull request scope

Use CodeRabbit Plan for a large multi-part change when the user asks for planning or the scope needs clarity.
Give the desired behavior, constraints, affected repositories, compatibility expectations, and verification requirements.
Inspect the generated phases and dependencies, then split the work into useful changes with their own verification.
Copy the selected phase's `Agent Handoff` text into the existing implementation workflow.
Never select the `Coding Agent` handoff under the fixed-fee policy.
Never create issues or post `@coderabbitai plan` without authorization to publish the exact task.

Finish and check a coherent local change before a push or a ready-for-review pull request.
Use draft pull requests for active work when the user authorizes a draft.
Batch related commits into one push, and let the current review finish before another update.
Automatic reviews can pause or hit rate limits, so check review freshness after the final update.
Use `coderabbit-review` to finish the current-head automatic review and supported findings within an authorized pull request task.
An open pull request or passing CI finishes nothing, and the existing automatic review comes before any new local review.

## Triage and pull request security

Use Triage to inspect the user's open pull requests across connected, unarchived repositories the user can open.
Save an explicit filter on the pull request creator, because the personal `Requires my action` view can lose its filters.
Show CI state, unresolved feedback, current-head review state, and security risk.
Investigate missing, paused, stale, failed, and rate-limited reviews instead of reading every passing check as approval.
Use `Architecture Review` and `Blast Radius` as evidence for changed trust boundaries and affected callers.
A missing security section or a neutral check proves nothing about safety.
Publishing the CodeRabbit Security check differs from requiring it in GitHub.
Keep merge protections and approval rules unless the task covers changing them.
Never turn on automatic merge, close, or cloud fix rules to organize a queue.

## Finishing touches

Use Finishing Touches only for a concrete pull request task and delivery scope that the user authorized.
Read [delivery and billing](references/finishing-touches.md) before triggering a task.
An enabled feature grants no permission to post its command, open a pull request, or commit to a branch.
Initial eligible turns can cost nothing, and steering, revision, and follow-up turns can incur per-minute charges.
Confirm that paid `Agent` usage stays off, and stop when the service asks for paid continuation.
Inspect generated changes and run the repository's relevant checks before calling the task complete.

Use [official references](references/docs.md) to check flags, limits, configuration keys, and billing instead of recording prices here.
