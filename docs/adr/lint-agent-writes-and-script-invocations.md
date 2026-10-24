---
status: proposed
date: 2026-10-22
---
# Lint agent writes and script invocations through one runtime

## Context

Repository gates leave temporary scripts and uncommitted agent output unchecked.
Shell command inspection also treats unresolved script variables as an inspection failure.
An executable script can depend on Bash features unavailable in its resolved interpreter.

## Decision

Claude Code and Codex run `dotfiles lint-hook` after file mutations, and OpenCode bridges the same payload through its lint plugin.
The hook checks only the mutation's paths and returns a blocking decision with grouped findings and reproducing commands.
The existing Bash guard applies the same linting before script execution.
Literal assignments resolve script paths without evaluating shell code.
Unresolved paths require a literal path instead of reporting a hook defect.

`crates/rules/src/lint_hook_rules.rs` owns classification, linter selection, configuration selection, admission, literal expansion, and Bash feature detection.
Adapters discover repository configuration, run each linter with a ten-second deadline, and render client responses.
Repository configuration and supported exclusion mechanisms take precedence over strict fallback arguments.
A missing executable or timeout blocks admission.
Unknown file types select no linters and admit an empty outcome set.
Every reported finding blocks admission regardless of classification.
ShellCheck findings are grouped by code, and intentional single-quoted programs receive the SC2016 directive guidance.
The tool registry and shared mise data provision pinned linters on every profile.

## Alternatives

Commit hooks alone leave temporary files unchecked.
Client-specific linter implementations would duplicate policy and diagnostics.
Evaluating shell expressions to discover paths would execute the command being inspected.

## Consequences

Edits remain on disk when linting blocks the agent, so the next action can fix the findings.
External linter behavior and client hook semantics remain integration boundaries.
Action metadata is checked through a synthetic workflow because actionlint does not accept an action metadata file directly.
Kani checks the admission contract and rejecting probes in the proof inventory.
