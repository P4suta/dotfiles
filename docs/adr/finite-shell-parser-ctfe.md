---
status: proposed
date: 2026-10-16
---
# Evaluate fixed parser domains before symbolic selection

## Context

The six continuation-parser harnesses have finite input domains, but symbolic execution of their nested scans exceeds the existing proof budget.
Their properties, admitted inputs, and reachable acceptance and refusal witnesses must remain required.
The [obligation manifest](checked-obligation-manifest.md) binds each property to the actual implementation.

## Decision

The production [continuation parser](../../crates/rules/src/shell_cache_rules.rs) is a `const fn` with one body for ordinary execution and Rust constant evaluation.
The pinned Kani compiler evaluates that function while constructing the harnesses' result tables.
No separate parser, handwritten output table, stub, or conditional implementation supplies their results.

The shape tables evaluate all 48,128 combinations of the original 74-byte, 76-byte, and 38-byte inputs, each changed at every offset to every byte value.
The boundary harness retains its two independent symbolic prefix bytes and their complete Cartesian domain.
Its independent assertions use separate 256-entry tables, with the original two valid suffixes and two duplicate assignments evaluated as well.

Rows retain the offset that Kani compares with the selected symbolic input.
Each cell uses two bits for unevaluated, refusal, acceptance, or invalid data.
Both unevaluated and invalid cells fail instead of looking like an expected refusal.
The constructor uses the same byte to change the production input and select the cell receiving its result.
A separate Kani harness proves that this encoding reads back the written result and preserves every other byte's cell, over arbitrary storage, byte keys, and Boolean results without assumptions.
The independent expected predicates and original cover conditions remain in the Kani harnesses.

Constant evaluation must finish and rejects a panic, an invalid access, or an arithmetic overflow in any evaluated call.
Kani checks the selection, evaluation completeness, original postconditions, and reachable witnesses over every admitted symbolic input.
The existing source and compiler binding covers the table constructors and the production function together.
The rejecting checks must also demonstrate a property failure after corrupting production behavior or omitting an evaluation.

## Consequences

The trusted Rust compiler's constant evaluator executes the parser for these finite proofs.
Kani checks the resulting finite relation instead of symbolically executing the parser's loops, so its former parser unwinding and safety-property counts are not retained as evidence.
Compilation time belongs to the verification cost and must be reported beside backend time.
Native tests, Miri, and mutation checks continue to exercise ordinary execution of the production function.
This composition proves the existing finite contracts and makes no claim of equivalence for every possible input length or content.

## Alternatives

Continuing symbolic loop expansion did not finish within the existing budget.
A detached parser model would leave the production link unproved.
Increasing the budget or dropping inputs would change the required contract rather than discharge it.

## Reconsider

Reassess this composition when the pinned compiler, parser semantics, input domains, or required properties change.
Any broader contract needs its own complete implementation-bound argument.
