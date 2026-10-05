# Check that every agent instruction line is classified

Status: Accepted.

## Context

The managed global `CLAUDE.md` and `AGENTS.md` files repeated rules that hooks, guards, and skills already enforce or state, and mixed them with judgment that nothing can enforce.
A duplicated rule drifts from its mechanism, and an agent cannot tell which lines a tool will catch and which depend on it.

## Decision

The instruction files render only the partials `agent_policy`, `prose_policy`, and `remote_machines`.
[The agent instruction audit](../agent-instruction-audit.md) classifies every instruction line of those partials as judgment or as a line kept until a named follow-up mechanizes it, and maps every removed line to the mechanism that holds it.
A line that only names a skill for a trigger the skill's own description states is removed, because clients select skills from their descriptions.
`just check` runs `xtask/src/instruction_audit.rs`, which refuses an unclassified line, a stale or duplicate entry, a follow-up reference without a definition, and a profile template that renders text outside the partials.
Each refusal names the file and line and ends with the edit to make and `just check`.

The check is string matching over tracked text with no decision state to explore, so it has no Kani harness; its unit tests cover each refusal, and one test runs it on the repository's own files.

## Alternatives

Removing every line without a mechanism at once was rejected because an unenforced rule would vanish before its mechanism exists.
Keeping the mapping in template comments beside each line was rejected because the removed lines and the follow-ups have no line to sit beside.

## Consequences

Adding an instruction line requires deciding, in the same change, whether a mechanism can enforce it.
Landing a follow-up removes its retained lines and their entries in the same change.
