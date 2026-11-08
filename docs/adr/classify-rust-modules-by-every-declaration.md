---
date: 2026-11-06
status: proposed
---

# Classify Rust modules by every declaration

## Context

File-only comparison treats private helpers in an outlined Kani module as production.
Rust loads [outlined modules](https://doc.rust-lang.org/reference/items/modules.html#module-source-filenames) through declarations whose [configuration attributes](https://doc.rust-lang.org/reference/conditional-compilation.html#the-cfg-attribute) control compilation.

## Decision

Exclude a module only when every declaration requires `test` or `kani` and at least one declaration exists.
Resolve conventional module paths and literal path attributes from that same snapshot.
Treat [Cargo targets](https://doc.rust-lang.org/cargo/reference/cargo-targets.html) and [build scripts](https://doc.rust-lang.org/cargo/reference/manifest.html#the-build-field) as production.
Keep production classification for ambiguous ownership, unresolved local production sources, and ordinary aliases.
Place source relevance in the pure rule core.
Treat unresolved expansion in the target Cargo package as unknown and inspect literal aliases from other packages.
External macros remain compiler-checked.
Bind proof bodies and helpers to the verification source.

## Alternatives

File-only comparison rejects valid outlined verification modules.
Ignoring proof filenames loses ordinary production aliases.
Compiler expansion can resolve more syntax but requires each project's complete target environment.

## Consequences

The checker trusts the Rust parser.
Unresolved compilation contexts remain conservative.

## Confirmation

Tests isolate guards, paths, inclusion, and ownership.
Proofs check ownership and source relevance, while fuzzing checks generated catalogs.
Native mutation inventories must cover every changed function and visitor method.
Reconsider expansion for unresolved module syntax.
