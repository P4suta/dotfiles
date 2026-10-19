---
name: concise-source
description: >-
  Express intent in types and executable behavior and keep source prose and lint exceptions minimal.
  Use for code comments, API documentation, lint suppressions, and unnecessary explanatory text.
---

# Concise source

Express behavior in names, domain types, exhaustive variants, checked constructors, and executable contracts instead of prose.
When a comment explains a missing invariant, enforce the invariant through `rust-invariants` or `systematic-fixes`, then decide whether the comment still earns its place.
Write no commentary about intentions, diligence, preferences, past fixes, or the conversation.
Delete redundant narration and commented-out code.

Keep text only for what the code leaves unsaid: a public usage contract, a legal notice, a required tool directive, a safety argument, or a consequential external constraint.
Give public API documentation enough detail, examples, and failure conditions to use the API correctly.
Record durable architectural tradeoffs in an ADR instead of repeating them at call sites.
Write comments that `prose check --channel comment` accepts.

Treat `#[allow(...)]`, file-wide suppression directives, ignored failures, and exclusions as prohibited by default.
Fix the representation or behavior behind a diagnostic instead of shrinking the checked surface.
Use `#[expect(..., reason = "...")]` only for a narrow, necessary exception that no redesign can remove without breaking the contract.
Tie the reason to the enforced invariant or external rule, and check unfulfilled lint expectations so an obsolete exception fails.

The lint attribute `#[expect]` differs from `Result::expect` and `Option::expect`.
Propagate or represent failures at input, I/O, configuration, cleanup, and concurrency boundaries.
Never use a panic in place of a recoverable error.
An assertion about an invariant needs that invariant enforced by construction and checked by tests.

Use `ocomment` for the repository's comment policy.
Use `oss-readme` for a project's public introduction and `adr` for consequential reasoning.
