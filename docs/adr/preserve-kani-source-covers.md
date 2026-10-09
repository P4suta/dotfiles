---
status: accepted
date: 2026-10-09
---
# Preserve source witnesses under checked Kani MIR

## Context

The proof gate requires every declared cover to reach a production behavior.
In the pinned compiler, MIR jump threading duplicates cover sites across control-flow branches.
The clones produce unsatisfiable properties beside satisfied copies of the same source witnesses.
Repeated calls to a pure branching verdict can also clone later witness sites.
The same optimized compiler rejects proof constants whose enum variants have unused payload bytes.
The Miri binaries also exceed the existing budget when exhaustive matrices and repeated test setup run together.

## Decision

The Kani runner keeps its existing [checked MIR configuration](optimize-checked-kani-mir.md).
Pure, total Boolean cover operands use eager Boolean conjunction when short-circuit lowering duplicates their source sites.
The conjunction retains exactly the same truth value over every original input.
Witnesses of one pure verdict reuse its result across their original predicates.
Branching witness predicates are evaluated before cover sites to preserve one site for each original condition.
Partial operands retain their safety conditions through checked access, such as `last` for an empty slice.
Enum discriminator predicates use equivalent variant patterns to avoid materializing unused payload constants.
Production decisions, assertion truth conditions, and original input domains remain unchanged.
Overflow checks, debug assertions, source domains, postconditions, every cover, default memory encoding and solver, and the existing proof budget remain required.
Inventory discovery and verification use the same command constructor.
Unverified property errors name the harness and list each property's category, status, and description.
Each discovered harness has its own result file and exact identity check, keeping the shared JSON input limit unchanged.
Rejecting probes receive the same partitioning and must still fail on their intended assertions.
Measured slow exhaustive matrices remain fully enabled in native tests and use the owner's requested Miri selection.
Other enabled pure cases share one Miri test within each module to avoid repeated setup while executing every original assertion.
Native test identities remain separate.

## Validation

The legacy bypass harness retains all three witness predicates and passes with one satisfied property per declared cover.
The freshness harness retains all four verdict witnesses and passes with one satisfied property per declared cover.
The body and aside harnesses retain every original acceptance and refusal witness.
The boundary harness retains its full integer domains, breach assertions, and all four witnesses after replacing equivalent enum comparisons.
Rejecting probes use the current success variant names while retaining their original facts and false claims.
The fixed paid-usage probe uses the production classifier's unwind bound so its original argument reaches the intended assertion instead of an incomplete loop.
The combined review result exceeds the existing JSON limit, so the runner validates every production harness and every rejecting probe in separate bounded receipts.
The normal gate compiles every production harness and rejecting probe with the same flags.
The planner and reservation checks retain their complete existing domains and postconditions.
The Kani phase proves every discovered production harness and rejects every recorded counterexample.
All 189 native rules tests retain their individual identities and assertions.
The grouped Miri suite completes in 24.13 seconds on the host and 28.95 seconds for Windows, within the unchanged budget.

## Reconsider

Reassess the cover expressions when the pinned compiler or Kani changes and short-circuit witnesses survive optimization without duplicate properties.
