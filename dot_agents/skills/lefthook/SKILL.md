---
name: lefthook
description: >-
  Configure and diagnose local Git hooks with Lefthook while preserving the same authoritative project and CI gates.
  Use for hook setup and failures, not bypassing checks to commit or push.
---

# Lefthook Git Gates

Read the project's hook configuration and the effective `core.hooksPath` before installing or editing hooks.
Respect an existing shared dispatcher; installation must not overwrite unrelated global hooks.
Use the project's setup command when it is the supported integration point.
Keep a single authoritative configuration in a [supported Lefthook format](https://lefthook.dev/configuration/).

Run quick applicable staged-file checks before commit and the required project verification before push.
Use the same underlying mise, just, or Rust xtask commands that CI uses.
Match file scopes accurately, preserve filenames with spaces, and keep formatter changes confined to intended staged content.
Parallelize only independent checks and propagate every relevant failure.
Keep credential-consuming and release actions outside commit and push hooks.

Diagnose a failed gate using its actual output and the configured tool versions.
Fix the code or the broken verification environment rather than using `--no-verify`, skip variables, or a weaker CI rule.
Preserve signed-commit and Conventional Commit requirements.
After changing hook behavior, validate the real affected hook command, including an intended failure case when failure propagation changed.
Do not treat local hook success as evidence that remote required checks have passed.
