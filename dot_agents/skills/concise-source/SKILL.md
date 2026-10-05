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
An unsafe operation keeps its safety argument.
Record durable architectural tradeoffs in an Architecture Decision Record (ADR) instead of repeating them at call sites.
Write comments that `prose check --channel comment` accepts.

Treat `#[allow(...)]`, file-wide suppression directives, ignored failures, and exclusions as prohibited by default.
Fix the representation or behavior behind a diagnostic instead of shrinking the checked surface.
Use `#[expect(..., reason = "...")]` only for a narrow, necessary exception that no redesign can remove without breaking the contract.
Tie the reason to the enforced invariant or external rule, and check unfulfilled lint expectations so an obsolete exception fails.
Never suppress a new warning to keep the old implementation or to finish sooner.

The lint attribute `#[expect]` differs from `Result::expect` and `Option::expect`.
Propagate or represent failures at input, I/O, configuration, cleanup, and concurrency boundaries.
Never use a panic in place of a recoverable error, or call a fallible operation infallible because its usual input works.
An assertion about an invariant needs that invariant enforced by construction and checked by tests.

Use `ocomment` for the repository's comment policy, and keep necessary legal, safety, documentation, and language directives.
Use `oss-readme` for a project's public introduction and `adr` for consequential reasoning.
