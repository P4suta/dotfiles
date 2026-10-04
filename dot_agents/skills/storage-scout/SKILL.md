---
name: storage-scout
description: >-
  Use the personal storage-scout tool to inspect disk usage and safely plan build-artifact cleanup, pruning, and deduplication.
  Use for storage-scout, cache size, cleanup eligibility, and duplicate build outputs.
---

# storage-scout

Use the installed `storage-scout` on the machine whose storage is being inspected.
The project-owned [storage-scout skill](https://github.com/P4suta/storage-scout/blob/main/skills/storage-scout/SKILL.md) is the command procedure; load its local copy when the checkout is available.
Check the installed version and relevant `--help` before relying on version-specific options or JSON fields.
For changes to the tool itself, follow its `AGENTS.md` and use the source checkout's pinned checks.

Start with `storage-scout scan ROOT --measure both --json` and `storage-scout doctor --json` over the smallest relevant roots.
Use `storage-scout explain PATH --phase reap --json` to examine eligibility before changing cleanup policy.
Plan with the tool's dry runs and apply only the candidates and roots authorized by the user.
Do not substitute a recursive deletion, age threshold, forced tier, or unlocked file rewrite for a refused operation.
Use its ownership, lock, identity, and byte-equality checks as the authority for reaping, pruning, and sharing.
Validate the reported schema and distinguish a refusal or warning from an execution error.
For another machine, first load `multi-machine` and its `domyjob` execution procedure.
