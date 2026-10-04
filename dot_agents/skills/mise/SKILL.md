---
name: mise
description: >-
  Manage reproducible tool versions and execution with mise across development, CI, and dotfiles-managed machines.
  Use for mise setup, pins, lockfiles, or version-resolution failures.
---

# mise Tool Versions

Read the effective project configuration and existing task entrypoints before changing tool selection.
Use `mise x -- COMMAND` for commands that rely on the project's pinned versions.
Keep project pins in the repository and machine-wide defaults in its existing dotfiles profile.
Do not overwrite a global configuration to repair one project's version.

Use exact version requests or a committed compatible `mise.lock` for reproducibility.
A major selector or `stable` is a moving request, not an exact pin.
Inspect precedence, environment variants, tool backends, and active lockfiles when the resolved version differs from the expected one.
Verify the project's installed mise supports the intended lock and backend behavior against [official documentation](https://mise.jdx.dev/).

Keep development and CI on the same declared toolchain and invoke tasks without relying on interactive shell activation.
Pin mise and its CI action separately where reproducibility requires it, and preserve the repository's SHA-pinning policy.
Review backend provenance and supported platforms before adding an installer.
Store secrets through `doppler`; version configuration and lockfiles are public settings, not credential stores.

Use `renovate` for tested updates and avoid gratuitous version churn during an unrelated fix.
Use `just` for an existing command surface and `xtask` or another maintained Rust helper for procedural logic.
Verify the actual tool version and the affected project commands after changing a pin.
Do not run unrelated configured setup tasks merely to inspect the configuration.
