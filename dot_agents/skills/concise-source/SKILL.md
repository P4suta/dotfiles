---
name: concise-source
description: >-
  Express intent in types and executable behavior and keep source prose and lint exceptions minimal.
  Use for code comments, API documentation, lint suppressions, and unnecessary explanatory text.
---

# Concise Source

Prefer names, domain types, exhaustive variants, checked constructors, and executable contracts to prose describing how code should behave.
If a comment explains a missing invariant, implement that invariant through `rust-invariants` or `systematic-fixes` before deciding whether the comment is still necessary.
Do not add commentary about intentions, diligence, personal preferences, past fixes, or the conversation.
Remove redundant narration and commented-out code rather than preserving a development diary.

Keep text only when a reader needs information the implementation cannot express: a public usage contract, legal notice, required tool directive, safety argument, or a consequential external constraint.
Keep public API documentation sufficient to use the API correctly, with examples or failure conditions where necessary.
An unsafe operation still needs its actual safety argument; hiding it to obtain a clean comment report is not a correction.
Put durable architectural tradeoffs in an ADR instead of repeating them at every call site.
Write comments that `prose check --channel comment` accepts; the checker defines the language, style, and line rules.

Treat `#[allow(...)]`, file-wide disable directives, ignored failures, and exclusions as prohibited by default.
Fix the representation or behavior that causes the diagnostic rather than reducing the checking surface.
Use `#[expect(..., reason = "...")]` only for a narrowly scoped, demonstrably necessary exception when redesign cannot remove the diagnostic without violating the contract.
Keep the reason specific to the enforced invariant or external requirement, and check unfulfilled lint expectations so an obsolete exception fails.
Do not suppress a new warning merely to preserve the old implementation or finish sooner.

Rust's lint `#[expect]` is different from `Result::expect` and `Option::expect`.
Propagate or represent failures at input, I/O, configuration, cleanup, and concurrent boundaries.
Do not use a panic to replace a recoverable error or call a fallible operation infallible because its usual input works.
A narrowly justified assertion about an invariant must already be enforced by construction and checked by the relevant tests.

Use `ocomment` for the repository's actual comment policy and preserve necessary legal, safety, documentation, and language directives.
Use `oss-readme` for a project's public introduction and `adr` for consequential reasoning.
Passing a prose checker does not establish the behavior or soundness of the code.
