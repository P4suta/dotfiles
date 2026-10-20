---
name: ci-budget
description: >-
  Keep hosted CI lean: design workflows that run a fast deterministic gate and leave heavy verification to local hosts, and verify coherent changes locally before an explicit push.
  Use when adding or changing GitHub Actions workflows, schedules, fuzzing, mutation, proofs, or OS matrices, before authorized pushes or PR updates, and when Actions allowance runs low.
---

# Lean CI and local verification before pushing

Design hosted workflows from [lean workflows](references/lean-workflows.md): a fast deterministic gate on every change, no schedules, and heavy verification on local or native hosts.
Required hosted checks and the current-head PR review stay the final gates.
An active owner-imposed push pause overrides any push authorization until the owner explicitly resumes pushes.

## Prepare locally

Read the diff, workflow triggers, required jobs, and project check entry points.
Map each affected CI job to its local command and its required OS, architecture, environment, or service.
`dotfiles hosts check` runs the project gate on every OS family in the CI matrix, as `multi-machine` describes.
Report hosted-only prerequisites as unverified locally, such as GitHub event permissions, OIDC, environment approval, and publication.
Local verification grants no signing, publication, paid service calls, or self-hosted runner registration.

Reuse passing evidence for the same source and environment until a change or failure invalidates it.
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

Wait for the automatic reviews of the final pushed head.
Report an unresolved hold or hosted gate as pending with the local result and the blocked step.
[Skipped required workflows stay pending](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/skip-workflow-runs).
