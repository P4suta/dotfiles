# Check that every agent instruction line is classified

Status: Accepted.

## Context

The managed global `CLAUDE.md` and `AGENTS.md` files repeated rules that hooks, guards, and skills already enforce or state, and mixed them with judgment that nothing can enforce.
A duplicated rule drifts from its mechanism, and an agent cannot tell which lines a tool will catch and which depend on it.

## Decision

The instruction files render only the partials `agent_policy`, `prose_policy`, and `remote_machines`.
[The agent instruction audit](../agent-instruction-audit.md) classifies every line of those partials, headings and template directives included, as judgment, as a line kept until a named follow-up mechanizes it, as a heading, or as a directive.
It maps every removed line to what holds it now: a gate that refuses the violation, a skill that states the rule once, both, or a retained line it merged into.

A skill holds a rule only in the sense that the client loads the skill when its description matches the task.
Nothing refuses work done without loading it, so a line held by a skill is stated once rather than enforced; F5 turns this into a completion gate for implementation assurance only.

`just check` runs `xtask/src/instruction_audit.rs`, which refuses an unclassified line, a class that does not fit its line, a stale or duplicate quote, a follow-up that is undefined, duplicated, or unreferenced, a removed line without a valid holder, and a path the audit names that does not exist.
It also refuses a root dispatcher that does more than select its own file in a native profile, a profile template that renders text outside the partials, and an instruction file under any other path.
Each refusal names the file, and the line where one applies, and ends with the edit to make and `just check`.

The decisions on classes, follow-up definitions, and removed-line holders are a pure core in `xtask/src/instruction_rules.rs`, verified by Kani harnesses with rejecting counterexamples in `just proofs`; unit tests cover the parsing and each refusal.

## Alternatives

Removing every line without a mechanism at once was rejected because an unenforced rule would vanish before its mechanism exists.
Keeping the mapping in template comments beside each line was rejected because the removed lines and the follow-ups have no line to sit beside.

## Consequences

Adding an instruction line requires deciding, in the same change, whether a mechanism can enforce it.
Landing a follow-up removes its retained lines and their entries in the same change.
