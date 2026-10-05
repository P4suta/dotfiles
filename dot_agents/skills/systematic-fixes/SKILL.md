---
name: systematic-fixes
description: >-
  Fix a discovered defect by reproducing it, finding its architectural cause, and preventing the same problem family.
  Use for bug investigation and remediation, not speculative wholesale rewrites.
---

# Fix the problem family

Start from an observed failure and a regression test or deterministic reproducer that fails for the right reason.
Preserve useful evidence before changing the code.
Explain both the immediate cause and the design or verification gap that let the defect exist.
Trace why the existing compiler checks, proofs, schemas, test generators, or boundary validators missed the violated invariant.
Check whether the verification omitted a variant, shared the implementation's mistaken assumption, accepted stale evidence, or lacked a real boundary check.
A vanished symptom proves no removed cause.

Name the invariant that should have held and the boundary responsible for it.
Search other producers, consumers, variants, and transitions under that invariant.
Define the affected family by a shared mechanism instead of similar names or formatting.
Use the search to set scope, and leave unrelated systems alone even when a stronger design comes to mind.

Prefer making the invalid state or transition unrepresentable through types, exhaustive variants, ownership, or checked constructors.
For external state, check at the boundary and bind the resulting evidence to its exact subject.
When representation alone leaves the defect possible, add a meaningful automated gate or diagnostic.
When one authoritative boundary can enforce the rule, avoid spreading local conditionals, fallback values, assertions, allowlists, or reviewer reminders across callers.
A memory, note, or resolution to look harder fixes nothing, so enforce the invariant in a gate that runs unattended.
Never hide the failure, weaken a gate, or add an unrelated fallback to get a passing result.

Apply the correction across the affected family, and test its representative cases, edge conditions, and failure transitions.
Show that the original failure stops and that valid behavior still works.
Add compile-fail, property, concurrency, fault-injection, or integration tests when they establish the missing invariant.
Make coverage of domain variants and enforcement boundaries exhaustive or self-checking, so a future addition stays inside the mechanism.
When two implementations could share the same wrong assumption, use an independent specification or real-system evidence.
Improve the trace or reproducer when the investigation depended on guessing.
Record a changed durable design in an architecture decision record, and keep routine debugging history out of permanent documentation.

Complete the authorized fix and run the fitting project gates.
Resolve discovered gaps instead of treating a written limitation or follow-up as completion.
Use `ideal-first-development` for a larger coordinated correction.
A claim of root-cause prevention must name the mechanism that now enforces it and the evidence that checks that mechanism.
Bind that claim to the identified invariant and supported boundary, and never claim that ordinary tests rule out every future bug.
