---
name: xtask
description: >-
  Build repository-specific validation and orchestration as typed, tested Rust xtask commands.
  Use for Cargo xtask design or maintenance, not application-domain code or quick shell glue.
---

# Rust xtask

Reuse the existing xtask crate, Cargo alias, and command conventions.
For a new crate, keep it unpublished and aligned with the workspace's edition, MSRV, lint policy, and locked dependencies.
Keep its CLI narrow, with explicit typed subcommands and useful errors.
Use `rust-tooling`, `rust-invariants`, and `development-assurance` for implementation and verification.

Put deterministic repository checks and nontrivial developer orchestration here rather than in shell, Python, hooks, or copied CI snippets.
Separate planning, validation, and effects, and make supported transitions exhaustive.
Prefer semantic parsers and authoritative manifests to fragile regular expressions or text snapshots of code shape.
Keep external commands behind concrete testable capabilities and preserve their exit status and diagnostic context.
Keep a running xtask executable's build directory separate from the child checks that rebuild that package, including on Windows where an executing binary cannot be overwritten.

Use typed evidence for source, candidate, build, and release identities when the command guards a sensitive stage.
Keep candidate checks separate from signing and publication commands; an assurance check must not publish as a side effect.
Emit structured phase evidence for long operations and partial failure without private inputs.
Own temporary resources and test cleanup on error and interruption where supported.

Expose the command through the existing mise or just surface and invoke the same implementation from CI and hooks.
Test the contract it enforces with realistic success and refusal cases.
Keep a gate authoritative and focused rather than adding a new prose checklist or a second checker with different semantics.
