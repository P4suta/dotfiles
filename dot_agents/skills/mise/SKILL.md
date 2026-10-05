---
name: mise
description: >-
  Manage reproducible tool versions and execution with mise across development, CI, and dotfiles-managed machines.
  Use for mise setup, pins, lockfiles, or version-resolution failures.
---

# Tool versions with mise

Read the effective project configuration before changing tool selection.
Use `mise x -- COMMAND` for commands that rely on the project's pinned versions.
Keep project pins in the repository and machine-wide defaults in the dotfiles profile.
Never overwrite a global configuration to repair one project's version.

Use exact version requests or a committed compatible `mise.lock`.
A major selector or `stable` moves, so it never counts as an exact pin.
Inspect precedence, environment variants, tool backends, and active lockfiles when the resolved version differs from the expected one.
See the [official documentation](https://mise.jdx.dev/).

Keep development and CI on the same declared toolchain, and invoke tasks without interactive shell activation.
Pin mise and its CI action where reproducibility requires it, and follow the repository's SHA-pinning policy.
Store secrets through `doppler`.

Use `renovate` for tested updates, and leave versions alone during an unrelated fix.
Verify the actual tool version and the affected project commands after changing a pin.
