---
status: accepted
---
# Refuse generic anti-patterns through one catalog of checks

## Context

Unnamed numbers, inline URLs and system paths, and discarded failures reached merged code because no gate refused them.
The Common Weakness Enumeration classifies these as `CWE-1106`, `CWE-547`, and `CWE-390`, and the secure coding standards of the Software Engineering Institute forbid them.
The pure core of [ADR 0024](0024-pure-decision-core-behind-adapters.md) already runs a shared `ast-grep` catalog, which other repositories can run too.

## Decision

`home/dot_config/assure/catalog.toml` maps each anti-pattern to its weakness and to the check that refuses it in each language this repository uses.
Each check names an `ast-grep` rule of the catalog or a linter's own rule, such as `PSAvoidUsingEmptyCatchBlock` of `PSScriptAnalyzer`.
A language without a check carries an `uncovered` reason, and the structural gate refuses a language that has neither.
Each rule names its weakness in `metadata` and the corrective next action in `note`, and its fixtures show it refuses a violation and admits the corrected form.

A number or a location belongs in a named constant: a `const` item in Rust, or a module-level `const` in TypeScript.
A failure that leaves the caller's result standing goes through `platform::cleanup`, which reports it on standard error with what failed, and a missing path counts as removed.
Code that only tests and proofs compile may keep literals, as may array lengths, attributes, and enum discriminants.
A comment never suppresses a catalog rule, so the code changes instead.

Every host receives the catalog at `~/.config/assure`, so any repository can run `ast-grep scan --config ~/.config/assure/sgconfig.yml`.
The global enforcement points that run it in every repository arrive with issue 114, which makes every repository verify its code in proportion to its shape.

## Alternatives

A shrink-only baseline of findings would have let the existing literals stay, so the repository fixed them instead.
Clippy's `let_underscore_must_use` covers only `let _` on a `#[must_use]` value, while the catalog rule also refuses `.ok();`, an empty `Err(_)` arm, and the same failures in the shell and TypeScript code.

## Consequences

Shell and `PowerShell` code has no check for unnamed numbers and inline paths yet, as the catalog records with its reasons.
A new rule needs fixtures and a catalog entry before the gate admits it.
