---
name: rust-tooling
description: >-
  Build automation, validation, and developer helpers as maintained Rust tools instead of ad hoc shell, Python, or JavaScript scripts.
  Use when adding procedural tooling, not when invoking an existing command-line tool.
---

# Rust tooling

Write new procedural automation in Rust, including one-off helpers.
Prefer an existing Rust command or repository `xtask` before creating another executable.
Keep mise, just, hooks, and CI recipes as thin invocations of those commands.
A direct call to a maintained tool needs no Rust, but parsing, loops, state changes, retries, and business rules belong in typed Rust.
Never leave a shell or Python prototype behind as a second implementation.

Give the tool a scoped Cargo manifest, committed lockfile, explicit arguments, and accurate help.
Keep private helpers unpublished, and choose dependencies that fit the supported platforms and oldest supported Rust version.
Use `rust-invariants`, `development-assurance`, and `systematic-fixes` for the important boundaries.
Separate pure planning and checks from effects, and model permitted operations with exhaustive variants.
Read configuration and the process environment at the composition root, and pass concrete capabilities inward.

For destructive or remote mutations, offer a concrete plan, check identity and scope before applying, read back the result, and record partial failure with structured events.
Keep dry-run paths free of mutation, and use fresh report paths.
Never put private values in arguments, logs, fixtures, or the normal output stream.
Keep ownership-based cleanup and causal errors instead of success-shaped fallbacks.

Test observable contracts and failed transitions before adding the behavior.
Exercise the real external boundary for assumptions a fake leaves unproven.
Wire the helper into the existing project gates and dependency updates.
An untested or unmaintained Rust file, or one that duplicates a trusted project tool, adds nothing.
