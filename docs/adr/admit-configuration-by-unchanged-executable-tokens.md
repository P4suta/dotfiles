---
status: proposed
date: 2026-11-06
---

# Admit configuration by unchanged executable tokens

## Context

Extension checks reject just recipes and Rust test inventories.
Admitting arbitrary Rust files would hide executable changes.

## Decision

Recognize conventional just filenames as configuration.
For Rust, parse both revisions and compare executable tokens after removing literal string-array constants.
Preserve attributes, types, macro expressions, and executable items outside that admitted data form.
Require the declared configuration tests.

## Alternatives

A filename allowlist needs repository-specific maintenance and can admit added executable code.
Moving every inventory to another format changes projects to accommodate the owner tool.

## Consequences

The gate trusts the pinned Rust parser and token printer.
The pinned parser constructs constants with default generic fields, so populated generic fields are unreachable at classification.
Source tests require generic declarations to stay outside the accepted data form if parser support changes.
Unsupported data forms remain executable changes.

## Confirmation

Fixtures must admit standalone and mixed inventories while rejecting changed or deleted functions, macro initializers, and hidden attributes.
Proofs check the admission decision.
Reconsider the admitted data form when a project needs another declarative representation.
