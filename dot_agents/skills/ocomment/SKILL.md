---
name: ocomment
description: >-
  Use the personal OComment tool to check source prose, inspect comment decisions, and apply safe comment edits.
  Use for OComment reports, repository comment gates, staged comment changes, and editing hooks in coding clients.
---

# Comment checks

Use the installed `ocomment` or the target project's pinned command.
For changes to OComment itself, read that checkout's `AGENTS.md` and run its workspace binary, so the changed version judges its own source.
The project-owned [OComment skill](https://github.com/P4suta/OComment/blob/main/skills/ocomment/SKILL.md) defines the command procedure, and the local copy takes precedence when the checkout exists.
Use the installed command's `--help` when its version differs from that procedure.
Never invent options, weaken `.ocomment.toml`, or bypass a hook to pass.

Apply `concise-source` before adding prose.
Run `ocomment check --format agent PATH` for findings to fix or `ocomment check --explain PATH` to inspect the deciding policy.
Use machine output only when a program needs fields, and keep stdout findings apart from stderr diagnostics.
Check the intended files and inspect a diff before applying removals.
Preserve partial staging, legal notices, safety arguments, public usage documentation, and required directives.
When a comment compensates for missing code, fix the code, because deleting the text alone leaves the problem.
