---
name: mutation-testing
description: >-
  Use mutation testing to verify that tests detect broken behavior and repair uncovered invariants.
  Use for survivor analysis, assurance gaps, and the owner's rust-mutants or njutest workflows.
---

# Mutation Testing

Read the project's current mutation configuration, maintained tasks, and tool manual before selecting a run.
Preserve its chosen tool, compile-time checks, mutation operators, coverage routing, and required CI contract.
Use `resource-coordination` before a full or otherwise expensive campaign and `multi-machine` for actual host execution.
Start with a supported dry run or changed-contract selection when it answers the current question, without weakening the full campaign's contract.

A killed mutant shows that the selected checking mechanism noticed that perturbation.
A surviving mutant requires investigation of the actual behavior, selected tests, execution result, and mutated source.
Distinguish a missed behavior, unreachable or equivalent change, build rejection, timeout, infrastructure error, and an operator outside the promised measurement.
Do not classify an unexecuted or failed campaign as successful evidence.

For a real survivor, identify the invariant and add the test or stronger representation that detects the broken behavior through the supported API.
Use `systematic-fixes` to correct the common verification gap across related variants, not an assertion tailored to the mutant's text.
Keep tests sensitive to failure and cleanup behavior as well as the happy path.
Use `formal-assurance` when a stronger checked property can cover the critical mechanism.
Do not add exclusions, blanket ignores, weakened thresholds, or accepted-survivor records solely to make a report green.
An equivalent or intentionally excluded operator needs a precise contract-based justification and the project's narrow existing exception mechanism.

Recheck the affected mutation and representative family after the correction, then run the relevant ordinary project gates.
Preserve tool versions, source identity, selected operators, seeds or deterministic reproducer, host identity, and actual outcomes needed to reproduce the evidence.
Keep campaign artifacts outside user-facing documentation and retain a durable design change in an ADR only when necessary.
Finish the authorized assurance work rather than stopping at a survivor list or a documented limitation.
