---
name: ci-budget
description: >-
  Reduce avoidable GitHub Actions runs by verifying coherent changes locally and managing an explicit push hold.
  Use before authorized pushes or PR updates, when Actions allowance runs low, or when mapping CI checks to the owner's native machines.
---

# Local verification before pushing

Find and fix problems locally before publishing a coherent change.
Required hosted checks and the current-head PR review stay the final gates.
An active owner-imposed push hold overrides any push authorization until the owner explicitly resumes pushes.

## Prepare locally

Read the repository instructions, the diff, workflow triggers, required jobs, and project check entry points.
Map each affected CI job to its local command and its required OS, architecture, environment, or service.
Run the equivalents on the affected Mac, Linux, and Windows hosts through `multi-machine` and `domyjob`.
Use pinned versions and existing build caches.
Report hosted-only prerequisites on their own, such as GitHub event permissions, OpenID Connect (OIDC), environment approval, and publication.
Local verification grants no signing, publication, paid service calls, or self-hosted runner registration.

Fix related findings in one update and rerun the affected checks.
Reuse passing evidence for the same source and environment until a change or failure invalidates it.
Run cheap checks first, and use `resource-coordination` before an expensive campaign.
Use `reliability` for intermittent failures, and keep their evidence instead of pushing until CI passes.

## Control publication

Before every push, inspect the effective hooks and the machine-local hold in [push control](references/push-control.md).
Honor the hold marker even where a Git client lacks the hook, and never call such a client protected.
Never remove a hold, bypass hooks, change CI requirements, or add skip directives to make a push succeed.
A calendar change, an old notification, or broad prior push permission leaves an active hold in place.
When allowance looks uncertain after a budget warning, check the owner's current [Actions billing](https://docs.github.com/en/billing/concepts/product-billing/github-actions) before a costly hosted run.

Continue local work, verification, and authorized commits during a hold.
Fetch before integration and publication, inspect divergence, and keep changes in small logical branches or PRs.
A local commit implies no push.
Publish a complete reviewable update after local checks and authorization, then follow its run instead of launching duplicates.
Inspect push, `pull_request`, tag, and merge triggers to know every hosted effect of one publication.

Finish the repository's automatic reviews of the final pushed head.
Report an unresolved hold or hosted gate as pending, and name the local result and the blocked publication step.
[Skipped required workflows stay pending](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/skip-workflow-runs), so skipping never replaces passing.
