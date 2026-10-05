---
name: rust-invariants
description: >-
  Design Rust correctness boundaries with types, exhaustive enums, ownership, and validated state transitions.
  Use for important domain invariants, APIs, and bug-family prevention, not to add abstraction to a trivial edit.
---

# Rust invariants

Start from the observable property callers need and the states that would violate it.
Choose the smallest representation whose supported API makes those invalid combinations unconstructible.
Treat library consumers, maintainers, operators, and end users as users of the design.
Choose a coherent default from their needs instead of asking the owner to settle routine implementation choices.

Use domain newtypes to separate interchangeable primitives, exhaustive enums for distinct cases, and private fields with checked constructors for validated inputs.
Represent mutually exclusive states with enum variants instead of independent booleans or unrelated optional fields.
Use ownership to express who controls resources and when code can consume or release them.
Use state-specific types when a later operation needs evidence from an earlier transition.
Check at the boundary, and pass the checked value into the core instead of rechecking raw strings or identifiers.

Make critical transitions exhaustive and explicit.
Avoid wildcard matches that hide an unhandled domain variant, sentinel values, stringly typed commands, and success-shaped defaults for unknown states.
Bind a completed proof or verification to the exact identity, revision, bytes, or capability it checked.
A boolean named `verified` proves nothing when its subject can change on its own.
Separate pure decisions from I/O, and inject the concrete capabilities a test must drive instead of mutable global test switches.

State exactly what the compiler enforces and what still depends on runtime checks.
Untrusted files, network responses, signatures, concurrency, and external services stay boundary conditions.
The type system alone proves no business rule, cryptographic property, or remote transaction.
When the critical algorithm needs stronger evidence, use an explicit argument, property, model, or formal method.
Use `formal-assurance` for executable Rust verification, kernel-checked models, and independent reference implementations.
Record that argument in the relevant architecture decision record and executable tests, and never call an ordinary unit test a proof.

Test the public contract, refused states, ownership and failure behavior, and any compatibility promise.
Use compile-fail tests when an API must reject invalid construction or an illegal transition at compile time.
Use property or model tests for algebraic laws and transition sequences that examples miss.
Refactor the affected problem family, not only the observed call site, and keep unrelated behavior intact.
Use `concise-source` when a comment or lint exception compensates for an invariant the representation should enforce.
