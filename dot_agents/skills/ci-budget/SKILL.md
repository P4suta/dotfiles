---
name: ci-budget
description: >-
  Reduce avoidable GitHub Actions runs by verifying coherent changes locally and managing an explicit push hold.
  Use before authorized pushes or PR updates, when Actions allowance is constrained, or when mapping CI checks to the owner's native machines.
---

# Local Verification and CI Budget

Use local verification to find and fix problems before publishing a coherent change.
Keep required hosted checks and current-head PR review as the final gates.
An existing push authorization remains applicable; an active owner-imposed hold is a separate stop that must be explicitly resumed.

## Prepare locally

Read the repository instructions, current diff, workflow triggers, required jobs, and project check entry points.
Map each affected CI job to its authoritative local command and required OS, architecture, environment, or service.
`dotfiles-xtask hosts check` runs the project gate on every OS family in the CI matrix; see `multi-machine`.
Report hosted-only prerequisites such as GitHub event permissions, OIDC, environment approval, and publication separately; a local build does not exercise them.
Local verification does not authorize signing, publication, paid service calls, or registering these machines as self-hosted runners.

Fix related findings in one coherent update and rerun the affected checks.
Reuse successful evidence for the same source and environment until a new change or unresolved failure invalidates it.
Run cheap relevant checks first, and use `resource-coordination` before an expensive campaign.
Use `reliability` for intermittent failures; preserve their evidence instead of repeatedly pushing until CI passes.

## Control publication

Before every push, inspect the effective hooks and the machine-local hold described in [push control](references/push-control.md).
Agent workflows must honor the marker even when a native Git client or existing repository has incomplete hook coverage; do not claim such a client is mechanically protected.
Do not remove a hold, bypass hooks, change CI requirements, or add skip directives to make a push succeed.
A calendar change, an old notification, or broad prior push permission does not resume an active hold.
If allowance is genuinely uncertain after a budget warning, inspect current owner billing evidence before a costly hosted run rather than guessing from repository visibility.

Continue local work, validation, and authorized commits during a hold.
Fetch before integration and publication, inspect divergence, and keep changes in small logical branches or PRs.
Avoid accumulating unrelated work into one large PR while publication is delayed.
A local commit does not imply an immediate push.
Publish a complete reviewable update after local checks and applicable authorization, then observe its existing run instead of launching duplicates.
Inspect push, pull_request, tag, and merge triggers so one publication's full hosted effects are understood.

Complete the repository's enabled automatic reviews of the final pushed head under its review policy.
An unresolved hold or hosted gate remains pending; report the verified local result and the specific blocked publication step.
Check current [Actions billing](https://docs.github.com/en/billing/concepts/product-billing/github-actions) when allowance or runner costs matter.
[Skipped required workflows can remain pending](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/skip-workflow-runs), so skipping is not a substitute for passing them.
