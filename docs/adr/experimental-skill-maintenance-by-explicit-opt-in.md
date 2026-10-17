---
status: accepted
date: 2026-10-13
---
# Enable experimental skill maintenance only through explicit local opt-in

## Context

Automatic skill observations and maintenance holds occupy development sessions when historical failures lack sufficient evidence for a disposition.
The owner wants ordinary development to proceed with the maintained engineering checks while retaining the maintenance engine and its history for experimentation.
Installing or applying dotfiles must preserve that choice.

## Decision

Skill maintenance defaults to off on every supported profile.
An unmanaged regular file at `~/.config/skill-ops/experimental` enables the experimental mode only when its contents equal `enabled` followed by one LF byte.
An existing `~/.config/skill-ops/disabled` overrides that opt-in.
Missing, invalid, unreadable, or non-regular opt-in files and inaccessible or symlinked parent components leave the mode off.
Chezmoi manages neither marker, so ordinary application cannot opt a machine in or remove its explicit stop marker.

The shared Rust adapter validates the filesystem facts and passes them to the pure mode decision in `crates/rules/src/incident_rules.rs`.
The client templates and OpenCode adapter apply the same contract.
Disabled CLI commands report that no assessment occurred, and the Stop entry point returns an empty response without reading a hook payload or checking freshness.
Automatic recorders return before writing state.
Adapter installation and templates remove only the owned observation and maintenance handlers, preserving unrelated handlers and verification hooks.
The experimental mode restores those handlers through the normal scoped adapter installation or configuration application.

Canonical skills, the maintenance engine, and machine-local observations, incidents, reports, and dispositions remain available.
Disabling the feature changes no historical record into a successful assessment or completed disposition.
Required compiler, typing, lint, test, proof, and source verification contracts remain effective.
Agents read the relevant skill files directly during ordinary development.

Normal shared-skill installation applies the owned skills and static policy.
It validates their exact native contents with the source profile and records the approved catalog through the existing checked hashes and tracked decisions.
This static installation works while experimental maintenance stays off and preserves observations, history, consent markers, and client hooks.
The scoped PR installer still refuses unrelated owned skill or policy drift from that approval.
Native filesystem integration tests exercise fresh installation, changed owned revisions, and mismatch refusals through the production installers.
These tests trust the native filesystem, chezmoi, and existing hash and decision cores.
They supply no proof of external filesystem behavior or a maintenance assessment.
Changes to those external contracts require fresh native checks.

This decision partially supersedes [ADR 0003](0003-local-skill-operations.md): its automatic collectors and completion holds apply only in the experimental mode.
Its immutable-history and evidence-integrity contracts still apply whenever the engine runs.

## Alternatives

Removing the engine would discard useful experimental capabilities and complicate later evaluation.
Enabling it when a stop marker disappears would let a fresh install or accidental deletion restart collection.
Managing the opt-in as a dotfiles target would make ordinary application overwrite the owner's local choice.
Treating disabled operation as a passing assessment would conceal unresolved historical obligations.

## Consequences

Fresh installations perform no automatic skill collection or maintenance assessment.
An owner can experiment on one machine without changing another machine's mode or copying its history.
The existing stop marker on the temporary Mac deployment remains effective until the owner explicitly removes it.
Client sessions may need their normal restart or hook refresh after registration changes; stale owned commands still consult the mode before doing work.
Experimental users retain responsibility for pending findings and the normal client trust process.
