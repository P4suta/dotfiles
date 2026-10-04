---
name: rust-invariants
description: >-
  Design Rust correctness boundaries with types, exhaustive enums, ownership, and validated state transitions.
  Use for important domain invariants, APIs, and bug-family prevention, not to add abstraction to a trivial edit.
---

# Compiler-Enforced Rust Invariants

Start from the observable guarantee required by callers and the states that would violate it.
Choose the smallest representation in which those invalid combinations cannot be constructed through the supported API.
Treat library consumers, maintainers, operators, and end users as users of the design.
Choose a coherent default from their needs rather than asking the owner to settle routine implementation choices.

Use domain newtypes to separate interchangeable primitives, exhaustive enums for genuinely distinct cases, and private fields with checked constructors for validated inputs.
Represent mutually exclusive states with variants rather than independent booleans or loosely related optional fields.
Use ownership to express who controls resources and when they can be consumed or released.
Use state-specific types when a later operation must require evidence produced by an earlier transition.
Keep validation at the boundary; pass the validated value into the core instead of repeatedly checking raw strings or identifiers.

Make critical transitions exhaustive and explicit.
Do not use wildcard matches that hide an unhandled domain variant, sentinel values, stringly typed commands, or success-shaped defaults for unknown states.
Keep a completed proof or verification bound to the exact identity, revision, bytes, or capability it checked.
A boolean named `verified` is not evidence when its subject can change independently.
Separate pure decisions from I/O, and inject the concrete capabilities a test must drive instead of mutable global test switches.

State precisely what the compiler guarantees and what still depends on runtime validation.
Untrusted files, network responses, signatures, concurrency, and external services remain boundary conditions.
Rust's type system alone does not prove a business rule, cryptographic property, or remote transaction.
Use an explicit argument, property, model, or formal method when the critical algorithm needs stronger evidence.
Use `formal-assurance` for executable Rust verification, kernel-checked models, and independent reference implementations.
Preserve that argument in the relevant ADR and executable tests without calling an ordinary unit test a proof.

Test the public contract, refused states, ownership and failure behavior, and any compatibility promise.
Use compile-fail tests when an API must reject invalid construction or an illegal transition at compile time.
Use property or model tests for algebraic laws and transition sequences when they add coverage ordinary examples cannot provide.
Refactor the affected problem family, not just the observed call site, while keeping unrelated behavior intact.
Use `concise-source` when a comment or lint exception is compensating for an invariant the representation should enforce.
