---
name: storage-scout
description: >-
  Use the personal storage-scout tool to inspect disk usage and plan safe build-artifact cleanup, pruning, and deduplication.
  Use for storage-scout, cache size, cleanup eligibility, and duplicate build outputs.
---

# Using storage-scout

Run the installed `storage-scout` on the machine that owns the inspected storage.
The project-owned [storage-scout skill](https://github.com/P4suta/storage-scout/blob/main/skills/storage-scout/SKILL.md) holds the command procedure, so load its local copy when the checkout exists.
Check the installed version and relevant `--help` before relying on version-specific options or JSON fields.
For changes to the tool itself, follow its `AGENTS.md` and use the source checkout's pinned checks.

Start with `storage-scout scan ROOT --measure both --json` and `storage-scout doctor --json` over the smallest relevant roots.
Before changing cleanup policy, run `storage-scout explain PATH --phase reap --json`.
Plan with the tool's dry runs, and apply only the candidates and roots the user authorized.
Never replace a refused operation with a recursive deletion, age threshold, forced tier, or unlocked file rewrite.
The tool's ownership, lock, identity, and byte-equality checks decide reaping, pruning, and sharing.
Distinguish a refusal or warning from an execution error.
For another machine, first load `multi-machine` and its `domyjob` execution procedure.
