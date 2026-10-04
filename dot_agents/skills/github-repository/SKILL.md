---
name: github-repository
description: >-
  Audit and reconcile GitHub merge settings, immutable releases, rulesets, and environment protections against the personal repository baseline.
  Use for repository governance and drift, not to publish a release.
---

# GitHub Repository Governance

Read project instructions and inspect the live repository before changing settings.
Use the Rust `repo-settings` CLI in [scripts/repo-settings](scripts/repo-settings/Cargo.toml) with `gh` authentication.
It reads governance metadata only and has no operations for reading credentials, publishing releases, rewriting refs, deleting protections, or disabling Immutable Releases.
The [baseline](assets/baseline.json) describes the personal convention: squash-only merges, PR-title commits, an empty default commit body, branch cleanup, signed linear protected default branches, and protected version tags.
Web signoff records DCO attestation separately from cryptographic commit signing.
Keep Conventional Commit titles and let existing required CI and stronger reviews continue to enforce their contracts.

Install the helper without publishing a package:

```sh
mise x -- cargo install --locked --path ~/.agents/skills/github-repository/scripts/repo-settings
```

Use fresh output paths and retain reports outside the repository:

```sh
repo-settings audit --owner OWNER --repo OWNER/PRIVATE-REPO --output /tmp/governance-before.json
repo-settings plan --snapshot /tmp/governance-before.json --output /tmp/governance-plan.json
repo-settings apply --plan /tmp/governance-plan.json --report /tmp/governance-apply.jsonl
repo-settings verify --snapshot /tmp/governance-before.json --output /tmp/governance-after.json
```

Owner discovery includes active public owned non-fork repositories; private repositories require explicit selection or `--include-private`.
Inspect the plan and findings before applying within the user's authorized scope.
Use `plan --scope merge-settings` and the same scope for `verify` when reconciling merge defaults separately from release and approval gates.
The scope is captured in the plan and recomputed during apply, so release or environment mutations cannot be inserted into a merge-settings plan.
Use `plan --scope environment-protections` and the same scope for `verify` to disable administrator bypass while preserving environment reviewers, deployment policies, repository settings, rulesets, and release immutability.
Use `plan --scope release-immutability` and the same scope for `verify` only after verifying that publication completes the draft and never overwrites published assets.
Do not ask again for routine setting changes the user already authorized.
Apply refuses edited plans and stale settings, checks repository identity before each write, and reads the settings back.
The apply report records started, verified, and failed operations so partial completion is visible.
GitHub's REST endpoints are not a transaction; concurrent external changes can still fail verification.
After a failure, audit and plan again; do not roll back by weakening a protection.

Preserve existing required checks, reviewers, wait timers, custom deployment ref policies, additional rules, and registry bindings.
The CLI adds missing baseline protections without deleting existing rulesets and disables administrator bypass for existing release-related environments.
It never invents a required CI context or assigns publication reviewers to an automation environment by guessing its name.
Resolve findings against actual workflow jobs and successful check evidence.
Before enabling Immutable Releases, verify that the actual release flow creates a draft, attaches final assets and evidence, then publishes once.
Fix a publish-first flow and retries that overwrite assets before enabling the setting; do not disable immutability to preserve that ordering.
Keep tag creation permissions separate from no-bypass update and deletion protection.
Use `release-workflow`, `doppler`, and `code-signing` for the conditional release and credential work.
