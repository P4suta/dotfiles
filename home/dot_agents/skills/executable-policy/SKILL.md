---
name: executable-policy
description: >-
  Turn enforceable development and skill rules into authoritative types, semantic checks, and required gates.
  Use for project standards, repeated agent mistakes, policy drift, or replacing procedural skill instructions with maintained tooling.
---

# Executable development policy

For each required rule, identify the observable contract and the narrowest mechanism that enforces it.
Prefer a type or checked construction, then a compiler, semantic lint, schema, or maintained project check.
Keep judgment, intent, authorization, and unsupported boundaries explicit.
Use `skill-operations` to track gaps and decide when a recurring workflow deserves a skill or an executable check.

## Establish one authoritative gate

Read the producers, consumers, project commands, local hooks, required CI, and existing exclusions.
Write shared decision logic once with `rust-tooling` or the project's typed tooling.
Use `xtask` and `just` for thin discoverable command entry points.
Call the same checker from the developer command and its gate: hooks and pull-request CI for a fast check, or the release workflow for a heavy one.
Check that the effective hook and workflow call it, and that the release workflow covers each supported OS.
An instruction or an installed executable alone makes no required gate.

Parse structured data with the format's parser, and discover the checked scope from authoritative manifests.
Reject missing inputs, tools, and harnesses, and incomplete or empty coverage.
Distinguish passing, failing, inconclusive, unsupported, skipped, and stale evidence, and accept only a completed applicable check.
Bind evidence to the exact source, configuration, tool version, and target or operation.
Use a revision or explicit transition as identity instead of time-based success.

Keep necessary exceptions narrow and machine-readable, with the rule, scope, reason, evidence, and removal condition.
An exemption never covers future files or replaces stronger existing protections.
A report-only tool, an ignored exit code, an optional CI job, or a resolved prose checklist enforces nothing.

## Verify rejection and maintenance

Show a representative broken input before adding the gate.
Verify that the entry point rejects the defect with a useful diagnostic and accepts valid behavior.
Detect the removal, narrowing, or deletion of required coverage where the project can enforce it.
Never add tests that only match headings, tool names, generated wording, or implementation details.

Use `development-assurance`, `formal-assurance`, and `reliability` for correctness, reproducible verdicts, and external assumptions.
Run native checks through the project's documented hosted runners or pinned container, and use `ci-budget` to place each check at the fast gate or the release gate.
After moving a rule into code, cut its skill down to the remaining judgment, invocation, failure handling, and limits.
A skill never repeats a version or exploration bound that a gate pins, and the skill check refuses one that does.

Keep owner-side tooling within the boundary in `formal-assurance`.
