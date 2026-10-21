---
date: 2026-10-13
status: proposed
---

# Validate Renovate reports before classifying freshness

The local dependency gate consumes reports from a pinned Renovate container for the exact offered Git tree.
A report can contain nonfatal repository notices and dependencies with no selected version while its other lookups remain complete.
Treating every problem or skipped extraction as a failed lookup prevents valid reports from reaching the freshness decision.

Decode the report in the Rust Renovate backend and validate every dependency and update before exclusions, pending checks, or update-type filtering.
Reject duplicate JSON fields and invalid types.
Preserve unknown metadata for producer compatibility without using it as evidence.
Keep raw declared versions separate from resolved versions used in diagnostics, so display improvements do not change source attribution.

Classify known Bunyan levels through the pure freshness core.
Trace, debug, info, and warning records are nonfatal notices.
Error, fatal, unknown, or malformed severity refuses evidence.
Dependency lookup warnings still refuse required evidence regardless of repository severity.
An `invalid-value` or `unspecified-version` extraction needs no version lookup only when `currentValue`, `currentDigest`, and `lockedVersion` are absent or null and updates and warnings are empty.
An empty string is a selected value.
This decision is independent of the dependency manager.
Keep the existing exact hosted-runner, matrix-input, and verified-counterpart contracts.

Keep full-report inspection strict for every lookup.
For a push with a verified source scope, require lookup evidence for changed manifests and affected inherited Cargo providers.
Unknown source scope requires every lookup.
Structural validation and repository error checks cover the complete report before source selection.
An unrelated failed lookup supplies no freshness evidence and cannot hide a malformed record or a repository error.

Report failures identify the manager, manifest, dependency, and a recognized reason without copying warning messages, HTTP payloads, or credentials.
`dotguard renovate inspect <report.json>` checks saved lookup evidence without Docker and lists selected update candidates.
Inspection does not establish freshness for a push, which still requires the offered-tree snapshot and source attribution.

Kani verifies the production severity and lookup decisions over their complete scalar input domains, together with the existing freshness decision.
It also verifies source selection for every combination of known scope, changed manifest, and affected inherited provider.
Registered counterexamples must fail on assertions against those same functions.
The JSON adapter depends on serde decoding and Renovate's field meanings, which the scalar Kani contract does not model.
Typed validation tests, a minimized report from Renovate 44.52.0, native guard integration tests, and a committed fuzz corpus cover that boundary.
Fuzzing is bounded evidence rather than a proof for every JSON document.
Reconsider the adapter contract when the pinned producer changes its report schema, skip meanings, or severity model.

## Alternatives

Treating every producer notice as an error refuses valid lookups and hides the actual freshness decision.
Skipping report validation admits failed lookups, malformed fields, and ambiguous duplicate keys.
Manager-specific exceptions duplicate shared report semantics and leave the same failure in other supported dependency managers.
