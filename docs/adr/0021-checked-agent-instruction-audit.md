# Check that every agent instruction line is classified

Status: Accepted.

## Context

The managed global `CLAUDE.md` and `AGENTS.md` files repeated rules that hooks, guards, and skills already enforce or state, and mixed them with judgment that nothing can enforce.
A duplicated rule drifts from its mechanism, and an agent cannot tell which lines a tool will catch and which depend on it.

## Decision

The instruction files render only the partials `agent_policy`, `prose_policy`, and `remote_machines`.
[The agent instruction audit](../agent-instruction-audit.md) classifies every line of those partials and of the agent and command prompt files, as judgment, as a line kept until a named follow-up mechanizes it, as a heading, as a template action that renders nothing, or as a prompt's front matter setting.
No class admits a template action that renders output, so no instruction text enters from an untracked file.
It maps every removed line to what holds it now: a gate that refuses the violation, a skill that states the rule once, both, or a retained line it merged into.

A skill holds a rule only in the sense that the client loads the skill when its description matches the task.
Nothing refuses work done without loading it, so a line held by a skill is stated once rather than enforced; F5 turns this into a completion gate for `formal-assurance` and `verification-tools` only.

`just check` runs `xtask/src/instruction_audit.rs`, which refuses an unclassified line, a class that does not fit its line, a line that renders template output, a stale or duplicate quote, a follow-up that is undefined, duplicated, or unreferenced, and a path the audit names outside a quote that does not exist.
It refuses a removed line unless a gate row names an existing gate source file, a skill row names an existing skill, and a merged row quotes a fragment of exactly one retained line.
It also refuses a root dispatcher whose actions do more than select its own file in a native profile, a profile template that renders text outside the partials, and a file that a client loads as instructions under any other path, by the target names `CLAUDE.md`, `CLAUDE.local.md`, `AGENTS.md`, `AGENTS.override.md`, `GEMINI.md`, and `copilot-instructions.md`.
Markdown in the Claude Code rule, agent, and command directories, the Codex prompt directory, and the OpenCode agent and command directories is audited line by line.
Each refusal names the file, and the line where one applies, and ends with the edit to make and `just check`.

The decisions on classes, follow-up definitions, and removed-line holders are a pure core in `xtask/src/instruction_rules.rs`, verified by Kani harnesses with rejecting counterexamples in `just proofs`; unit tests cover the parsing and each refusal.

## Alternatives

Removing every line without a mechanism at once was rejected because an unenforced rule would vanish before its mechanism exists.
Keeping the mapping in template comments beside each line was rejected because the removed lines and the follow-ups have no line to sit beside.

## Consequences

Adding an instruction line requires deciding, in the same change, whether a mechanism can enforce it.
Landing a follow-up removes its retained lines and their entries in the same change.
