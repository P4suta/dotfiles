---
name: just
description: >-
  Maintain a small discoverable just command surface over project tools and Rust xtask logic.
  Use for just recipes and developer entrypoints, not to move procedural code into shell recipes.
---

# just Command Surface

Read the existing justfile and `just --list` before adding an entrypoint.
Keep commands named for the result a developer needs and reuse the repository's current verification path.
Use the configured mise environment for pinned tools.
Prefer one discoverable command surface rather than several aliases implementing the same behavior.

Keep recipes thin: invoke the project's build, lint, test, or typed Rust helper with clear arguments.
Put parsing, conditional state changes, publication checks, retries, and resource cleanup in `xtask` or another Rust tool.
Use recipe dependencies to express ordering and explicit parallel execution only for independent work.
Keep mutation and publication commands distinct from read-only checks; a default recipe should be safe and helpful.

Preserve arguments and filenames correctly on supported platforms and choose the supported shell explicitly when needed.
Validate against the installed version and [just's manual](https://just.systems/man/en/).
Check the affected recipes and their failure propagation instead of adding tests that only match justfile text.
Keep recipe documentation in help and existing contributor instructions, with only user-facing commands in the minimal README.
