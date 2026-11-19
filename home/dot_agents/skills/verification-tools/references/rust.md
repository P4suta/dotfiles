# Rust verification coverage

Decide what must run from the actual crate features, test targets, toolchain, and native boundary.
Keep ordinary formatting, compiler checks, and strict Clippy checks in the fast authoritative gate.
Use `formal-assurance` for implementation proofs, and never treat a dynamic check as a soundness proof.

Clippy's [`disallowed_types`](https://clippy.rust-lang.org/stable/index.html#disallowed_types) checks configured type paths, including paths in macro expansions.
It doesn't reject a call merely because its inferred return type contains `Box` or `dyn`.
[`ScopedJoinHandle::join`](https://doc.rust-lang.org/std/thread/struct.ScopedJoinHandle.html#method.join) returns [`std::thread::Result`](https://doc.rust-lang.org/std/thread/type.Result.html), whose error type is `Box<dyn Any + Send + 'static>`.
A resolved-type lint that inspects the return type can refuse that call.
Sending each thread's result through [`std::sync::mpsc`](https://doc.rust-lang.org/std/sync/mpsc/index.html) avoids that type, and [`std::thread::scope`](https://doc.rust-lang.org/std/thread/fn.scope.html) still joins the threads and panics if one of them panicked.
With the [Rust toolchain pinned for the 2026-11-19 check](https://github.com/P4suta/dotfiles/blob/f5aee8b7aa1b35797196b6fa8c038d9956f78a58/mise.toml#L2), [`vec![a, b]`](https://doc.rust-lang.org/std/macro.vec.html) expanded through `Box` and tripped a `disallowed-types` entry for `Box`, while [`[a, b].to_vec()`](https://doc.rust-lang.org/std/primitive.slice.html#method.to_vec) passed.
The slice conversion requires [`Clone`](https://doc.rust-lang.org/std/clone/trait.Clone.html).
[`Vec::from([a, b])`](https://doc.rust-lang.org/std/vec/struct.Vec.html#impl-From%3C%5BT;+N%5D%3E-for-Vec%3CT%3E) moves non-`Clone` elements and passed the same Clippy configuration on that date.
The standard library doesn't document that expansion, so recheck it after a toolchain update before a gate decision depends on it.

For undefined behavior and unsafe contracts, add a pinned compatible nightly with [Miri](https://github.com/rust-lang/miri) and run the applicable production tests through it.
Include safe wrappers around unsafe dependencies when the project exercises their contract.
Require the identified tests to run, because a filter that drops every relevant test establishes no coverage.
Miri interprets particular executions and approximates the undefined-behavior rules, so a clean run proves no soundness for all callers or schedules.
Its foreign-function and platform support stays limited, so isolate supported decision logic and verify native adapters on their own.
Never turn off isolation or borrow checks to make a failing check pass.

When memory or race behavior crosses an interpreter's supported boundary, use [supported sanitizers](https://doc.rust-lang.org/unstable-book/compiler-flags/sanitizer.html) for instrumented native execution.
Check the exact sanitizer, target, dependencies, linking, and instrumentation coverage before claiming a result.
For synchronization algorithms, use a concurrency model checker such as [Loom](https://docs.rs/loom/latest/loom/) with faithful primitives and explicit exploration bounds.

Check locked dependencies with maintained [RustSec tooling](https://github.com/rustsec/rustsec) and an immutable advisory snapshot.
Record its revision or digest so every contributor checks the same input.
Enforce dependency source, license, and version policy when the project's distribution or threat boundary requires it.
Treat current advisory freshness as a separate observation, and never report a cached snapshot as a current audit.
For input-heavy APIs, maintain bounded property or fuzz checks with preserved seeds and minimized regressions.
Use `mutation-testing` to establish that important assertions detect representative broken behavior.
