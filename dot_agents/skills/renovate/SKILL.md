---
name: renovate
description: >-
  Maintain tested dependency and tool-version updates with Renovate, shared presets, lockfiles, and immutable action pins.
  Use for Renovate setup, version freshness, or update failures.
---

# Renovate and version freshness

Read the repository's local configuration and resolved shared preset before changing update behavior.
Keep the common policy in the shared preset and only necessary project differences locally.
The shared source lives at [P4suta/renovate-config](https://github.com/P4suta/renovate-config), selected with `github>P4suta/renovate-config`.
Before relying on its grouping, schedule, update age, digest pinning, and automerge rules, read its current `default.json` and confirm that the repository extends it.
Check current options in [Renovate's documentation](https://docs.renovatebot.com/configuration-options/).
Never copy a whole preset into a repository or guess at an unsupported custom manager.

Track the toolchains, runtime and package-manager versions, direct dependencies, lockfiles, GitHub Actions, containers, and release tools that the project uses.
Prefer supported built-in managers, and add a narrow custom manager only for a real untracked declaration.
For Actions and containers, keep immutable digests with useful version comments, and update them together.
Before choosing the newest suitable stable version, check the upstream release notes, support policy, compatibility constraints, and oldest supported Rust version.
An untested `latest` reference makes no version policy.

Group updates that must change together, and keep unrelated major migrations reviewable.
After an update, verify exact resolved dependencies, the relevant project gates, and consumer compatibility.
Preserve narrow security exceptions, and diagnose newly reported vulnerabilities instead of suppressing the report.
When changing matching rules, run the supported configuration validator and inspect manager extraction or a non-mutating dry run.

Honor protected-branch CI, signed commits, and squash conventions.
Automatic dependency merges require explicit existing policy and green required checks.
Release PRs and production approval gates stay separate.
Use `approval-boundaries` for an AI-submitted approval.
A version update grants no authority for a release, registry upload, tag rewrite, or lockfile change outside the intended dependency set.
