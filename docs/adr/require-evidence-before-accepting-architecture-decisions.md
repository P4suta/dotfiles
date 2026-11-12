---
status: proposed
date: 2026-11-06
---

# Enforce concise architecture decision records

## Context

Instructions alone can't reject oversize or incomplete ADRs.
[Nygard](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions) favors short records; the exact budget is owner policy.

## Decision

Use one shared checker for changed personal ADRs.
Enforce a whole-source size budget and substantive context, decision, alternatives, consequences, and confirmation.
Keep semantic review and owner acceptance separate.

## Alternatives

Keeping instructions alone leaves violations unchecked.
Checking every historical record forces unrelated rewrites.
Checking changed records enforces the policy without rewriting untouched history.

## Consequences

Long or incomplete changes block agent completion and publication gates.
Structural checks can't establish sound reasoning.
Reverting the shared checker restores the previous behavior without migrating records.

## Confirmation

Test the actual write, completion, and publication boundaries with oversize, incomplete, and valid records.
Preserve untouched history.
Reconsider the budget when a necessary decision can't remain understandable within it.
