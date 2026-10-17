---
name: type-contracts
description: >-
  Require checked type contracts and validated external inputs in every implementation language, including mandatory annotations in dynamic languages.
  Use for implementation, API changes, type-checker setup, or closing unchecked code paths.
---

# Checked type contracts

Express domain invariants and permitted transitions through the language's strongest practical checked contracts.
For dynamic languages, require explicit parameter, return, and public state annotations in a supported type system with a strict checker.
Use inference for clear local expressions, because redundant annotations check no behavior.
Use `rust-invariants` for Rust boundaries and `verification-tools` to choose current supported checking tools.

## Cover the real implementation

Read the language version, package configuration, entry points, tests, generated code, and imports before configuring the checker.
Check all owned production and test code, and include unannotated functions and code reached through imports.
Require annotations at function and public data boundaries.
When the checker permits missing annotations, enforce their presence with a lint.
Read [dynamic language coverage](references/dynamic-languages.md) when configuring Python or JavaScript checking.
For other languages, choose maintained tools from official documentation instead of inferring support from a familiar tool name.

Never pass the gate with unrestricted dynamic types, unchecked casts, blanket ignores, missing-import suppression, or broad exclusions.
Represent uncertain input as an explicit unknown value, and narrow it through a checked predicate or validated constructor.
Keep an unavoidable interop escape narrow, justified, and covered by a boundary test, and make unused suppressions fail.
Generated or third-party code needs a defined trust boundary, generation or stub checks, and checked owned adapters.
For a language without useful annotation support, keep glue thin and put procedural decisions in a maintained typed tool.

Model distinct states and failure variants explicitly.
Prefer exhaustive variants, validated identifiers and units, ownership, immutable values, and total transformations when they remove a relevant bug family.
Add no abstraction that protects no actual invariant.

## Validate at runtime boundaries

Annotations don't validate JSON, environment variables, configuration, files, database rows, or remote responses.
Validate their shape, ranges, encoding, invariants, and version before constructing trusted domain values.
Preserve meaningful typed errors, and reject invalid transitions instead of coercing malformed input into a default.
Check serialization and schema compatibility with actual consumers.

Use `executable-policy` to make type checking and annotation rules authoritative in local commands, hooks, and required CI.
Verify that the real gate rejects a missing annotation and a representative type error.
When strengthening an existing project, block new unchecked code at once, and close the remaining scoped gaps within the authorized work.
A baseline of old errors never exempts new or changed code.
