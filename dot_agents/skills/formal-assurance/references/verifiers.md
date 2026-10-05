# Verifier-specific checks

## Kani

Isolate the pure decision core, quantify valid and invalid inputs, and assert the actual invariant and failure behavior.
Make assumptions, loop bounds, abstractions, and stubs explicit, and keep the admitted input space nonempty.
Require successful unwinding assertions or another justified completeness argument.
Require every expected harness to run and every asserted contract to stay reachable.
Use cover properties to detect contradictory assumptions and unreachable assertions.
Turn counterexamples into deterministic regressions.
Check the installed toolchain and features in [Kani's documentation](https://model-checking.github.io/kani/).
Kani lacks a native Windows build, so run the same pinned gate on a Linux host, such as a pinned container over a read-only view of the checkout.

## Lean 4

State the model, preconditions, transition relation, and theorem for the promised property.
Inspect the axioms and dependencies of the kernel-checked proof.
Reject `sorry`, `admit`, an unproved project axiom, unsafe evaluation, or an oracle in a required theorem.
Reject vacuous theorems and assumptions that assert the conclusion.
Check refinement between the model's domains and the implementation's finite integers, bytes, errors, and external boundaries.
Run the pinned `lake build` or proof check in CI.
Use [Lean's official reference material](https://lean-lang.org/learn/) for current commands.

## Independent executable references

Write an independent Haskell or other executable reference from the authoritative specification without sharing the production algorithm or generated expected output.
Compare observable output byte for byte or through a justified canonical form.
Cover domain variants with specification-derived fixtures, generated cases, and malformed inputs.
Force evaluation so laziness, exceptions, and nontermination never pass as agreement.
For Haskell, read [the official documentation](https://www.haskell.org/documentation/) and the pinned compiler and package configuration.
