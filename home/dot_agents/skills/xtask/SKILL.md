---
name: xtask
description: >-
  Build repository-specific validation and orchestration as typed, tested Rust xtask commands.
  Use for Cargo xtask design or maintenance, not app-domain code or quick shell glue.
---

# Rust xtask

Reuse the existing xtask crate and command conventions.
Keep a new crate unpublished and aligned with the workspace's edition, MSRV, lint policy, and locked dependencies.
Keep its CLI narrow.
Give it explicit typed subcommands and useful errors.
Use `rust-tooling`, `rust-invariants`, and `development-assurance` for implementation and verification.

Put deterministic repository checks and nontrivial developer orchestration here instead of in shell, Python, hooks, or copied CI snippets.
Separate planning, checks, and effects, and make supported transitions exhaustive.
Prefer semantic parsers and authoritative manifests to fragile regular expressions or text snapshots of code shape.
Keep external commands behind concrete testable capabilities, and preserve their exit status and diagnostic context.
Give a running xtask executable a build directory apart from the child checks that rebuild that package, because Windows locks an executing binary.

When the command guards a sensitive stage, use typed evidence for source, candidate, build, and release identities.
Keep candidate checks apart from signing and publication commands.
An assurance check must never publish as a side effect.
Emit structured phase evidence for long operations and partial failure without private inputs.
Own temporary resources, and test cleanup on error and interruption where supported.

Expose the command through the existing mise or just surface, and invoke the same code from CI and hooks.
Test the contract it enforces with realistic success and refusal cases.
