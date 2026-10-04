---
name: rust-tooling
description: >-
  Implement automation, validation, and developer helpers as maintained Rust tools instead of ad hoc shell, Python, or JavaScript scripts.
  Use when adding procedural tooling, not when merely invoking an existing CLI.
---

# Rust Tooling

Use Rust for new procedural automation, including one-off helpers that would otherwise become quick scripts.
Prefer an existing Rust command or repository `xtask` before creating another executable.
Keep mise, just, hooks, and CI recipes as thin invocations of those commands.
A direct invocation of a maintained tool is appropriate; parsing, loops, state changes, retries, and business rules belong in typed Rust.
Do not prototype the real logic in shell or Python and leave that prototype as a second implementation.

Give the tool a scoped Cargo manifest, committed lockfile, explicit arguments, and accurate help.
Keep private helpers unpublished and choose dependencies that fit the supported platforms and MSRV.
Use `rust-invariants`, `development-assurance`, and `systematic-fixes` for the important boundaries.
Separate pure planning and validation from effects, and model allowed operations with exhaustive variants.
Read configuration and the process environment at the composition root and pass concrete capabilities inward.

For destructive or remote mutations, offer a concrete plan, validate identity and scope before applying, read back the result, and record partial failure with structured events.
Keep dry-run paths free of mutation and use fresh report paths.
Never put private values in arguments, logs, fixtures, or the normal output stream.
Keep ownership-based cleanup and causal errors instead of success-shaped fallbacks.

Test observable contracts and failed transitions before adding the behavior.
Exercise the real external boundary for assumptions a fake cannot prove.
Wire the helper into the existing project gates and dependency updates.
A Rust source file alone is not an improvement if it remains untested, unmaintained, or duplicates a tool the project already trusts.
