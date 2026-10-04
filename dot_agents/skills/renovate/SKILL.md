---
name: renovate
description: >-
  Maintain tested dependency and tool-version updates with Renovate, shared presets, lockfiles, and immutable action pins.
  Use for Renovate setup, version freshness, or update failures.
---

# Renovate and Version Freshness

Read the repository's local configuration and resolved shared preset before changing update behavior.
Use the existing shared preset as the common policy and leave only necessary project differences locally.
For these personal projects, the shared source is [P4suta/renovate-config](https://github.com/P4suta/renovate-config), normally selected with `github>P4suta/renovate-config`.
Read its current `default.json` and confirm that the consuming repository actually extends it before relying on its grouping, schedule, update age, digest pinning, and automerge rules.
Verify current options with [Renovate's documentation](https://docs.renovatebot.com/configuration-options/).
Do not copy a whole preset into every repository or enable an unsupported custom manager by guessing.

Track toolchains, runtime and package-manager versions, direct dependencies, lockfiles, GitHub Actions, containers, and release tools that the project actually uses.
Prefer supported built-in managers; use a narrowly scoped custom manager only for a real untracked declaration.
For Actions and containers, retain immutable digests and useful version comments, and update them together.
Check the upstream release and changelog, support policy, compatibility constraints, and MSRV before selecting the newest suitable stable version.
An untested `latest` reference is not a maintained version policy.

Group updates that need to change together and keep unrelated major migrations reviewable.
Verify exact resolved dependencies, the relevant project gates, and consumer compatibility after an update.
Preserve narrow security exceptions and diagnose newly reported vulnerabilities rather than suppressing the report.
Validate the configuration with the supported validator and inspect manager extraction or a non-mutating dry run when changing matching rules.

Honor protected-branch CI, signed commits, and squash conventions.
Automatic dependency merges require explicit existing policy and green required checks; release PRs and production approval gates stay separate.
Use `approval-boundaries` for an AI-submitted approval.
Version-update authorization does not authorize a release, registry upload, tag rewrite, or lockfile change outside the intended dependency set.
