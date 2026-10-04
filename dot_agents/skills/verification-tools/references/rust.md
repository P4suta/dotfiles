# Rust Verification Coverage

Use the actual crate features, test targets, toolchain, and native boundary when deciding what must run.
Keep ordinary formatting, compiler checks, and strict Clippy checks in the fast authoritative gate.
Use `formal-assurance` for implementation proofs rather than treating a dynamic check as a proof of soundness.

For undefined behavior and unsafe contracts, introduce a pinned compatible nightly with [Miri](https://github.com/rust-lang/miri) and execute the applicable production tests through it.
Include safe wrappers around unsafe dependencies when their contract is exercised by the project.
Require identified tests to execute; filtering every relevant test away does not establish coverage.
Miri interprets particular executions and approximates Rust's undefined-behavior rules; a clean run does not prove soundness for all callers or schedules.
Its foreign-function and platform support is limited, so isolate supported decision logic and verify native adapters separately.
Do not disable isolation or borrow checks merely to make a failing check pass.

Use [supported sanitizers](https://doc.rust-lang.org/unstable-book/compiler-flags/sanitizer.html) for instrumented native execution when memory or race behavior crosses an interpreter's supported boundary.
Check the exact sanitizer, target, dependencies, linking, and instrumentation coverage before claiming a result.
Use a concurrency model checker such as [Loom](https://docs.rs/loom/latest/loom/) for relevant synchronization algorithms, with faithful primitives and explicit exploration bounds.
Combine these methods when they establish different guarantees.

Check locked dependencies against current vulnerability advisories through maintained [RustSec tooling](https://github.com/rustsec/rustsec).
Enforce dependency source, license, and version policy when the project's distribution or threat boundary requires it.
Treat advisory freshness and offline operation as distinct evidence rather than reporting cached data as a current audit.
For input-heavy APIs, maintain bounded property or fuzz checks with preserved seeds and minimized regressions.
Use `mutation-testing` to establish that important assertions detect representative broken behavior.

These are examples of coverage mechanisms, not a closed tool inventory.
Recheck upstream support and evaluate a replacement against the same contract before adoption.
