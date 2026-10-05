---
name: ci-budget
description: >-
  Reduce avoidable GitHub Actions runs by verifying coherent changes locally and managing an explicit push hold.
  Use before authorized pushes or PR updates, when Actions allowance runs low, or when mapping CI checks to the owner's native machines.
---

# Local verification before pushing

Required hosted checks and the current-head PR review stay the final gates.
An active owner-imposed push hold overrides any push authorization until the owner explicitly resumes pushes.

## Prepare locally

Read the diff, workflow triggers, required jobs, and project check entry points.
Map each affected CI job to its local command and its required OS, architecture, environment, or service.
Run the equivalents on the affected Mac, Linux, and Windows hosts through `multi-machine` and `domyjob`.
Report hosted-only prerequisites as unverified locally, such as GitHub event permissions, OIDC, environment approval, and publication.
Local verification grants no signing, publication, paid service calls, or self-hosted runner registration.

Reuse passing evidence for the same source and environment until a change or failure invalidates it.
Run cheap checks first, and use `resource-coordination` before an expensive campaign.
Use `reliability` for intermittent failures instead of pushing until CI passes.

## Control publication

Before every push, inspect the effective hooks and the machine-local hold in [push control](references/push-control.md).
Honor the marker even where a Git client lacks the hook.
Never remove a hold, bypass hooks, change CI requirements, or add skip directives to make a push succeed.
Only the owner's explicit instruction resumes a hold.
Before a costly hosted run with uncertain allowance, check the owner's [Actions billing](https://docs.github.com/en/billing/concepts/product-billing/github-actions).

Continue local work, verification, and authorized commits during a hold.
A local commit implies no push.
Fetch before publishing, then publish one complete reviewable update and follow its run instead of launching duplicates.
Inspect push, `pull_request`, tag, and merge triggers for the hosted effects of one publication.

Wait for the automatic reviews of the final pushed head.
Report an unresolved hold or hosted gate as pending with the local result and the blocked step.
[Skipped required workflows stay pending](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/skip-workflow-runs).
