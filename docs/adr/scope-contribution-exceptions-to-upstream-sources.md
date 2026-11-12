---
date: 2026-11-08
status: proposed
---

# Scope contribution exceptions to upstream sources

## Context

[Runtime contribution rules](https://github.com/dotnet/runtime/blob/main/.github/copilot-instructions.md#ai-generated-content-disclosure) require a visible AI disclosure.
Personal hooks reject the disclosure and reformat unchanged upstream paragraphs and comments.
External PRs must keep owner-local review controls private.

## Decision

Register required notices with their destination, primary source, reason, and removal condition.
Check the remaining publication text normally.
Register an immutable Git baseline to preserve unchanged paragraphs and comments.
Honor explicit project wrapping settings for registered Markdown.
Match inherited style findings by rule, message, and text without permitting extra occurrences.
Preserve Markdown list items and table cells while checking additions.
Keep personal review controls in local state.

## Alternatives

Disabling hooks would also admit unrelated violations.
Reformatting upstream files would expand the contribution beyond its intended change.
Manual publication would leave the same conflict for later contributions.

## Consequences

Registrations require owner review and fail closed when malformed or unavailable.
Reconsider the representation when a destination requires a notice with variable content.

## Confirmation

Tests must reject notices at another destination, unregistered provenance text, invalid Git identities, and newly authored prose violations.
External descriptions must exclude owner-local controls.
