---
name: mutation-testing
description: >-
  Use mutation testing to verify that tests detect broken behavior and repair uncovered invariants.
  Use for survivor analysis, assurance gaps, and the owner's rust-mutants or njutest workflows.
---

# Mutation testing

Read the project's mutation configuration, maintained tasks, and tool manual before selecting a run.
Preserve its chosen tool, compile-time checks, mutation operators, coverage routing, and required CI contract.
Use `resource-coordination` before an expensive campaign and `multi-machine` for host execution.
Start with a dry run or changed-contract selection when it answers the question, without weakening the full campaign's contract.

A killed mutant shows that the selected check noticed that perturbation.
Investigate a surviving mutant's behavior, selected tests, execution result, and mutated source.
Distinguish a missed behavior, an unreachable or behavior-preserving change, a build rejection, a timeout, an infrastructure error, and an operator outside the promised measurement.
Never count an unexecuted or failed campaign as evidence.

For a real survivor, name the invariant and add the test or stronger representation that detects the broken behavior through the supported API.
Use `systematic-fixes` to close the shared verification gap across related variants, never an assertion tailored to the mutant's text.
Cover failure and cleanup behavior, not only the happy path.
Use `formal-assurance` when a checked property can cover the critical mechanism.
Never add exclusions, blanket ignores, weaker thresholds, or accepted-survivor records to turn a report green.
A behavior-preserving or excluded operator needs a precise contract-based justification through the project's narrow existing exception mechanism.

Recheck the affected mutation and its family after the fix, then run the relevant project gates.
Record tool versions, source identity, selected operators, seeds or a deterministic reproducer, host identity, and outcomes.
Keep campaign artifacts out of user-facing documentation.
Finish the authorized assurance work instead of stopping at a survivor list or a documented limitation.
