# Preserve included review capacity

Status: superseded for capacity arithmetic and local cadence by [layered review capacity](0008-layered-review-capacity.md).

## Context

The owner wants layered safeguards that keep included usage from running out and stop excess usage polling.
The command-line tool reports usage as numeric structured output.
The official pull request status inquiry, observed on October 4, 2026, reported availability without remaining capacity.
Command-line and pull request reviews draw from separate pools, and dashboard history shows no current numeric reserve.

## Decision

Strengthen the managed command-line boundary instead of adding another review client.
After a one-review request, keep at least the ceiling of 20% of the observed included limit.
Require one hour between attempts and at most four attempts per rolling day.
Count failed attempts durably, and keep existing review history.
Keep a durable ledger for quota probes under its own lock, with a 15-minute gap and eight attempts per rolling day.
Initialize that ledger once with its own persistence marker, keep the existing ledger during upgrade, and refuse deleted or corrupt initialized state.
Run managed reviews and usage queries only on the Mac, and run native builds and tests on their own hosts.
Keep paid usage inactive, and stop when numeric usage goes missing or disagrees.
Honor a persistent machine-local owner pause before any vendor call, permit only the guard's local status, and require explicit resumption.
Keep the pause across installation, and exercise the native executable's refusal paths without service access.
Accept the official standalone `@coderabbitai ignore` directive in a checked pull request body, so an authorized merge can keep the separate pull request review pause.
The directive alone makes no meaningful body and never authorizes unfinished title or summary generation.

Run one budget-qualified command-line review after local checks on an initial substantive pull request.
On an existing pull request, read its review before any duplicate local analysis, fix supported findings, and recheck locally before publishing one coherent update.
Use official usage checks for the separate pull request pool, and hold any operation that would start a review while numeric capacity stays unknown.
One hour sets the shortest wait before reconsideration, and grants neither authorization nor evidence of refill.

## Assurance and limits

Exhaustive source-bound Kani checks cover overflow, rollback, and the reserve arithmetic and cooldown of the production functions in `xtask/src/review_rules.rs`.
The proof runner checks exact harness inventories, reachable acceptance and refusal, and false reserve and early review controls.
An exhaustive proof shows that the production pull request marker predicate never lets review exclusion permit unfinished generation, and the gate refuses a false summary control.
Native process tests check exact directive decoding and preservation, and refuse directive-only, unsupported, and unfinished bodies before `gh` starts.
Native integration tests exercise locked files, durable preflight reservations, failed probes, rolling-day limits, supported usage spellings, and installation history.
Operating-system locking, persistence, clocks, the pinned vendor tool, and truthful service usage stay external assumptions.
The shared installation keeps service credentials and every local observation and ledger.

The managed command-line gates enforce themselves in code.
The dedicated pull request command leaves ordinary Git and `gh` alone, so publication holds stay explicit obligations of the session.
Service snapshots reserve no capacity from browser, IDE, automatic pull request, or unmanaged clients.
This change claims no account-wide ceiling and no protection from every provider restriction.
Extending the limit past the managed entry points needs a service-enforced cap or a shared reservation gateway.
Reconsider numeric pull request automation when an official endpoint reports the current pool, identity, and remaining allowance.
