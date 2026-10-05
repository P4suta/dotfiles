---
name: portable-skills
description: >-
  Create or maintain shared skills and their dotfiles installation for Claude Code, Codex, OpenCode, and other capable clients.
  Use for skill portability and multi-machine distribution, not ordinary app deployment.
---

# Portable shared skills

Keep one canonical skill under the dotfiles-managed `~/.agents/skills/NAME` tree, with a meaningful name and a short discriminating description in `SKILL.md`.
Codex and OpenCode discover this tree directly.
For Claude Code, link `~/.claude/skills/NAME` to the same directory with a relative path.
After changing a client integration, verify discovery with the installed clients and their current official documentation.
Never make a copy that can drift, point a permanent link into a temporary checkout, or overwrite unrelated client settings or login state.

State the goal, decision criteria, context, invariants, and verification without naming an AI product.
Name capabilities such as filesystem access, the `gh` command, a browser, or native signing tools rather than one client's tool, plugin, or harness.
Substitute a tool only when it preserves the same contract, and report any capability without a substitute.
Keep product-specific discovery or UI metadata out of the portable instructions.
Prescribe an answer layout, subagent count, or approval flow only when the task requires it.

Keep stable procedure apart from repository facts, project status, local paths, machine identities, and credentials.
Put conditional platform or registry details in references loaded only for that case.
Prefer a small composable skill to a broad trigger that takes over unrelated work.
Cross-reference a neighboring skill instead of duplicating its rules.
Use `skill-operations` for dependency-aware loading, usage evidence, improvement dispositions, and the catalog review gate.
Build deterministic reusable automation as a maintained Rust helper with a locked manifest instead of parallel shell or Python glue.

Manage installations and tool versions through the existing dotfiles and mise conventions.
Keep authentication and runtime state machine-local and out of synchronization.
Apply only the intended skill paths on a host.
A skill installation grants no authority to apply another machine's whole dotfiles profile.
Use `dotfiles` to apply matching setup through each affected host's own profile and verify its native discovery entries.

Check names, frontmatter, relative links, required resources, and helper behavior.
The dotfiles `mise run check:rust` gate and its Rust xtask check and generate the shared discovery entries.
Run `mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- install` from the dotfiles source to apply only validated skill files and client entries through chezmoi.
A project-owned manual may load the same canonical file through a small checked-in discovery page on a supported platform without native links.
Try the instructions on a realistic task without client-exclusive tools.
Preserve current user instructions and authorization, and keep a reference project's implementation choices out of universal requirements.
Default approval rules belong in `approval-boundaries`.
A task-specific delegation never grants permanent approval authority in a shared skill.
