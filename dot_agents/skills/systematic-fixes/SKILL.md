---
name: systematic-fixes
description: >-
  Fix a discovered defect by reproducing it, finding its architectural cause, and preventing the same problem family.
  Use for bug investigation and remediation, not speculative wholesale rewrites.
---

# Fix the Problem Family

Start with an observed failure and a regression test or a deterministic reproducer that fails for the right reason.
Preserve useful evidence before modifying the code.
Explain both the immediate cause and the design or verification gap that allowed the defect to exist.
Trace why the existing compiler checks, proofs, schemas, test generators, or boundary validators failed to cover the violated invariant.
Check whether the verification mechanism omitted a variant, shared the implementation's mistaken assumption, accepted stale evidence, or lacked a real boundary check.
Do not treat a symptom disappearing as evidence that its cause has been removed.

Identify the invariant that should have held and the actual boundary responsible for it.
Search other producers, consumers, variants, and transitions governed by that same invariant.
Define the affected family from a shared mechanism, not a resemblance in names or formatting.
Use the search to establish scope; do not rewrite unrelated systems merely because a stronger design is imaginable.

Prefer making the invalid state or transition unavailable through types, exhaustive variants, ownership, or checked constructors.
For external state, validate at the boundary and bind the resulting evidence to its exact subject.
Use a meaningful automated gate or diagnostic where the defect cannot be eliminated by representation alone.
Avoid spreading local conditionals, fallback values, assertions, allowlists, or reviewer reminders across every caller when one authoritative boundary can enforce the rule.
Do not hide the failure, weaken a gate, or add an unrelated fallback to obtain a passing result.

Apply the correction across the affected family and test its representative cases, edge conditions, and failure transitions.
Demonstrate that the original failure is prevented and previously valid behavior remains valid.
Add compile-fail, property, concurrency, fault-injection, or integration tests when they establish the missing invariant.
Make coverage of domain variants and enforcement boundaries exhaustive or self-checking so a future addition cannot silently escape the mechanism.
Use an independent specification or real-system evidence when two implementations could agree on the same incorrect assumption.
Improve the trace or reproducer when the original investigation depended on guessing.
Record a changed durable design in an ADR; keep routine debugging history out of permanent documentation.

Complete the authorized fix and run the appropriate project gates.
Resolve discovered gaps instead of treating a written limitation or follow-up as completion; use `ideal-first-development` for a larger coordinated correction.
A claim of root-cause prevention must name the mechanism that now enforces it and the evidence that checks that mechanism.
Bind that claim to the identified invariant and supported boundary; do not claim that ordinary tests eliminate every possible future bug.
