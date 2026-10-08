---
status: accepted
date: 2026-10-08
---
# Keep property fuzzing in the fuzz targets instead of bolero

## Context

Kani checks the archive entry check of the WezTerm setup and the attribution stripping of the Git guard only on short inputs.
While issues 75 and 84 add their harnesses, neither subject has a check at all.
Long entry names, composite `..` names, and large messages fall outside every check.
The pilot of issue 105 tried bolero, which runs one `check!()` harness as a property test under `cargo test` and as a coverage-guided campaign under `cargo-bolero`.
It wrote harnesses for both subjects that call a shared specification function.
No accepted archive entry resolves outside the destination by a lexical walk of its components.
`strip` stays idempotent, keeps every line but an attribution, and collapses only the blank lines at the end.

The pilot measured:

- bolero 0.13.7, the newest release, doesn't resolve in this workspace.
  Its derive crate 0.13.5 pins `indexmap` to `=2.11.2`.
  The `toml` 1.0 line of the workspace requires at least 2.11.4.
  Only bolero 0.13.4 resolves, two releases behind, and it adds 29 packages to `Cargo.lock`.
- `cargo-bolero` 0.13.5 builds and runs every test target of a package with coverage instrumentation.
  It fails to link, and then fails to load the test targets of the binaries of `dotguard` and `dotfiles`.
  The campaign ran only after invoking the library tests by hand with the same flags and `BOLERO_FUZZER=libfuzzer`.
- The archive entry check compiles only on Windows, where `cargo-bolero` has no engine, so its campaign ran bolero's random engine instead.
  That engine tried 682,075,574 inputs in 600 s and found no counterexample.
- The libFuzzer campaign on `strip` ran 41,226,470 inputs in 600 s on Linux and found no counterexample.
  Coverage grew from 116 to 205 edges over the unit-test seeds, but the count includes bolero's own string generator, so no share of it belongs to `strip`.
- Under `cargo test` each harness runs random inputs for 1 s on Windows and passes.
- Neither subject has a Kani harness in this checkout yet, so no campaign input can lie beyond a Kani bound.

## Decision

The repository doesn't adopt bolero.
The exit condition of the pilot required an input outside the Kani bound or a measurable coverage gain, and the campaigns showed neither.
Its newest release doesn't resolve here, and its campaign runner doesn't work on packages with binaries without a hand-written workaround.

Unbounded text that a Kani harness can't cover gets a cargo-fuzz target under `fuzz/` with a committed corpus, which `just proofs` already replays.
The exemptions that name this pilot as their reconsideration condition reconsider when such a target owns their function.

## Alternatives

Pinning bolero 0.13.4 and keeping the harnesses as advisory property tests would add a dependency tree behind the newest release for random inputs that found nothing.
The bolero Kani integration would let one harness serve both tools, but issue 105 keeps the Kani harnesses as plain `#[kani::proof]`, and the integration shares the dependency problem.

## Reconsider

Reconsider when a bolero release resolves with the workspace's `toml` and `cargo-bolero` can select the library tests of a package with binaries.
A subject's Kani harness must also exist for a campaign to exceed.
