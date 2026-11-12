# Scoped verification adapters

The affected verification adapters run through `cargo test --locked -p dotfiles --test assurance -- MODE`.
The `mutations` mode checks configuration classification, coverage extensions, proof receipts, and captured evidence.
The `rule-mutations` mode checks the production verification rules.
Both modes select their test package explicitly and verify that every native test phase names that package and executes selected tests.
The native report must also pass the shared inventory, baseline, phase, and outcome checks.

Build the host image from `crates/dotfiles/tests/fixtures/verification-image/Dockerfile` and set `DOTFILES_VERIFICATION_IMAGE` to its immutable image identity.
The image pins Rust 1.99.1, cargo-mutants 27.1.0, cargo-fuzz 0.13.2, and the nightly compiler dated August 21, 2026.
It uses the host architecture for compilation and sanitizer runs.
The existing Kani runner keeps its separate AMD64 image.
Native execution avoids emulating the compiler without changing the selected mutations, tests, tool versions, or resource limits.
Each adapter container retains the ordinary two-CPU and two-gibibyte limits, with no swap and a 900-second stop limit.

The `fuzz` mode copies the committed corpus before exploring 10,000 inputs with seed 657 and an 8,192-byte input limit.
It checks the actual cargo-fuzz version and requires completion of the fixed input count under AddressSanitizer.
The optimized sanitizer build uses sixteen code generation units with debug assertions, overflow checks, and coverage instrumentation.
Compilation with one code generation unit exceeded the two-gibibyte limit and produced no successful result.
The compiler failure hid a later assumption that temporary corpus paths were relative.
The adapter removes the host project root before adding the container root and checks corpus visibility inside the container before compilation.
Receipt and phase inputs must round-trip through their canonical encodings.
The result carries the source and plan identities supplied by the request-bound runner.
Interrupted or incomplete campaigns supply no successful result.

Composite Stop tests check independent refusal conditions.
Resetting the answer baseline doesn't discharge an incomplete implementation contract from a previous request.
