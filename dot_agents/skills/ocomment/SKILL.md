---
name: ocomment
description: >-
  Use the personal OComment tool to check source prose, inspect comment decisions, and apply safe comment edits.
  Use for OComment reports, repository comment gates, staged comment changes, and agent editing hooks.
---

# OComment

Use the installed `ocomment` or the target project's pinned command.
For changes to OComment itself, read that checkout's `AGENTS.md` and use its workspace binary so the source is judged by the version being changed.
The project-owned [OComment skill](https://github.com/P4suta/OComment/blob/main/skills/ocomment/SKILL.md) is the command procedure; load the local copy when the checkout is available.
Use the installed command's `--help` when its version differs from that procedure.
Do not invent options, weaken `.ocomment.toml`, or bypass a hook to obtain a pass.

Apply `concise-source` before adding prose.
Run `ocomment check --format agent PATH` for actionable findings or `ocomment check --explain PATH` to inspect the deciding policy.
Use machine output only when a program needs fields, and keep stdout findings separate from stderr diagnostics.
Check the actual intended files and inspect a diff before applying removals.
Preserve partial staging, legal notices, safety arguments, public usage documentation, and required directives.
Fix the underlying representation when a comment is compensating for missing code; removing its text alone does not solve that problem.
