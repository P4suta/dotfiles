---
name: portable-skills
description: >-
  Create or maintain shared agent skills and their dotfiles installation for Claude Code, Codex, OpenCode, and other capable clients.
  Use for skill portability and multi-machine distribution, not ordinary application deployment.
---

# Portable Shared Skills

Keep one canonical skill under the dotfiles-managed `~/.agents/skills/NAME` tree.
Use `SKILL.md` with a meaningful name and a short discriminating description.
Codex and OpenCode discover this tree directly; use a relative `~/.claude/skills/NAME` alias to the same directory for Claude Code.
Verify discovery against the installed clients and current official documentation when changing their integration.
Do not make independent copies that can drift, point permanent aliases into temporary checkouts, or overwrite a client's unrelated settings or login state.

Express the goal, decision criteria, necessary context, invariants, and verification independently of an AI product.
Refer to capabilities such as filesystem access, a GitHub CLI, a browser, or native signing tools rather than requiring one client's tool name, plugin, or harness.
Use an available equivalent when it preserves the same contract; explain a real missing capability instead of pretending it exists.
Keep product-specific discovery or UI metadata outside the portable task instructions.
Do not force a particular answer layout, agent count, or approval flow unless the task actually requires it.

Separate stable reusable procedure from repository facts, current project status, local paths, machine identities, and credentials.
Keep conditional platform or registry details in references loaded only for that case.
Prefer a small composable skill to a broad trigger that takes over unrelated work.
Cross-reference a neighboring skill for shared behavior rather than duplicating its rules.
Use `skill-operations` for dependency-aware loading, local usage evidence, mandatory improvement dispositions, and the current catalog review gate.
Use a maintained Rust helper with a locked manifest when deterministic reusable automation is warranted.
Do not add shell or Python glue as a parallel source of behavior.

Manage installations and tool versions through the existing dotfiles and mise conventions.
Keep authentication and runtime state machine-local and excluded from synchronization.
Apply only the intended skill paths on a host; a skill installation is not authorization to apply an entire other machine's dotfiles profile.
Follow the project's multi-machine execution and synchronization instructions before operating another host.
Use `dotfiles` to apply equivalent skill setup through each affected host's own profile and verify its native discovery entries.

Validate names, frontmatter, relative links, required resources, aliases, and the helper's observable behavior.
Use the dotfiles `mise run check:rust` gate and its Rust xtask to validate and generate the shared discovery entries.
Use `mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- install` from the dotfiles source to apply only validated skill files and client entries through chezmoi.
Project-owned manuals may use small checked-in discovery pages that load the same canonical file when a native link is unavailable on a supported platform.
Try the instructions against a realistic task using the available client without assuming client-exclusive tools.
Preserve current user instructions and existing authorization, and do not turn a reference project's implementation choices into universal requirements.
Default approval rules belong in `approval-boundaries`; a task-specific delegation must not become permanent approval authority in a shared skill.
