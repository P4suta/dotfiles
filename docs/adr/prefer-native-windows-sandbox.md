---
date: 2026-10-24
status: proposed
---
# Prefer the supported native Windows sandbox

## Status

Proposed for the Windows Codex profile.

## Context

The legacy elevated sandbox runs commands as separate Windows users.
A protected user-local mise directory can prevent those users from launching its PowerShell shim.
PowerShell then fails before any command or profile code runs.
An unrestricted session or a system-wide executable can still work under different permissions.

Microsoft Execution Containers (MXC) provide native process isolation on supported Windows devices.
The [official sandbox guide](https://learn.chatgpt.com/docs/windows/windows-sandbox) recommends this implementation when the device and managed policy permit it.
The [configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference) defines automatic preference with legacy fallback.

## Decision

The Windows [Codex template](../../home/dot_codex/modify_config.toml) sets `features.prefer_mxc = true`.
It preserves the host's legacy selection, permission profile, approval policy, and unrelated keys.
Codex owns capability detection and managed-policy enforcement.
Unavailable or prohibited native isolation retains the configured legacy implementation.
A failed command doesn't trigger fallback.
Mac, Linux, and WSL keep their host preferences because this selection governs Windows execution.

Disabling sandboxing would remove the intended permission boundary.
Granting broad access to user-local tool directories would change machine permissions and couple each install to the legacy sandbox accounts.
Forcing `windows.sandbox = "mxc"` would reject devices without native support instead of keeping their legacy selection.

## Consequences

Compatible devices can launch user-local tools without granting the legacy sandbox users access to each installation.
The chosen implementation still enforces Codex's active permission profile.
Native isolation has its own loopback, child-process lifetime, and denied-read timing constraints, as the official guide describes.
This change doesn't repair legacy filesystem permissions on devices that require the fallback.

The [configuration regression](../../xtask/tests/codex_config.rs) exercises the production template across all profiles, roles, signing settings, and absent, false, or true host preferences.
The pinned chezmoi interpreter checks this external semantic boundary.
Kani doesn't execute Go templates or Windows isolation APIs.
Existing decision-core proofs remain required at the release gate.
Native sandbox probes check process startup and project reads on the affected host.
These checks establish neither upstream isolation correctness nor compatibility on every Windows build.
Revisit the choice when Codex changes implementation selection, managed policy, or process and network isolation semantics.
