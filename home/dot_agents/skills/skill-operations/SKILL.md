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
Keep conditional references conditional, and reject unknown names, stale aliases, and dependency cycles.
No skill, dependency, analysis result, or previous task grants permanent authority for external mutations.
Never duplicate skill bodies across clients or turn project facts into universal standards.
Use `skill-creator` to write and check a changed skill when available.

## Observe and analyze

Native client adapters record loader signals, direct file reads, and weaker shell references apart.
Inspect collector coverage and missing or untrusted hooks before interpreting counts.
An observed load proves no compliance, and an absent signal proves no lack of need.
Never fabricate retrospective counts or treat a third-party skill with a similar name as an owned skill.

Record demonstrated failures, repeated procedures, missing workflows, conflicting instructions, and executable-policy opportunities with `skill-ops note` and concrete local evidence.
Keep prompts, command bodies, credentials, full transcripts, and runtime state out of shared artifacts.
Analyze the current catalog and local observations before a skill change and whenever the maintenance gate turns pending.
Never let a reinstall reset the history.

## Decide and improve

Assess every finding with real tasks, observed eligibility, supported platforms, existing checks, and the surrounding skill graph.
Use `executable-policy` for mechanical enforcement and `verification-tools` for a newly required capability.
Carry out supported accepted improvements within the authorized scope, verify their affected behavior, and record the resulting local evidence.
Reject unsupported recommendations with a concrete reason and evidence.
Give a necessary deferral an explicit reconsideration condition.
Never apply a frequency-based merge unexamined or claim that a disposition record proves semantic quality.

Run the same authoritative catalog, link, resource, decision, lint, test, and proof checks before publication and in required CI.
Changing a skill or policy invalidates its analysis, so reassess the current content instead of copying an old success receipt.
Draft a changed skill's decision with `just decide NAME` in the dotfiles repository, and write the reason the new revision deserves.
For a demonstrated behavioral failure, add a realistic regression scenario instead of a test that matches prose wording.
Apply verified changes through each affected host's native profile, and check installed discovery and collector definitions.
Complete applicable repository review and CI before reporting the authorized PR task complete.
