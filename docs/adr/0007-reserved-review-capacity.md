# Preserve Included Review Capacity

Status: Partially superseded by [ADR 0008](0008-layered-review-capacity.md) for capacity arithmetic and local cadence.

## Context

The owner requires multiple safeguards against exhausting included usage and against excessive usage polling.
CLI usage is available as numeric structured output, while the official PR status inquiry observed on October 4, 2026 returned only availability without remaining capacity.
CLI and PR reviews draw from separate pools, and historical dashboard rates cannot establish a current numeric reserve.

## Decision

Strengthen the existing managed CLI boundary rather than introducing another review client.
Keep at least the ceiling of 20% of the observed included limit after a one-review request.
Require one hour between attempts and at most four attempts per rolling day.
Count failed attempts durably and preserve existing review history.
Use a separately locked, durable ledger for quota probes, with a 15-minute gap and eight attempts per rolling day.
Initialize this additional ledger once with its own persistence marker, preserving the existing ledger during upgrade and refusing deleted or corrupt initialized state.
Run managed reviews and usage queries only on the Mac across the three deployed hosts; native builds and tests still run on their appropriate hosts.
Keep paid usage inactive and stop when numeric usage is missing or inconsistent.
Honor a persistent machine-local owner pause before any vendor invocation, allowing only the guard's local status and requiring explicit resumption.
Preserve the pause during installation and exercise the real native executable's refusal paths without service access.
Permit the official standalone `@coderabbitai ignore` directive in a checked PR body so an authorized merge can preserve the separate PR review pause.
The directive alone is not a meaningful body and cannot authorize unfinished title or summary generation.

Use one budget-qualified CLI review after local checks for an initial substantive PR.
Inspect an existing PR's review before duplicate local analysis, fix supported findings, and recheck locally before publishing one coherent update.
Use official supported usage checks for the separate PR pool and hold an operation that would start a review if numeric capacity cannot be established.
One hour is a minimum reconsideration interval, not automatic authorization or evidence of refill.

## Assurance and limits

The production functions in `xtask/src/review_rules.rs` have exhaustive source-bound Kani checks for reserve arithmetic and cooldown, including overflow and rollback.
The required proof runner checks exact harness inventories, reachable acceptance and rejection, and deliberately false reserve and early-review assertions.
The production PR marker predicate has an exhaustive proof that review exclusion cannot authorize unfinished generation, with a deliberately false summary claim rejected by the gate.
Native process tests verify exact directive decoding and preservation and reject directive-only, unsupported, and unfinished bodies before `gh` starts.
Native integration tests exercise actual locked files, durable preflight reservations, failed probes, rolling-day limits, supported usage spellings, and installation history.
Operating-system locking, persistence, clocks, the pinned vendor CLI, and truthful service usage remain external assumptions.
The shared installation preserves service credentials and all preexisting local observations and ledgers.

The managed CLI gates are mechanical; the current dedicated PR command does not intercept ordinary Git or `gh`, so publication holds remain explicit agent obligations.
Service snapshots cannot reserve capacity against browser, IDE, automatic PR, or unmanaged clients.
This change does not claim an account-wide absolute ceiling or protection against every provider restriction.
A service-enforced cap or a shared reservation gateway is required before extending that guarantee beyond the managed entry points.
Reconsider numeric PR automation when an official supported current-capacity endpoint or response supplies the required pool, identity, and remaining allowance.
