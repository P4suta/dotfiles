# Turn off persistent agent memory in the native client profiles

Status: accepted.

## Context

Claude Code, Codex, and OpenCode each keep persistent memory by default.
Memory differs per client and per machine, escapes review and version control, and fails without a sign when wrong or missing.
Operational knowledge that lived there drifted between clients and no gate enforced it.

Claude Code and Codex rewrite their settings files at run time, so this repository left both files to each machine and couldn't write a key in them.
OpenCode has no memory switch, and its prompt asks the agent to keep memories in `.github/instructions/memory.instruction.md`.

## Decision

Every profile that manages a client's instructions also turns off that client's memory in the client's own settings.
Claude Code sets `autoMemoryEnabled` to `false`.
Codex sets `features.memories`, `features.external_agent_memory_import`, `features.chronicle`, `memories.generate_memories`, and `memories.use_memories` to `false`.
OpenCode denies `read` and `edit` on the prompted memory file at a project root and below any directory, and denies every `bash` command that names it.
OpenCode applies the last matching rule, so each deny follows the `*` rule it narrows.

The files the clients rewrite come from chezmoi `modify_` templates.
Each template parses the current file, overwrites the keys the profile names, and keeps every other key the client wrote.
The Windows Subsystem for Linux profile renders its whole Claude Code settings template the same way.

`just profiles` renders every profile and fails when a managed client's settings miss a memory switch or leave one on or unset.
For OpenCode it evaluates the rendered rules in file order over the memory file's paths and commands.
`xtask/src/profile_rules.rs` holds the pure rule that memory stays off only when every switch reads `false`.
A Kani harness covers every switch list up to the longest one a client uses, with a rejecting counterexample.

A native apply still refuses a target changed outside chezmoi, except a `modify_` target whose rendering keeps every key path of the current host file.
`host_edit_refused` in `xtask/src/profile_rules.rs` holds that rule with a Kani harness and a rejecting counterexample.
`just profiles` also writes a key no template sets into every rendered `modify_` target and fails unless the rendering keeps it.
A template that replaces the whole file fails as a merge.
The same host file has every memory switch on, and the check fails unless the rendering turns each one off again.

The content of the former memories moved to defaults, commands, gates, and repository documents, mapped in `docs/agent-memory-migration.md`.

## Verification at the client boundary

The repository checks cover the rendered settings, and one run covered the clients' own handling on the rendered Windows profile.
OpenCode 1.18.34 refused `read`, `write`, `edit`, and `bash` calls on the memory file at a project root and in a subdirectory, by relative and absolute path, and allowed other files and commands.
The probe runs `opencode debug agent build --pure --tool read --params '{"filePath":".github/instructions/memory.instruction.md"}'` with `OPENCODE_CONFIG` pointing at the rendered file, in a project that contains the file.
The same probe allowed the call when a deny preceded `"*": "allow"`, so the profile check models rule order.
Codex 0.160.0 listed the three features from `config.toml` in `codex features list`, and refused to start with a non-boolean `memories.generate_memories` or `memories.use_memories`, so it reads every key written here.

## Alternatives

Exporting `CLAUDE_CODE_DISABLE_AUTO_MEMORY` from shell profiles loses to a client that starts outside those shells.
Managing the whole Claude Code and Codex settings files on every profile loses because the clients rewrite them.
Every apply would revert runtime choices, and every runtime change would refuse the next apply.
A managed policy file in a system directory needs administrator rights and lies outside a user profile.

## Consequences

A client that adds keys to its settings no longer blocks an apply, and the apply restores the memory keys when the client turned them back on.
The next apply reverts a runtime change to a key a profile writes.
An array a profile writes counts as one value and replaces the host's entries, even when the profile's array stays empty.
The Windows Subsystem for Linux profile's `permissions.allow` and `permissions.deny` discard entries a client added at run time.
A host edit that a rendering would drop still refuses the apply.
The first apply re-serializes the Claude Code and Codex files: Claude Code settings come out with sorted keys and two-space indentation, and Codex settings lose comments and formatting.
The Windows Subsystem for Linux profile now renders the whole OpenCode configuration, so its first apply replaces an existing machine-local `~/.config/opencode/opencode.json`, and the required backup directory keeps the old file.
A new managed client needs its memory switches in its profile and in `xtask/src/agent_memory.rs`, so the profile check covers it.
