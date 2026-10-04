---
name: type-contracts
description: >-
  Require checked type contracts and validated external inputs in every implementation language, including mandatory annotations in dynamic languages.
  Use for implementation, API changes, type-checker setup, or closing unchecked code paths.
---

# Checked Type Contracts

Express domain invariants and allowed transitions through the language's strongest practical checked contracts.
For dynamic languages, require explicit parameter, return, and public state annotations using a supported type system and a strict checker.
Use inference for clear local expressions; redundant annotations are not a substitute for checking behavior.
Use `rust-invariants` for Rust boundaries and `verification-tools` to select current supported checking tools.

## Cover the real implementation

Read the language version, package configuration, entry points, tests, generated code, and imports before configuring the checker.
Check all owned production and test code, including previously unannotated functions and code reached through imports.
Require annotations at function and public data boundaries, and enforce their presence with an appropriate lint when the checker permits missing annotations.
Read [dynamic language coverage](references/dynamic-languages.md) when configuring Python or JavaScript checking.
Choose maintained equivalents for other languages from their official documentation rather than inferring support from a familiar tool name.

Do not use unrestricted dynamic types, unchecked casts, blanket ignores, missing-import suppression, or broad exclusions to satisfy the gate.
Represent uncertain input as an explicit unknown value and narrow it through a checked predicate or validated constructor.
Keep an unavoidable interop escape narrow, justified, and covered by a boundary test; unused suppressions must fail.
Generated or third-party code needs a defined trust boundary, generation or stub checks, and checked owned adapters.
For a language without useful annotation support, keep glue thin and put procedural decisions in a maintained typed tool.

Model distinct states and failure variants explicitly.
Prefer exhaustive variants, validated identifiers and units, ownership, immutable values, and total transformations when they eliminate a relevant bug family.
Avoid adding an abstraction that does not protect an actual invariant.

## Validate at runtime boundaries

Annotations do not validate JSON, environment variables, configuration, files, database rows, or remote responses.
Validate their shape, ranges, encoding, invariants, and version before constructing trusted domain values.
Preserve meaningful typed errors and reject invalid transitions instead of coercing malformed input into a default.
Keep serialization and schema compatibility checked against actual consumers.

Use `executable-policy` to make type checking and annotation requirements authoritative in local commands, hooks, and required CI.
Verify that a missing annotation and a representative type error are rejected by the real gate.
When strengthening an existing project, prevent new unchecked code immediately and close the remaining scoped gaps within the authorized work.
Keep any migration inventory explicit; a baseline of old errors must not silently exempt newly added or modified code.
