# Project verification and optional execution receipts

Project-owned commands define required validation before completion, publication, and release.
Agents run the applicable checks, repair failures, and give a short confirmation naming successful results.
Managed hooks remind agents of that duty and check reply prose without requiring a parallel verification plan or an answer-evidence submission.
Issue scope, shared ADR content, signatures, push pauses, project hooks, and required PR checks keep their own gates.

The [explicit runner](../home/dot_agents/skills/development-assurance/references/verification-contract.md) remains available when the user requests source-bound execution receipts.
Initialize its request context with `dotfiles answer begin` before registering a plan.
The runner binds completed native results to the request, source, declared checks, tool identity, and execution order.
Its producer adapters reject empty, stale, skipped, incomplete, or inconclusive results.
The runner's classifications and receipts don't establish a test's semantic adequacy or replace project gates.

Explicit completion checks share the session's OS lock.
Explicit attempts take an exclusive lock.
Routine agent edits and replies don't take that lock or read another checkout's verification state.
The [Rust file-lock contract](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock_shared) remains an external semantic boundary.
`cargo test --locked -p dotfiles --test verification` exercises held readers, a held writer, release, and cross-process reads.
Native lock coverage assumes cooperating processes on a filesystem implementing Rust's contract.
`cargo test --locked -p platform --lib issue_context::tests` checks existing report paths and traversal refusals.
Reassess the boundary when the lock API or filesystem semantics change.

The project retains native Git, hook, collector, and profile regressions, pinned tools, required CI aggregation, and scoped installation checks.
Native profile rehearsals compare installed files while preserving unrelated settings and runtime state.
Both manifest families use their own native generator.
Heavy proof and mutation campaigns remain at their project-defined stage.
Publication, signing, and approval keep their separate authorization boundaries.

Earlier acceptance criteria came from [dotfiles-mac issue 36](https://github.com/P4suta/dotfiles-mac/issues/36), [dotfiles-private issue 59](https://github.com/P4suta/dotfiles-private/issues/59), and [dotfiles-win issue 36](https://github.com/P4suta/dotfiles-win/issues/36).
Current source checks and actual command results determine the short confirmation.
