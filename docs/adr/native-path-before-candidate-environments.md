---
date: 2026-10-12
status: proposed
---
# Recover native PATH before constructing candidate environments

## Context

Mise activation prepends project installation directories to PATH and records their origin in activation metadata.
Discarding that metadata while retaining the activated PATH makes a candidate installer inherit the parent's tools as native directories.
A remote bootstrap can also replace the activation metadata after capturing PATH.

## Decision

The [native PATH constructor](../../crates/platform/src/code_environment.rs) captures PATH and its activation metadata together before candidate state replaces their origin.
The remote [bootstrap](../../crates/dotfiles/src/hosts.rs) transports that pair with explicit metadata presence.
Incomplete or conflicting provenance refuses construction.
Without activation metadata, the captured native PATH needs no external query.
Otherwise, the captured mise executable recovers PATH through its environment command under owned configuration, cache, state, and temporary storage.
Only the validated PATH is retained, and unsuccessful or malformed responses refuse construction without displaying their payload.
Required native executable capabilities are captured before their installation directories disappear from PATH.
The [candidate environment](../../crates/dotfiles/src/host_checks.rs) uses this result after dropping activation metadata and redirecting writable user state.

## Alternatives

Retaining the entire activated PATH repeats tool installation directories and changes descendant executable resolution.
Applying inherited activation metadata after changing HOME can restore the wrong origin and configuration.
Removing directories by pathname assumes mise's internal activation format and can remove legitimate native capabilities.

## Consequences

An activated parent requires one isolated native query when constructing its candidate environment.
The environment command and operating-system executable lookup remain external trust boundaries.
The [native regression](../../crates/dotfiles/tests/host_runtime.rs) uses the installed pinned mise to activate a tool, rejects inherited candidate paths and malformed metadata, preserves an actual native executable, and poisons unrelated native configuration.
Changes to mise's activation protocol require renewed native evidence.
