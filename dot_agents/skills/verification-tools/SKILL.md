---
name: verification-tools
description: >-
  Install and maintain mandatory verification tools for the actual language, risks, and supported targets of a project.
  Use for implementation, project setup, assurance gaps, or propagating a proven new check across existing projects.
---

# Required verification tools

Treat fitting verification tools as part of a project's implementation contract.
Choose them by the property they establish, never by a fixed list of popular names.
Use `type-contracts` for typing, `formal-assurance` for proofs, `reliability` for uncontrolled dependencies, and `executable-policy` for enforcement.

## Establish the required coverage

Inspect the actual languages, unsafe or native code, concurrency, input boundaries, dependency graph, generated code, and supported platforms.
Map each relevant failure class to an effective check, and find gaps in the existing project gates.
Require compiler or strict type checks, relevant semantic lints, deterministic behavioral checks, and dependency and secret checks where they apply.
Add memory, undefined-behavior, race, property, fuzz, mutation, or model checks when the implementation exposes those risks.
An existing test suite removes no relevant obligation.

Read current official documentation and maintained upstream repositories before choosing or updating a tool.
Verify supported language versions, target semantics, maintenance, licensing, installation provenance, runtime cost, and compatibility with the project's other checks.
Replace an obsolete tool only after preserving the properties it established.
Read [Rust verification](references/rust.md) for Rust-specific interpretation and tool constraints.

Install and pin the chosen tools through the project's existing `mise` and locked package conventions.
Run them over real production code and the applicable tests or harnesses.
Record the command, covered files and features, supported targets, assumptions, expected nonempty work, and result.
Treat an unsupported target, skipped harness, missing input, empty scan, or tool failure as missing coverage to resolve.
When one tool leaves a boundary uncovered, add a supported complementary check and keep the remaining obligation explicit.
Never suppress the boundary or weaken the property to get a passing result.

## Make adoption durable

Wire required checks into the authoritative project command, fitting local hooks, and required CI.
Use `executable-policy` to verify that the actual gate rejects a representative defect.
Run available native checks through `multi-machine`, and use `ci-budget` before publishing a coherent update.
Use `resource-coordination` for expensive campaigns, because adopting a tool grants no unbounded fuzz run or paid service.

When a new tool or configuration proves useful, inventory the owner's active projects for the same uncovered property.
Carry the proven behavior to compatible projects in small independent changes, and adapt language versions, targets, and check entry points.
Track adoption, incompatibility with concrete evidence, and pending work instead of assuming every repository migrated.
New projects inherit the current capability baseline, and an old project's absence from a recent task grants no permanent exemption.
Use `skill-operations` to record adoption evidence and proposals that move repeated assurance work into maintained checks.
