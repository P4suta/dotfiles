---
date: 2026-10-25
status: proposed
---
# Bind text dependency projection to native byte proofs

## Status

Proposed.

## Context

The dependency decision consumes a login and two optional classification strings.
Its policy depends on the login bytes, exact classification equality, and a reported bot flag.
The proof must cover every admitted login within its declared byte bound.
Eight text fixtures alone cannot establish that correspondence.

## Decision

Keep the executable decision in the production byte core.
Kani checks every byte sequence of length zero through sixteen and every combination of its three Boolean inputs.
Valid UTF-8 logins within the same byte bound form a subset of that domain.

Check the complete text projection before and after every campaign and CI partition.
Require its exact function, classification constants, native postcondition, input bound, arbitrary inputs, attributes, and coverage witnesses.
Reject added branches, changed conversions, shadowed bindings, assumptions, stubs, and changed inputs during verification.
Use the same check in the normal regression suite.

The conversion relies on Rust's specified [string byte representation and `as_bytes`](https://doc.rust-lang.org/std/primitive.str.html#method.as_bytes).
Classification relies on the standard [equality of `Option` values](https://doc.rust-lang.org/std/option/enum.Option.html#impl-PartialEq-for-Option%3CT%3E).
The checked projection substitutes those facts into the production byte decision without introducing another decision.
Keep the finite text harness and real GitHub regressions as boundary evidence.

## Consequences

The composed contract covers the original login byte domain without treating the text fixtures as exhaustive.
The native proof does not claim an unbounded login domain or verify the standard library and compiler themselves.
The source parser, token comparison, pinned verifier, and standard operations remain explicit trust boundaries.

A direct proof that reconstructs arbitrary strings also verifies the UTF-8 constructor and remained incomplete under the per-harness limit.
That result supplies no native verification credit or exception.
Increasing the limit supplies no implementation correspondence.
Restricting the login alphabet or length would discard admitted inputs.
Reassess the composition when the projection changes or a native verifier checks the complete text boundary.
