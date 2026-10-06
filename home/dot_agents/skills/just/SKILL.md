---
name: just
description: >-
  Maintain a small discoverable just command surface over project tools and Rust xtask logic.
  Use for just recipes and developer entrypoints, not to move procedural code into shell recipes.
---

# Command surface with `just`

Read the existing justfile and `just --list` before adding an entrypoint.
Name commands for the result a developer needs, and reuse the repository's verification path.
Use the configured mise environment for pinned tools.
Keep one command for each behavior.

Keep recipes thin, and have them invoke the project's build, lint, test, or typed Rust helper with clear arguments.
Put parsing, conditional state changes, publication checks, retries, and resource cleanup in `xtask` or another Rust tool.
Express ordering with recipe dependencies, and run only independent work in parallel.
Keep mutation and publication commands apart from read-only checks, and make the default recipe safe.

Preserve arguments and filenames on supported platforms, and choose the shell explicitly when needed.
See [the manual](https://just.systems/man/en/).
Test the affected recipes and their failure propagation, never the justfile text.
List only user-facing commands in the README.
