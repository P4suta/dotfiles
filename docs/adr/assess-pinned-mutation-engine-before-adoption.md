---
status: proposed
date: 2026-10-13
---

# Assess a pinned mutation engine before adoption

## Status

Proposed.

## Context

The local native cargo-mutants campaign establishes complete outcomes with zero surviving mutants and an assertion-free rejecting control.
Hosted mutation is paused while njutest is assessed.
The pinned njutest contract gives affirmative mutation answers from sealed `wasm32-wasip1` executions with 32-bit usize.
Native leads and unproven rows cannot establish parity with the current native 64-bit contract.

## Decision

Keep native cargo-mutants authoritative and hosted mutation paused until a reviewed complete comparison establishes replacement eligibility.
Use an optional named mise environment for the immutable njutest Git revision, generated for the repository and rendered globally from one canonical partial.
Selecting the environment and preparing it through the guarded installer makes the same tool available from other project directories.
Default hosted installation does not select this environment.

Validate complete original upstream documents against their original offline closed schemas before projecting assessment facts.
Record the original Git blob identities, SHA-256 values and revision in the vendor manifest.
Imported documents remain imported assessments and grant no execution or native parity credit.
Only a normal invocation of the pinned existing executable over an owned protected committed snapshot records executed assessment origin.
Its normal phase producer retains the invocation, source, executable, compiler, configuration, terminal status and original report bytes.
The snapshot and artifact namespace are owner-managed; arbitrary privileged replacement by the owner is outside this contract.

Map old and new operators only where source digest, byte span, original bytes and replacement bytes agree.
Keep every unmapped old operator as a gap and preserve the complete new catalog rather than assuming equivalent coverage.
Measure the cold engine cache and its warm readback on the same captured source and owned cache, with prepared Cargo dependency inputs identified separately.
Run the actual assertion-free fixture and require an observed survivor instead of treating an infrastructure error as a sensitivity control.

## Alternatives

Adopting njutest before the complete comparison would substitute sealed 32-bit evidence for the native 64-bit contract without established parity.
Treating imported reports as executed assessments would omit the owned producer's captured source and actual invocation.
Default installation of the optional engine would incur its setup cost before compatibility and replacement eligibility are established.

## Consequences

The assessment can inspect real reports before new execution and makes incompatibility visible without weakening the required campaign.
Preparing the optional tool has a one-time setup cost that remains separate from mutation measurement.
Compilation compatibility, complete native operator parity, declared nontermination correspondence and cold/warm campaign costs remain unmeasured until the actual campaign runs.
An assessment is not a replacement mutation receipt or a repository-wide assurance verdict.

[The pinned upstream contract](https://github.com/P4suta/njutest/blob/46d5b5f4446503162ea18189756249231abb6fc7/docs/assurance-contract.md) defines the evidence scope.
[The original document manifest](../../xtask/schema/njutest/manifest.json) binds the vendored bytes.
[The adapter](../../xtask/src/njutest_assessment.rs) and its producer retain the assessment boundary.
