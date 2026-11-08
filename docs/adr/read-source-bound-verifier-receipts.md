---
status: proposed
date: 2026-11-06
---

# Read source-bound verifier receipts

## Context

The Kani reader recognizes one owner's count summary.
Other maintained runners store native reports beside source digests.

## Decision

Read the existing receipt format identified by verification phase records.
Match each phase digest to its receipt, validate native harness inventories and counterexamples, and store only referenced artifacts.
Bind captured bytes to their hashes before granting result authority.
Preserve the existing summary protocol when no receipt phases appear.

## Alternatives

Requiring projects to print the owner's summary couples project output to private tooling and checks counts without native identities.
Printing full verifier traces increases routine output and requires project changes.

## Consequences

The owner trusts the project's mapping from its digest to verifier inputs.
The runner still binds the complete checkout before and after execution.
Unknown formats and malformed phases fail explicitly.

## Confirmation

Tests must reject missing artifacts, digest mismatches, failed or incomplete proofs, missing counterexamples, path traversal, and altered retained bytes.
Reconsider the protocol when an upstream runner changes its persisted format.
