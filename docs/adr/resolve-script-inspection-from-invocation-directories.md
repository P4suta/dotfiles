---
date: 2026-10-25
status: proposed
---
# Resolve script inspection from invocation directories

## Status

Proposed.

## Context

An event directory can differ from the directory where a later command executes.
Inspecting a relative script from the event directory can select a different file with the same name.
Legacy policy and script linting need the same selected files and unresolved-directory results.

Shell location changes depend on control flow and process boundaries.
PowerShell also provides [built-in location functions](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_built-in_functions?view=powershell-7.6), while [Set-Location accepts provider paths](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.management/set-location?view=powershell-7.6).
An absolute filesystem directory requires more evidence than a parsed command name.

## Decision

Use the existing native syntax parsers to carry an invocation directory through supported control flow.
Accept literal absolute Bash location changes on successful `&&` branches and the supported straight-line PowerShell `Set-Location -LiteralPath` form with explicit `-ErrorAction Stop`.
Keep unsupported providers, conditional joins, location functions, startup options, and uncertain command bindings unresolved.
Preserve binding uncertainty after later literal location calls.
Resolve relative files only after establishing an absolute invocation directory, including extensionless shebang probes.
Inspect nested relative scripts from their caller's directory even when the entry script has an absolute path.

Collect selected files and unavailable inspections once in legacy inspection, and pass that catalog to script linting.
Keep mandatory GitHub body inspection and existing depth and input limits.
Track literal assignment facts within each supported process scope.
Don't persist command-prefix assignments as expansion facts for the command they precede.
Refuse variable paths when conditional or unresolved control flow prevents establishing their values.

The production [DirectoryContext](../../crates/rules/src/legacy_rules.rs) owns location joins and sticky binding invalidation.
Its source-bound Kani harness checks the same generic operations over `Option<u8>`, including equal, unequal, and absent values.
Production uses `PathBuf`, whose equality supplies the abstraction's external value comparison.
The proof establishes the state operation under that comparison, not shell parsing, command bindings, filesystem identity, or executor behavior.
Planted script regressions distinguish correct selection from stale reads across the native parser boundaries.

## Consequences

Unknown contexts produce explicit inspection refusals before relative file reads.
Some valid compound commands require literal script paths or simpler invocations.
Native initial command bindings and inherited startup configuration remain assumptions of the external executor.
The parser's `-NoProfile` subprocess doesn't establish the executor's bindings or runspace lifetime.

Reusing the event directory after a location change can inspect the wrong file.
Independent script discovery in the lint hook can disagree with legacy policy.
Evaluating shell code to discover a directory would execute the command under inspection.
