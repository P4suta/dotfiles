---
status: proposed
date: 2026-10-14
---
# Report optional notification failures with fixed warnings

## Context

The [notification queue decision](speak-fixed-development-events-through-an-optional-desktop-queue.md) preserves the operation's result and streams when notification fails.
Silently discarding an unexpected queue, worker, or callback failure prevents diagnosis.
Formatting arbitrary errors can expose command arguments, private paths, or notification identities.

## Decision

Preserve the primary operation's result and standard output.
Report unexpected notification failures on standard error with a fixed operation name and fixed warning text.
Never render an arbitrary error value, command, payload, path, or request identity.
Keep disabled notifications, unavailable desktops, missing recordings, and rejected malformed requests quiet.
Use the closed operation set in [the speech adapter](../../crates/platform/src/speech.rs) and the fixed warning in [the OpenCode adapter](../../home/dot_config/opencode/plugins/notifications.ts).

## Alternatives

Discarding every failure would preserve streams while hiding faults in enabled notifications.
Printing the original error would improve detail but expose values outside the notification contract.
Failing the primary operation would give an optional service authority over development gates.

## Consequences

An enabled notification can emit a diagnostic without failing or falsely announcing completion of the primary operation.
The fixed warning omits detailed error causes to preserve privacy.
[Boundary tests](../../crates/dotfiles/tests/notifications.rs) check quiet cases and exact warnings.
The speech adapter test checks that reporting never invokes an error's formatter.
