---
status: accepted
date: 2026-10-08
---
# Drop Aeneas extraction for the plan and ordering cores and try Verus next

## Context

The plan theorem of `skill_ops::dependency_plan` and the ordering theorem of `order_rules::order` should hold of the Rust code, while Kani ties a transcription to it only for small graphs.
The pilot of issue 102 tried Charon and Aeneas, which translate Rust to Lean so a theorem speaks of the extracted function itself.
It built a pilot image from the Rust 1.99.0 base the proof image pins.
The image holds the Aeneas nightly `nightly-2026.10.06-557eff8` by checksum, the Charon commit its `charon-pin` names built on `nightly-2026-09-17`, and Lean 4.31.0 with Mathlib from the prebuilt archive.
The Lean package of issue 83, which holds the theorems to restate, hasn't landed.

The pilot measured:

- Charon reads both subjects in about 30 s, and the `LLBC` of two runs differs only in the destination path it records, so the same destination gives byte-identical output.
- Aeneas can't translate either core as written.
  `order` scans with the slice pattern `[needs, rest @ ..]`, which stops Aeneas with an inconsistent projection of the slice's metadata.
  `dependency_plan` leaves an outer loop with `ensure!`, which Aeneas refuses as a break to an outer loop, and `visit` returns early from inside its loop, which Aeneas doesn't support yet.
- Rewriting `order` to scan by index with a flag instead of an early return passes its unit tests and comes out as Lean that stays byte-identical across two runs.
  The extracted file type-checks with the Aeneas library in 9 s.
- The extracted loops become `loop` over a body returning `ControlFlow`, so each theorem needs a measure and an invariant for every loop through `loop.spec_decr_nat`.
  The shifts carry a precondition that the step count stays below 32.
- No theorem closed, because the theorems of issue 83 don't exist yet and the dependency plan has no index core to state them about.

## Decision

The repository doesn't adopt the extraction.
The exit condition required both cores extracted and both theorems closed, and neither core extracts as written.
Extraction would mean rewriting production loops into the shapes Aeneas accepts, and keeping a second Lean toolchain with Mathlib beside the gate's Mathlib-free package.

Verus comes next, because it proves the Rust source in place, early returns and slice patterns included, with no second toolchain to keep in step.
The bounded Kani bridges stay until a candidate closes both theorems.

## Alternatives

Rewriting both cores for Aeneas now would change the code the gate already proves to suit a tool that hasn't yet proved a theorem about them.
Keeping the pilot running without a decision would leave the manifest with an advisory pilot that discharges nothing.

## Reconsider

Reconsider Aeneas when it supports early returns and breaks inside loops, and the Lean package of issue 83 holds both theorems to restate.
