---
status: proposed
date: 2026-10-13
---
# Retain Miri sysroots with fresh interpretation

## Context

An isolated Code environment removes Miri's default standard-library cache with its private home.
Retaining the complete Miri target would also preserve launch records containing a previous checkout and environment.
An earlier complete Code result does not identify enough individual gate inputs to authorize a new complete result after selected checks alone.

## Decision

Retain only Miri standard-library artifacts in the existing host build cache.
Separate namespaces by the pinned nightly, actual compiler and Miri version reports, resolved target, and project Cargo configuration.
Custom compiler, Miri, sysroot, target, or flag overrides use the original uncached path.
Hold an exclusive namespace lease through preparation and interpretation.
Run the pinned `cargo miri setup` before every use of the retained sysroot and refuse unsuccessful preparation.
This setup command validates the upstream standard-library source and configuration fingerprint even with an explicit `MIRI_SYSROOT`.

Keep launch targets within the disposable Code home.
Every campaign still discovers the current test inventory, executes every required batch, validates complete results and budgets, and runs the undefined-behavior fixture.
Measure each batch with the collector's host monotonic clock and retain its elapsed milliseconds, rounded upward, with the selected tests and raw result.
Require that measurement during both collection and retained-report validation against the existing 30-second budget.
The measured Cargo invocation includes compilation and startup, so it is a conservative upper bound on interpreted execution time.
The interpreted libtest JSON clock remains a protocol diagnostic because Miri replaces clocks under default isolation.
Missing native timing is refused, and the full batch inventory, ignore states, isolation, and undefined-behavior fixture remain required.
Partition exhaustive byte and casing tests into disjoint named shards without removing any original input or assertion.
Run each such shard separately and keep ordinary tests grouped, so their combined execution does not conceal an over-budget workload.
No log, digest, or earlier success replaces execution or authorizes a complete Code note.

The owner-managed cache and pinned compiler installation remain external trust boundaries.
The namespace digest separates inputs; it does not authenticate an adversary rewriting either storage.
The setup behavior is delegated to [cargo-miri](https://github.com/rust-lang/miri/blob/master/cargo-miri/src/setup.rs) and its [sysroot builder](https://github.com/RalfJung/rustc-build-sysroot/blob/master/src/lib.rs).
Fault-injection tests cover preparation failure and the lease, while existing Miri result checks remain mandatory.
Actual pinned native compatibility and cold-versus-warm measurements remain pending.

## Alternatives

Retaining the complete Miri target would preserve launch records from an earlier checkout and environment.
Reusing an earlier complete Code result after selected checks would lack the current gate input identities needed to authorize that result.
Keeping sysroots only inside the isolated Code home would discard them with that disposable home.

## Consequences

Repeated Code runs can keep standard-library compilation without carrying old launch environments into a new candidate.
Changed checkout paths can still rebuild crate artifacts.
No execution-time reduction has been measured for this change.
Profile-specific complete Code reuse and the performance targets in [issue 153](https://github.com/P4suta/dotfiles/issues/153) remain pending.
Reconsider broader reuse after a producer binds every required gate to authentic source and tool identities.
