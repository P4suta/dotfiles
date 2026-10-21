---
name: mutation-testing
description: >-
  Use mutation testing to verify that tests detect broken behavior and repair uncovered invariants.
  Use for survivor analysis, assurance gaps, and a project's pinned mutation workflow.
metadata:
  tools: "tools.toml"
---

# Mutation testing

Read the project's mutation configuration and maintained tasks before selecting a run.
Use the project's pinned mutation tool and documented platform runner.
Apply the bounded-tool rule in `formal-assurance` through the project's bounded container entrypoint or `dotfiles bounded run` on owner machines.
Follow `formal-assurance` for reproducible verdicts and `resource-coordination` before an expensive campaign.
Start with a dry run or changed-contract selection when it answers the question.

Investigate a surviving mutant's behavior, selected tests, execution result, and mutated source.
Distinguish a missed behavior, an unreachable mutation, a build rejection, a timeout, an infrastructure error, and an operator outside the promised measurement.
Keep a timeout inconclusive, never count it as a killed or surviving mutant.
Never count an unexecuted or failed campaign as evidence.

For a real survivor, name the invariant and add the test or stronger representation that detects the broken behavior through the supported API.
Use `systematic-fixes` to close the shared verification gap across related variants, never an assertion tailored to the mutant's text.
Cover failure and cleanup behavior, not only the happy path.
Use `formal-assurance` when a checked property can cover the critical mechanism.
Never add exclusions, blanket ignores, weaker thresholds, or accepted-survivor records to turn a report green.
An excluded operator needs a precise contract-based justification through the project's narrow existing exception mechanism.

Recheck the affected mutation and its family after the fix, then run the relevant project gates.
Record tool versions, source identity, selected operators, configured bounds and seeds, runner configuration, and outcomes.
Finish the authorized assurance work instead of stopping at a survivor list.

Keep owner-side tooling within the boundary in `formal-assurance`.
