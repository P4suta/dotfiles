---
name: ideal-specification
description: >-
  Derive issues, ADRs, specifications, and documentation from the ideal and governing standards, not the request's wording.
  Use when writing any persisted problem, design, or rationale.
---

# Derive from the Ideal

Treat a request as evidence of a problem, not as the specification.

1. Find the governing standards and prior art.
2. State the ideal end state they imply.
3. Derive testable requirements from it.
4. Keep a request element only if it adds a real constraint.

Write:

- Established terms, never the request's words or framing.
- Requirements and reasons, never who asked.
- RFC 2119 keywords where levels matter.
- The fewest words that stay correct.
  Cut anything the reader can act without.

Check: it would read the same had anyone else asked.
