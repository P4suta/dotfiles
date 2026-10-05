# Rust verification coverage

Decide what must run from the actual crate features, test targets, toolchain, and native boundary.
Keep ordinary formatting, compiler checks, and strict Clippy checks in the fast authoritative gate.
Use `formal-assurance` for implementation proofs, and never treat a dynamic check as a soundness proof.

For undefined behavior and unsafe contracts, add a pinned compatible nightly with [Miri](https://github.com/rust-lang/miri) and run the applicable production tests through it.
Include safe wrappers around unsafe dependencies when the project exercises their contract.
Require the identified tests to run, because a filter that drops every relevant test establishes no coverage.
Miri interprets particular executions and approximates the undefined-behavior rules, so a clean run proves no soundness for all callers or schedules.
Its foreign-function and platform support stays limited, so isolate supported decision logic and verify native adapters on their own.
Never turn off isolation or borrow checks to make a failing check pass.

When memory or race behavior crosses an interpreter's supported boundary, use [supported sanitizers](https://doc.rust-lang.org/unstable-book/compiler-flags/sanitizer.html) for instrumented native execution.
Check the exact sanitizer, target, dependencies, linking, and instrumentation coverage before claiming a result.
For relevant synchronization algorithms, use a concurrency model checker such as [Loom](https://docs.rs/loom/latest/loom/) with faithful primitives and explicit exploration bounds.
Combine these methods when they establish different properties.

Check locked dependencies for current vulnerability advisories through maintained [RustSec tooling](https://github.com/rustsec/rustsec).
Enforce dependency source, license, and version policy when the project's distribution or threat boundary requires it.
Treat advisory freshness and offline operation as distinct evidence, and never report cached data as a current audit.
For input-heavy APIs, maintain bounded property or fuzz checks with preserved seeds and minimized regressions.
Use `mutation-testing` to establish that important assertions detect representative broken behavior.

These mechanisms illustrate coverage and form no closed tool inventory.
Recheck upstream support, and assess a replacement by the same contract before adoption.
