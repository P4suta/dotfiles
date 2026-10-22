---
name: lefthook
description: >-
  Configure and diagnose local Git hooks with Lefthook while preserving the same authoritative project and CI gates.
  Use for hook setup and failures, not bypassing checks to commit or push.
metadata:
  tools: "tools.toml"
---

# Lefthook Git gates

Read the project's hook configuration and the effective `core.hooksPath` before installing or editing hooks.
Respect an existing shared dispatcher, and never overwrite unrelated global hooks.
Use the project's setup command when it has one.
Keep one authoritative configuration in a [supported Lefthook format](https://lefthook.dev/configuration/).

Run quick staged-file checks before commit and the project's fast gate before push: format, lint, type checks, and tests on the current host.
Call the same mise, just, or Rust xtask commands that the pull-request CI gate uses.
Keep proofs, Miri, fuzzing, mutation, stress repeats, and other hosts out of hooks, because `ci-budget` places them at the release gate.
Match file scopes exactly, preserve filenames with spaces, and confine formatter changes to the intended staged content.
Parallelize only independent checks, and propagate every relevant failure.
Keep credential-consuming and release actions out of commit and push hooks.

Fix the code or the broken environment instead of using `--no-verify`, skip variables, or a weaker CI rule.
Preserve signed-commit and Conventional Commit rules.
After changing hook behavior, run the real affected hook command, including a failure case when failure propagation changed.
