---
name: executable-policy
description: >-
  Turn enforceable development and skill rules into authoritative types, semantic checks, and required gates.
  Use for project standards, repeated agent mistakes, policy drift, or replacing procedural skill instructions with maintained tooling.
---

# Executable Development Policy

For each required rule, identify the observable contract and the narrowest mechanism that can actually enforce it.
Prefer a type or validated construction, then a compiler, semantic lint, schema, or maintained project check.
Keep judgment, intent, authorization, and unsupported boundaries explicit; a wording check cannot prove that an agent followed an instruction.
Use `skill-operations` to track gaps and decide when a recurring workflow deserves a skill or an executable check.

## Establish one authoritative gate

Read the actual producers, consumers, project commands, local hooks, required CI, and existing exclusions.
Implement shared decision logic once with `rust-tooling` or the project's maintained typed tooling.
Use `xtask` and `just` as appropriate for thin discoverable command entry points.
Invoke the same underlying checker from the developer command, hooks, and required CI rather than maintaining different policy interpretations.
Check that the effective hook and workflow really invoke it, including OS-specific execution paths.
An agent instruction or installed executable alone is not a required gate.

Parse semantic data with the format's parser and use authoritative manifests to discover the checked scope.
Reject missing required inputs, tools, expected harnesses, and incomplete or empty coverage.
Distinguish pass, failure, unsupported, skipped, and stale evidence; only an applicable completed check satisfies its obligation.
Bind evidence to the exact source, configuration, tool version, and relevant target or operation.
Avoid time-based success assumptions where a revision or explicit transition provides a stable identity.

Keep necessary exceptions narrow and machine-readable, with the affected rule, scope, reason, evidence, and condition for removal.
An exemption must not silently cover future files or replace stronger existing protections.
Do not treat a report-only tool, ignored exit code, optional CI job, or resolved prose checklist as mechanical enforcement.

## Verify rejection and maintenance

Define the behavior and demonstrate a representative broken input before adding the gate.
Verify that the actual entry point rejects the intended defect with a useful diagnostic and accepts valid supported behavior.
Check that disabling, narrowing, or deleting required coverage is itself detected where the project can enforce it.
Do not add tests that only match headings, tool names, generated wording, or implementation details.

Use `development-assurance`, `formal-assurance`, and `reliability` for the implementation's correctness and external assumptions.
Run supported native checks through `multi-machine`, and use `ci-budget` for coherent publication.
Keep tool and rule updates under existing dependency maintenance and report the exact guarantees enforced.
After moving a rule into code, shorten its skill to the remaining judgment, invocation, failure handling, and limitations.
Preserve discoverability and explanations that still help the user make a meaningful decision.
