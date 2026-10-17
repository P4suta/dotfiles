---
status: accepted
date: 2026-10-08
---
# Keep bounded harnesses instead of Kani loop contracts for the Git argument scan

## Context

The Git guard reads the global options before the subcommand in one `while` loop in `crates/dotguard/src/gitargv.rs`, and the gate reads each `-c` or `--config-env` setting there.
A Kani harness of that loop covers only a bounded alphabet and length, so a policy override after many global options lies outside the proof.
The pilot of issue 103 tried Kani's experimental loop contracts, which replace a loop by its invariant and decreasing measure so no unwinding bound applies.
The manifest names `gate_rules::spelling` as the pilot's subject, the long-option scan that shares the loop's shape.

The pilot ran Kani 0.68.0 with `CBMC` 6.11.0 from the pinned proof image, with `-Z loop-contracts`:

- `#[cfg_attr(kani, kani::loop_invariant(...))]` and `kani::loop_decreases` on a `while` statement compile on the stable toolchain when the crate root enables `stmt_expr_attributes` and `proc_macro_hygiene` only under `cfg(kani)`.
- On the loop as written, which clones each setting into a `Vec<(String, Range<usize>)>`, the host killed `goto-instrument` for lack of memory after 4 min 21 s without a cap.
  Under a 10 GB cap it killed the process after 6 min 46 s, with the `format!` call reduced to plain string pushes.
- A copy with the collection removed, which keeps only the index and a count over a `&[&str]` of up to 64 words, didn't finish within 75 minutes.
  The gate's budget gives each harness 30 s.
- The invariant has to admit an index one past the end, because a value-taking option as the last word steps over it, which the production code then clamps with `min`.
- Stating that every policy setting before the subcommand reaches `config_override` for any length needs a quantified invariant over the words already read, which `-Z quantifiers` would add on top of the loop contract.

## Decision

The repository doesn't adopt Kani loop contracts.
The Git argument scan keeps the bounded harness that issue 84 adds, and the residue beyond its bound stays a `disproportionate` exemption beside the loop, with this record as its evidence.

## Alternatives

Rewriting the scan to index a fixed-size array would make the contract tractable, but it would prove a different loop than the one the guard runs.
Raising the proof budget for one harness would hide the cost from the gate that every other harness meets.

## Reconsider

Reconsider when a Kani release instruments a loop contract over a body that pushes to a `Vec` within the proof budget.
Reconsider also when the guard's scan becomes a loop over indices.
