---
date: 2026-10-22
status: accepted
---
# Scope a legacy bypass to one script invocation

## Status

The owner accepted this route on 2026-10-22.

## Context

The agent shell hook inspects scripts before running them and refuses legacy commands inside third-party installers that the caller can't edit.
The existing `LEGACY: REASON` bypass admits one command segment, and repository exceptions identify only managed bootstrap paths.
A caller needs an explicit, recorded route for running one external script while other command segments retain their checks.

## Decision

An invocation-local `DOTFILES_LEGACY_BYPASS='SCRIPT_PATH: REASON'` skips inspection of exactly the script path the invocation names.
The [pure rule](../../crates/rules/src/legacy_rules.rs) matches the complete path followed by a colon and a nonempty trimmed reason.
It preserves Windows drive colons and compares paths without normalization.

The [hook](../../crates/dotfiles/src/legacy.rs) applies this decision before reading metadata or contents, checking size, or descending into the script.
It records the trimmed reason and invocation words once through the existing `Rule::Legacy` bypass log.
The invocation's own command and every sibling segment, including inline shell commands, retain inspection.
The legacy-entry bypass and repository exceptions keep their existing behavior.

Piping a script through standard input would evade the file inspection boundary and conflict with the download-to-shell refusal.
Adding third-party paths to repository exceptions would persist a broader exemption without an invocation-specific reason.
Neither alternative provides this explicit, recorded scope.

## Consequences

Supported shell invocations, sourcing, and direct shell-script paths share one matching rule.
An admitted script's contents and descendants remain uninspected, including files that the hook couldn't read.
The caller takes responsibility for that script and records why, while unrelated commands still receive diagnostics.
The [proof inventory](../../policy/obligations.toml) includes the matching harness and rejecting probes, and hook tests cover scope, logging, and unavailable inspections.
