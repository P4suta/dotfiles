# Check that every agent instruction line has a class

Status: accepted.

## Context

The managed global `CLAUDE.md` and `AGENTS.md` files repeated rules that hooks, guards, and skills already enforce or state, and mixed them with judgment that nothing can enforce.
A duplicated rule drifts from its mechanism, and an agent can't tell which lines a tool catches and which depend on it.

## Decision

The instruction files render only the partials `agent_policy`, `prose_policy`, and `remote_machines`.
[The agent instruction audit](../agent-instruction-audit.md) gives every line of those partials and of the agent and command prompt files one class:

- judgment
- a line kept until a named follow-up mechanizes it
- a heading
- a template action that renders nothing
- a prompt's front matter setting

No class admits a template action that renders output, so no instruction text enters from an untracked file.
The audit maps every removed line to what holds it now.
A gate that refuses the violation, a skill that states the rule once, both, or a retained line that absorbed it holds each one.

A skill holds a rule only because the client loads the skill when its description matches the task.
Nothing refuses work done without loading it, so a skill states a line once and enforces nothing.
F5 turns this into a completion gate for `formal-assurance` and `verification-tools` only.

`just check` runs `xtask/src/instruction_audit.rs`, which refuses:

- an unclassified line
- a class that doesn't fit its line
- a line that renders template output
- a stale or duplicate quote
- a follow-up that has no definition, a second definition, or no reference
- a path the audit names outside a quote that doesn't exist

It refuses a removed line unless a valid row holds it.
A gate row names an existing gate source file, a skill row names an existing skill, and a merged row quotes a fragment of exactly one retained line.
It also refuses a root dispatcher whose actions do more than select its own file in a native profile, and a profile template that renders text outside the partials.
It refuses a file that a client loads as instructions under any other path, by the target names `CLAUDE.md`, `CLAUDE.local.md`, `AGENTS.md`, `AGENTS.override.md`, `GEMINI.md`, and `copilot-instructions.md`.
The audit reads Markdown in the Claude Code rule, agent, and command directories, the Codex prompt directory, and the OpenCode agent and command directories line by line.
Each refusal names the file and, where one applies, the line, and ends with the edit to make and `just check`.

`xtask/src/instruction_rules.rs` holds the decisions on classes, follow-up definitions, and removed-line holders as a pure core.
Kani harnesses with rejecting counterexamples verify it in `just proofs`, and unit tests cover the parsing and each refusal.

## Alternatives

Removing every line without a mechanism at once loses because an unenforced rule would vanish before its mechanism exists.
Keeping the mapping in template comments beside each line loses because the removed lines and the follow-ups have no line to sit beside.

## Consequences

Adding an instruction line requires deciding, in the same change, whether a mechanism can enforce it.
Landing a follow-up removes its retained lines and their entries in the same change.
