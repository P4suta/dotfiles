---
name: skill-operations
description: >-
  Operate and improve the owner's shared skill system through dependency-aware loading, local observations, analysis, evidence-backed decisions, regression checks, and native rollout.
  Use for skill changes, recurring workflow gaps, maintenance findings, or a pending skill-ops gate.
---

# Shared skill operations

The maintained `skill-ops` command and the dotfiles Rust checks hold the operational source of truth.
Keep this infrastructure independent of any code-review service or model provider.
Use `portable-skills` for canonical discovery and `dotfiles` for native installation.
Read [commands and evidence](references/operations.md) for the command surface and record schemas.
Read [analysis and improvement](references/analysis.md) when assessing the catalog or recorded observations.

## Discover and execute

Select the smallest relevant skill or a matching workflow bundle.
Run `skill-ops load NAME` to load the configured prerequisites once, before the requested instruction.
Inspect the plan instead of treating every cross-reference as an unconditional dependency.
Keep conditional references conditional, and reject unknown names, stale links, and dependency cycles.
Read current project instructions and preserve the user's actual authorization.
No skill, dependency, analysis result, or previous task grants permanent authority for external mutations.

Keep the canonical instructions, metadata, supporting resources, client adapters, and versioned check contracts together.
Never duplicate skill bodies across clients or turn project facts into universal standards.
When available, use `skill-creator` to write and check a changed skill.

## Observe and analyze

Native client adapters record loader signals, direct file reads, and weaker shell references apart.
Inspect collector coverage and missing or untrusted hooks before interpreting counts.
An observed load proves no compliance, and an absent signal proves no lack of need.
Never fabricate retrospective counts or treat a third-party skill with a similar name as an owned skill.

Record demonstrated failures, repeated procedures, missing workflows, conflicting instructions, and executable-policy opportunities with `skill-ops note` and concrete local evidence.
Keep prompts, command bodies, credentials, full transcripts, and runtime state out of shared artifacts.
Analyze the current catalog and local observations before a skill change and whenever the maintenance gate turns pending.
Preserve event identities and old decisions, and never let a reinstall reset the history.

## Decide and improve

Assess every finding with real tasks, observed eligibility, supported platforms, existing checks, and the surrounding skill graph.
Options include sharpening discovery, adding a missing skill, introducing a workflow bundle, merging overlap, splitting conditional material, retiring obsolete guidance, and moving deterministic behavior into a maintained checker.
Use `executable-policy` for mechanical enforcement and `verification-tools` for a newly required capability.

Carry out supported accepted improvements within the authorized scope, verify their affected behavior, and record the resulting local evidence.
Reject unsupported recommendations with a concrete reason and evidence.
Give a necessary deferral an explicit reconsideration condition, and expect it back at the next analysis.
Never apply a frequency-based merge unexamined, rubber-stamp findings, or claim that a disposition record proves semantic quality.

Run the same authoritative catalog, link, resource, decision, lint, test, and proof checks before publication and in required CI.
Changing a skill or policy invalidates its analysis, so reassess the current content instead of copying an old success receipt.
Draft a changed skill's decision with `just decide NAME` in the dotfiles repository, and write the reason the new revision deserves.
For a demonstrated behavioral failure, add a realistic regression scenario instead of a test that matches prose wording.
Apply verified changes through each affected host's native profile, and check installed discovery and collector definitions.
Complete applicable repository review and CI before reporting the authorized PR task complete.
