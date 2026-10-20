---
name: functional-architecture
description: >-
  Place decisions in a pure core behind ports and effects in thin adapters, and name the domain types the core returns.
  Use with formal-assurance for implementation, when choosing where a boundary goes, or when a structural gate refuses core code.
---

# Functional core behind ports

The gates decide what a core may contain.
This skill covers the judgment they can't make.

## Where the boundary goes

A decision belongs in the core when its result depends only on values: what to run, whether to refuse, which state comes next.
Reading a file, the clock, the environment, a process, or the network belongs in an adapter that turns the result into a value before it calls the core.
When a decision needs a fact mid-way, split it into two pure steps around the adapter's read.
Otherwise define a port in the core, and let the adapter provide it.
Keep the adapter thin enough to show at a glance that it only gathers facts and applies the decision.

## Domain types

Name a type for each value that carries meaning beyond its representation, and validate it in its constructor.
Return a named type instead of bare text, and let the adapter render it through `Display` or an accessor.
Express absence and failure as `Option` or `Result` with a named reason, never as a panic, a sentinel, or a magic value.
Return a new state instead of mutating shared state.

## Gates

- In Rust, the core lives in a crate without dependencies or the standard library that denies the panic lints, so the compiler refuses an effect or a panic.
- The project's pinned structural checker refuses an effect, bare text, a panic, or shared state in core paths, and each finding's note names the fix.
- Other languages enforce the dependency direction with their own checker: dependency-cruiser, import-linter, depguard, or ArchUnit.
- The proofs and property tests that `formal-assurance` requires target each pure core function first.

Keep owner-side tooling within the boundary in `formal-assurance`.
