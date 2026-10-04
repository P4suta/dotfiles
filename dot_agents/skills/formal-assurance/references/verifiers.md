# Verifier-specific Checks

## Kani

Isolate the pure decision core, quantify valid and invalid inputs, and assert the actual invariant and failure behavior.
Make assumptions, loop bounds, abstractions and stubs explicit and check that the admitted input space is nonempty and representative.
Require successful unwinding assertions or another justified completeness argument for the stated scope; a truncated loop is not evidence about arbitrary executions.
Treat timeout, unsupported behavior, skipped harnesses and solver failure as missing evidence, never a passing proof.
Require execution of every expected harness and reachability of the asserted contract.
Use cover properties or concrete witnesses to detect contradictory assumptions and unreachable assertions.
Turn counterexamples into deterministic regressions and repair the common cause.
Verify the installed toolchain and supported features against [Kani's documentation](https://model-checking.github.io/kani/) before choosing a harness strategy.

## Lean 4

State the model, preconditions, transition relation and theorem corresponding to the promised guarantee.
Use the kernel-checked proof and inspect its axioms and dependencies.
Reject `sorry`, `admit`, an unproved project axiom, unsafe evaluation or an oracle as a substitute for a required theorem.
Distinguish standard foundational assumptions from newly introduced claims and reject vacuous theorems or assumptions that assert the conclusion.
Check refinement between the model's domains and the implementation's finite integers, bytes, errors and external boundaries.
Prove or mechanically validate the translation and its stated scope rather than claiming that a separately rewritten algorithm proves the implementation.
Run the pinned `lake build` or proof check in CI and keep the theorem tied to the current contract.
Use [Lean's official reference material](https://lean-lang.org/learn/) for current commands and semantics.

## Independent executable references

An independent Haskell or other executable reference supplements applicable proofs.
Implement from the authoritative specification without sharing the production algorithm, normalization mistakes or generated expected output.
Specify observable output and compare it byte-for-byte or through a justified canonical representation.
Cover domain variants with specification-derived fixtures, generated cases and malformed inputs.
Force evaluation so laziness, exceptions and nontermination cannot masquerade as agreement.
Use external specifications and independent known-answer fixtures to catch shared mistakes.
For Haskell, read [the official documentation](https://www.haskell.org/documentation/) and the pinned compiler and package configuration.
