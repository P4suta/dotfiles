---
name: ci-budget
description: >-
  Keep hosted CI lean with a fast gate on every change and heavy checks at the release gate, and verify coherent changes before an explicit push.
  Use when adding or changing GitHub Actions workflows, schedules, fuzzing, mutation, proofs, or OS matrices, before authorized pushes or PR updates, and when Actions allowance runs low.
---

# Lean CI and local verification before pushing

Design hosted workflows from [lean workflows](references/lean-workflows.md): a fast deterministic gate on every change, no schedules, and heavy checks at the release gate.
Run affected verification at the stage the project defines, following `development-assurance`.
Before release, require the full inventory on the exact candidate.
Affected evidence can't replace full-scope evidence.
Reuse current results instead of starting duplicate campaigns at commit, push, or PR transitions.
Use `formal-assurance` for reproducible verdicts and run heavy checks in the project's pinned container or documented hosted runners.
Required hosted checks and repository-required reviews remain merge gates.
Use automatic incremental CodeRabbit reviews and follow `pull-request` for an explicit merge instruction during unavailable optional review.
An explicit push pause overrides any push authorization until the user explicitly resumes pushes.

## Prepare locally

Read the diff, workflow triggers, required jobs, and project check entry points.
Map each affected CI job to its local command and its required OS, architecture, environment, or service.
Run the fast gate on the current host before a push, and leave other OS families to hosted CI and the release gate.
Before every push, answer whether every applicable local check required for that push passed, with a short reason naming the actual commands and results.
Run missing checks and fix failures before answering yes.
Repair missing tools or services, or use a justified equivalent check.
If a required local check still can't run, ask the user for the decision needed before pushing.
An unverified disclaimer grants no exemption.
The owner's hosts may cache artifacts and exact-source evidence within the owner-side tooling boundary in `formal-assurance`.
Keep hosted-only prerequisites at their proper workflow stage, such as GitHub event permissions, OIDC, environment approval, and publication.
Local verification grants no signing, publication, paid service calls, or self-hosted runner registration.

Reuse passing check results for the same source and environment until a change or failure invalidates them.
Don't require manually authored verification receipts or detailed evidence submissions.
Run cheap checks first, and use `resource-coordination` before an expensive campaign.
Use `reliability` for intermittent failures instead of pushing until CI passes.

## Control publication

Before every push, inspect the effective hooks and the machine-local pause in [push control](references/push-control.md).
Honor the marker even where a Git client lacks the hook.
Never remove a pause, bypass hooks, change CI requirements, or add skip directives to make a push succeed.
Only the owner's explicit instruction resumes a pause.
Before a costly hosted run with uncertain allowance, check the owner's [Actions billing](https://docs.github.com/en/billing/concepts/product-billing/github-actions).

Continue local work, verification, and authorized commits during a pause.
A local commit implies no push.
Fetch before publishing, then publish one complete reviewable update and follow its run instead of launching duplicates.
Inspect push, `pull_request`, tag, and merge triggers for the hosted effects of one publication.

Inspect the final pushed head's automatic review state without demanding a manual command or full review.
Wait for active automatic review unless the owner's explicit optional-review merge instruction applies.
Report an unresolved hold or hosted gate as pending with the local result and the blocked step.
[Skipped required workflows stay pending](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/skip-workflow-runs).
