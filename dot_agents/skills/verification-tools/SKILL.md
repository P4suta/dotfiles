---
name: verification-tools
description: >-
  Install and maintain mandatory verification tools for the actual language, risks, and supported targets of a project.
  Use for implementation, project setup, assurance gaps, or propagating a proven new check across existing projects.
---

# Required Verification Tools

Treat suitable verification tools as part of a project's implementation contract.
Choose them by the guarantee they establish rather than keeping a permanent list of fashionable tool names.
Use `type-contracts` for typing, `formal-assurance` for proofs, `reliability` for uncontrolled dependencies, and `executable-policy` for enforcement.

## Establish the required coverage

Inspect the actual languages, unsafe or native code, concurrency, input boundaries, dependency graph, generated code, and supported platforms.
Map each relevant failure class to an effective check and identify gaps in the existing project gates.
Require compiler or strict type checks, relevant semantic lints, deterministic behavioral checks, and dependency and secret checks where they apply.
Add memory, undefined-behavior, race, property, fuzz, mutation, or model checks when the implementation exposes their corresponding risks.
An existing test suite does not remove a relevant obligation.

Consult current official documentation and maintained upstream repositories before selecting or updating a tool.
Verify supported language versions, target semantics, maintenance, licensing, installation provenance, runtime cost, and compatibility with the project's other checks.
Reject redundant tools that add no useful coverage, and replace an obsolete tool only after its required guarantees are preserved.
Read [Rust verification](references/rust.md) for Rust-specific interpretation and tool constraints.

Install and pin the selected tools through the project's existing `mise` and locked package conventions.
Exercise them against real production code and the applicable tests or harnesses.
Record the command, covered files and features, supported targets, assumptions, expected nonempty work, and result.
An unsupported target, skipped harness, missing input, empty scan, or tool failure is missing coverage to resolve.
Where one tool cannot cover a boundary, use a supported complementary check and keep the remaining obligation explicit.
Do not suppress the boundary or weaken the guarantee merely to obtain success.

## Make adoption durable

Integrate required checks into the authoritative project command and appropriate local hooks and required CI.
Use `executable-policy` to verify that a representative defect is rejected by the actual gate.
Run available native checks through `multi-machine` and use `ci-budget` before publishing a coherent update.
Use `resource-coordination` for expensive campaigns; tool adoption does not authorize an unbounded fuzz run or paid service.

When a new tool or configuration proves useful, inventory the owner's active projects for the same uncovered guarantee.
Apply the proven behavior to compatible projects in small independent changes, adapting language versions, targets, and check entry points.
Track adoption, incompatibility with concrete evidence, and pending work instead of assuming all repositories were migrated.
New projects must inherit the current capability baseline; an old project's absence from a recent task is not a permanent exemption.
Reassess discovery when starting a project, changing a language or dependency boundary, or processing a tool update.
Use `skill-operations` to record useful adoption evidence and proposals to move repeated assurance work into maintained checks.
