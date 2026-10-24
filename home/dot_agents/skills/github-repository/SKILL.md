---
name: github-repository
description: >-
  Audit and reconcile GitHub merge settings, immutable releases, rulesets, and environment protections to the personal repository baseline.
  Use for repository governance and drift, not to publish a release.
---

# GitHub repository governance

Read project instructions and inspect the live repository before changing settings.
Use the Rust `repo-settings` CLI in [scripts/repo-settings](scripts/repo-settings/Cargo.toml) with `gh` authentication.
Its audit checks the remote default branch with `dotfiles repository check --remote OWNER/REPO`, so install the reviewed `dotfiles` checker first.
Resolve metadata, README, description, and license drift through `repository.toml` and `dotfiles repository generate --remote OWNER/REPO`.
The [baseline](assets/baseline.json) defines the personal convention: squash-only merges, PR-title commits, an empty default commit body, branch cleanup, signed linear protected default branches, and protected version tags.
The baseline requires a squash merge queue only for organization-owned repositories whose active default-branch rulesets require status checks.
Audit records the repository owner's type as `User` or `Organization`, and older snapshots without that field default to `User`.
For user-owned repositories, plan no queue change, preserve the strict check policy, and report no queue findings.
Personal repositories rely on stacked pull requests, which merge atomically, and keep strict required checks.
Audit reads workflow YAML from the default branch and identifies the jobs that report each required check.
Every reporting workflow must include the `merge_group` trigger before the plan adds a queue.
Resolve findings that name a missing trigger or an unverified check reporter before enabling the queue.
The queue uses ALLGREEN grouping and tests one entry at a time, with no batching wait and a 60-minute check timeout.
With the queue, set `strict_required_status_checks_policy` to false because the queue tests each change on the current base.
Update the existing ruleset and preserve its other fields, rules, and bypass actors.
Web signoff differs from cryptographic commit signing.

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
- `security` turns on secret scanning, push protection, and SHA pinning for actions, and reports a repository that runs any action, because only a reviewer can list the permitted ones.

The apply report records started, verified, and failed operations, so partial completion stays visible.
After a failure, audit and plan again, and never roll back by weakening a protection.

Preserve existing required checks, reviewers, wait timers, custom deployment ref policies, extra rules, and registry bindings.
Never invent a required CI context or guess publication reviewers from an environment name.
Resolve findings with actual workflow jobs and successful check evidence.
Before turning on Immutable Releases, confirm that the release flow creates a draft, attaches final assets and evidence, then publishes once.
Fix a publish-first flow or asset-overwriting retries first, and never turn immutability off.
Keep tag creation permissions apart from no-bypass update and deletion protection.
Use `release-workflow`, `doppler`, and `code-signing` for release and credential work.
