# Disable persistent agent memory in the native client profiles

Status: Accepted.

## Context

Claude Code, Codex, and OpenCode each keep persistent memory by default.
Memory is per client and per machine, is not reviewed or versioned, and fails silently when it is wrong or missing.
Operational knowledge that lived there drifted between clients and was never enforced.

Claude Code and Codex rewrite their settings files at run time, so this repository left both files to each machine and could not set a key in them.
OpenCode has no memory switch; its prompt asks the agent to keep memories in `.github/instructions/memory.instruction.md`.

## Decision

Every profile that manages a client's instructions also disables that client's memory in the client's own settings.
Claude Code sets `autoMemoryEnabled` to `false`.
Codex sets `features.memories`, `features.external_agent_memory_import`, `features.chronicle`, `memories.generate_memories`, and `memories.use_memories` to `false`.
OpenCode denies `read` and `edit` on the prompted memory file at a project root and below any directory, and denies every `bash` command that names it.
OpenCode applies the last matching rule, so each deny follows the `*` rule it narrows.

The runtime-rewritten files are chezmoi `modify_` templates that parse the current file, merge in the keys the profile sets, and keep every other key the client wrote.
The Windows Subsystem for Linux profile merges its whole Claude Code settings template the same way.

`just profiles` renders every profile and fails when a managed client's settings are missing or leave a memory switch enabled or unset.
For OpenCode it evaluates the rendered rules in file order against the memory file's paths and commands.
The pure rule, that memory is off only when every switch is explicitly disabled, lives in `xtask/src/profile_rules.rs` with a Kani harness over every switch list up to the longest one a client uses and a rejecting counterexample.

Native application still refuses a target changed outside chezmoi, except a `modify_` target whose rendering keeps every key path of the current host file.
`host_edit_refused` in `xtask/src/profile_rules.rs` holds that rule with a Kani harness and a rejecting counterexample.
`just profiles` also writes a key no template sets into every rendered `modify_` target and fails unless the rendering keeps it, so a template that replaces the whole file cannot pass as a merge.

The content of the former memories moves to defaults, commands, gates, and repository documents, mapped in `docs/agent-memory-migration.md`.

## Verification at the client boundary

The repository checks verify the rendered settings; the clients' own handling was verified once against the rendered Windows profile.
OpenCode 1.18.34 refused `read`, `write`, `edit`, and `bash` calls on the memory file at a project root and in a subdirectory, by relative and absolute path, and allowed other files and commands.
The probe is `opencode debug agent build --pure --tool read --params '{"filePath":".github/instructions/memory.instruction.md"}'` with `OPENCODE_CONFIG` set to the rendered file, run in a project that contains the file.
The same probe with a deny placed before `"*": "allow"` was allowed, which is why the profile check models rule order.
Codex 0.160.0 reported the three features from `config.toml` in `codex features list`, and refused to start with a non-boolean `memories.generate_memories` or `memories.use_memories`, so it reads every key set here.

## Alternatives

Setting `CLAUDE_CODE_DISABLE_AUTO_MEMORY` in shell profiles was rejected because a client started outside those shells would not see it.
Managing the whole Claude Code and Codex settings files on every profile was rejected because the clients rewrite them, so every application would revert runtime choices and every runtime change would refuse the next application.
A managed policy file in a system directory was rejected because it needs administrator rights and lies outside a user profile.

## Consequences

A client adding keys to its settings no longer blocks application, and application restores the memory keys if the client turned them back on.
A runtime change to a key a profile sets is reverted by the next application.
A host edit that a rendering would drop still refuses application.
The merge re-serializes the Claude Code and Codex files: Claude Code settings are rewritten with sorted keys and two-space indentation, and Codex settings lose comments and formatting, on the first application.
The Windows Subsystem for Linux profile now renders the whole OpenCode configuration, so its first application replaces an existing machine-local `~/.config/opencode/opencode.json`; the required backup directory keeps the replaced file.
A new managed client needs its memory switches in its profile and in `xtask/src/agent_memory.rs`, so that the profile check covers it.
