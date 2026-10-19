---
status: accepted
date: 2026-10-10
---
# Check prose before persistence

## Context

A client displays a reply before its Stop hook can check it.
A rewrite adds another visible reply and consumes another turn.
The reply checks of [ADR 0016](0016-one-writing-standard.md) therefore cannot enforce the standard before display.

## Decision

Managed AI clients MUST stop checking replies with `prose`.
Apply and prose installation remove the Claude Code and Codex reply handlers and the OpenCode plugin.
The cleanup preserves other settings and handlers, including existing empty groups.
The reply command and channel have no remaining consumer and are removed.

The file, comment, Markdown, commit message, issue, pull request, and release note gates remain unchanged.
The pinned linter installation and configuration checks remain unchanged.

Kani checks the [hook group retention rule](../../crates/rules/src/prose_rules.rs).
The migration trusts JSON parsing, filesystem operations, and the standard library's command prefix matching.
[Integration tests](../../crates/prose/tests/prose.rs) cover existing settings, plugin removal, idempotence, failures, and the commit and document gates.

## Alternatives

Disabling new registrations alone would leave existing hosts checking replies.
Requesting another rewrite would still add a second reply after display.

## Consequences

Persisted prose passes its existing gates before it lands.
Reply style depends on the client following its instructions.
The next successful apply or installation removes existing reply checks without requiring a separate migration command.
