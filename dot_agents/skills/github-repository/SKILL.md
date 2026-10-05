---
name: github-repository
description: >-
  Audit and reconcile GitHub merge settings, immutable releases, rulesets, and environment protections with the personal repository baseline.
  Use for repository governance and drift, not to publish a release.
---

# Repository governance

Read project instructions and inspect the live repository before changing settings.
Use the Rust `repo-settings` command in [scripts/repo-settings](scripts/repo-settings/Cargo.toml) with `gh` authentication.
It reads governance metadata only, and it has no operation that reads credentials, publishes releases, rewrites refs, deletes protections, or turns off Immutable Releases.
The [baseline](assets/baseline.json) defines the personal convention: squash-only merges, PR-title commits, an empty default commit body, branch cleanup, signed linear protected default branches, and protected version tags.
Web signoff records a Developer Certificate of Origin attestation, which differs from cryptographic commit signing.
Keep Conventional Commit titles, and let existing required CI and stronger reviews enforce their contracts.

Install the helper without publishing a package:

```sh
mise x -- cargo install --locked --path ~/.agents/skills/github-repository/scripts/repo-settings
```

Write reports to fresh paths outside the repository:

```sh
repo-settings audit --owner OWNER --repo OWNER/PRIVATE-REPO --output /tmp/governance-before.json
repo-settings plan --snapshot /tmp/governance-before.json --output /tmp/governance-plan.json
repo-settings apply --plan /tmp/governance-plan.json --report /tmp/governance-apply.jsonl
repo-settings verify --snapshot /tmp/governance-before.json --output /tmp/governance-after.json
```

Owner discovery covers active public owned non-fork repositories, and private repositories need explicit selection or `--include-private`.
Inspect the plan and findings, then apply within the user's authorized scope without asking again.
Pass the same `--scope` to `plan` and `verify`:

- `merge-settings` reconciles merge defaults alone, and apply recomputes the recorded scope, so the plan never carries release or environment changes.
- `environment-protections` removes administrator bypass and preserves environment reviewers, deployment policies, repository settings, rulesets, and release immutability.
- `release-immutability` applies only after confirming that publication completes the draft and never overwrites published assets.

Apply refuses edited plans and stale settings, checks repository identity before each write, and reads the settings back.
The apply report records started, verified, and failed operations, so partial completion stays visible.
The GitHub API offers no transactions, and concurrent external changes can still fail verification.
After a failure, audit and plan again, and never roll back by weakening a protection.

Preserve existing required checks, reviewers, wait timers, custom deployment ref policies, extra rules, and registry bindings.
The command adds missing baseline protections without deleting rulesets, and it removes administrator bypass from existing release environments.
It never invents a required CI context or guesses publication reviewers from an environment name.
Resolve findings with actual workflow jobs and successful check evidence.
Before turning on Immutable Releases, confirm that the release flow creates a draft, attaches final assets and evidence, then publishes once.
Fix a publish-first flow or asset-overwriting retries first, and never turn immutability off to keep that order.
Keep tag creation permissions apart from no-bypass update and deletion protection.
Use `release-workflow`, `doppler`, and `code-signing` for release and credential work.
