---
name: skill-operations
description: >-
  Operate and improve the owner's shared skill system through dependency-aware loading, local observations, analysis, evidence-backed decisions, regression checks, and native rollout.
  Use for skill changes, recurring workflow gaps, maintenance findings, or a pending skill-ops gate.
---

# Shared Skill Operations

Use the maintained `skill-ops` command and the dotfiles Rust checks as the operational source of truth.
Keep this development infrastructure independent of a code-review service or agent provider.
Use `portable-skills` for canonical discovery and `dotfiles` for native installation.
Read [commands and evidence](references/operations.md) for the actual command surface and record schemas.
Read [analysis and improvement](references/analysis.md) when assessing the catalog or recorded observations.

## Discover and execute

Select the smallest relevant skill or an applicable workflow bundle.
Use `skill-ops load NAME` to load the configured prerequisites once, before the requested instruction.
Inspect the plan instead of assuming every cross-reference is an unconditional dependency.
Keep conditional references conditional, and reject unknown names, stale aliases, and dependency cycles.
Read current project instructions and preserve the user's actual authorization.
A skill, dependency, analysis result, or previous task is not permanent authority for external mutations.

Keep the canonical instructions, metadata, supporting resources, client adapters, and versioned check contracts together.
Do not duplicate skill bodies across clients or turn historical project facts into universal standards.
Use `skill-creator` when available for authoring and validating a changed skill.

## Observe and analyze

Native client adapters record loader signals, direct file reads, and weaker shell references separately.
Inspect collector coverage and missing or untrusted hooks before interpreting counts.
An observed load does not prove compliance, and an absent signal does not prove the skill was unnecessary.
Do not fabricate retrospective counts or treat a third-party skill with a similar name as an owned skill.

Record demonstrated failures, repeated procedures, missing workflows, conflicting instructions, and executable-policy opportunities with `skill-ops note` and concrete local evidence.
Keep prompts, command bodies, credentials, full transcripts, and runtime state out of shared artifacts.
Analyze the current catalog and local observations before a skill change and whenever the maintenance gate becomes pending.
Preserve event identities and old decisions; a reinstall must not reset the history.

## Decide and improve

Assess every finding against real tasks, observed eligibility, supported platforms, existing checks, and the surrounding skill graph.
Consider sharpening discovery, adding a missing skill, introducing a workflow bundle, merging overlap, splitting conditional material, retiring obsolete guidance, or moving deterministic behavior into a maintained checker.
Use `executable-policy` for mechanical enforcement and `verification-tools` for a newly required capability.

Implement supported accepted improvements within the authorized scope, verify their affected behavior, and record the resulting local evidence.
Reject unsupported recommendations with a concrete reason and evidence.
Keep a necessary deferral explicit with a condition for reconsideration; it remains a backlog item and is presented again at the next analysis.
Do not blindly apply a frequency-based merge, rubber-stamp every finding, or claim that a disposition record proves semantic quality.

Run the same authoritative catalog, alias, resource, decision, lint, test, and proof checks before publication and in required CI.
Changing a skill or policy invalidates the corresponding analysis; reassess the current content instead of copying an old success receipt.
Add a realistic regression scenario for a demonstrated behavioral failure rather than a test that matches prose wording.
Apply verified changes through each affected host's native profile and check installed discovery and collector definitions.
Complete applicable repository review and CI before reporting the authorized PR task complete.
