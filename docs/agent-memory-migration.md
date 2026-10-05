# Agent memory migration

Persistent agent memory is off in every managed client, as [ADR 0018](adr/0018-agent-memory-off.md) records.
This document maps each former Claude Code memory entry to the default, command, gate, or repository document that replaced it.
The former entries remain on the host where they were written and are no longer loaded.

## Client switches

Each client's profile disables its own memory, and `just profiles` fails when a rendered profile leaves a switch on or unset.

| Client | Profile source | Switches |
| --- | --- | --- |
| Claude Code | `dot_claude/modify_settings.json` | `autoMemoryEnabled = false` |
| Codex | `dot_codex/modify_config.toml` | `[features]` `memories`, `external_agent_memory_import`, `chronicle` and `[memories]` `generate_memories`, `use_memories`, all `false` |
| OpenCode | `dot_config/opencode/opencode.json.tmpl` | `permission.read` and `permission.edit` deny `.github/instructions/memory.instruction.md` at a project root and below any directory, and `permission.bash` denies every command naming `memory.instruction.md` |

The Claude Code and Codex files are rewritten by the clients at run time, so their `modify_` templates merge these keys into the existing file.
The Windows Subsystem for Linux profile merges its whole Claude Code settings template the same way, and that template carries the same key.

## Former entries

| Former entry | Replaced by | Mechanism or document |
| --- | --- | --- |
| `MEMORY.md` | Document | This document; the index has nothing to migrate. |
| `feedback_verify_commit_contents.md` (mechanize instead of operate) | Gate | The pre-commit gate refuses a commit whose staged tree a gate changed (`preserve_staged_tree` in `xtask/src/hooks.rs`), and the post-commit hook refuses an empty commit (`guard/src/postcommit.rs`). The principle is in the `systematic-fixes` skill. |
| `feedback_draft_until_merge_ready.md` | Gate and skill | `pr-workflow ready` refuses while any check on the PR head is unfinished or not passed, or none is reported (`checks_state` in `xtask/src/pr_rules.rs`, [ADR 0013](adr/0013-ready-only-after-passing-checks.md)). The `pull-request` skill runs it only when no scoped work remains. |
| `feedback_no_heavy_apply_loop.md` | Command and gate | `just diff` and `just verify` preview a profile without changing the host. `just apply` refuses without `--live`, the host's own profile, and a fresh backup directory, and the transaction in `xtask/src/transaction.rs` restores every target when application or verification fails ([ADR 0010](adr/0010-rehearse-native-application.md)). |
| `user_security_policy_1password.md` | Gate | `check_public` in `xtask/src/profiles.rs` refuses private key material, credential paths, and secret retrieval in managed templates, and `quality::secrets` runs Gitleaks over the source ([ADR 0009](adr/0009-public-native-profiles.md)). |
| `feedback_bash_tool_mise_shims.md` | Default and document | Hook gates start their tools with the mise shims on `PATH` (`xtask/src/runtime.rs`), and `lefthook.yml` runs `mise x --`. The remaining agent shell behavior is in [agent clients](agent-clients.md#tool-search-path-for-agent-shells-on-windows). |
| `project_herdr_preview_mouse_fix.md` | Default and code | The herdr channel selects its manifest in `tools/dotctl/src/setup/herdr.rs`, tested by `the_preview_channel_reads_the_preview_manifest`, and `.chezmoidata.json` selects the preview channel that carries the upstream fix ([herdrdev/herdr#4319](https://github.com/herdrdev/herdr/pull/4319)). Every tool process started by xtask drops the hook's repository variables (`GIT_LOCAL_ENVIRONMENT` in `xtask/src/tool.rs`), so a test run from a hook cannot write to the repository that ran it. |
| `user_terminal_setup.md` | Default and document | The WezTerm profile starts `pwsh -NoLogo` in the local domain (`.chezmoitemplates/profiles/windows/dot_config/wezterm/wezterm.lua.tmpl`). Its font, opacity, and tab bar come from `platforms.windows.terminal` and its colors from `platforms.windows.palette` in `.chezmoidata.json`; those values are the appearance now, and the entry's Ayu Dark colors, 0.75 opacity, and always shown tab bar are not carried over. The Claude Code host terminal is in [agent clients](agent-clients.md#claude-code-host-terminal-on-windows). |
| `project_claude_auto_mode_classifier.md` | Document | [Agent clients](agent-clients.md#claude-code-permission-evaluation-in-auto-mode). |
| `feedback_bash_tool_posix.md` | Document | [Agent clients](agent-clients.md#the-bash-tool-on-windows). |
| `project_windows_defender_hang_devdrive.md` | Document | [Windows host operations](windows-host.md#defender-engine-hangs-and-the-dev-drive). |
| `feedback_autonomous_execution.md` | Judgment | Listed below. |
| `feedback_official_first.md` | Judgment | Listed below. |
| `feedback_simplicity.md` | Judgment | Listed below. |
| `feedback_docs_no_cross_platform.md` | Judgment | Listed below. |
| `user_theme_preference.md` | Default | `platforms.mac.palette` and `platforms.windows.palette` in `.chezmoidata.json` are the color definitions those profiles' terminal and tool configurations render, so switching the scheme is one edit of each value. Both are Tokyo Night; the entry's Ayu preference is not carried over, and adopting Ayu is a change to those values. |

## Judgment preferences

These entries state a preference that no mechanism here can decide, so each has a proposed home where an agent reads it as an instruction instead of a recollection.

| Former entry | Preference | Proposed home |
| --- | --- | --- |
| `feedback_autonomous_execution.md` | Carry verification, backup, application, and recovery through without handing steps back; ask only for what an agent cannot do, such as restarting its own client. | The Workflow section of the global agent instructions, in the instruction audit of issue #42. |
| `feedback_official_first.md` | Look for the upstream documentation, issues, and recommended setup before writing a local workaround, and cite them where the setting is made. | The `systematic-fixes` skill. |
| `feedback_simplicity.md` | Prefer a minimal configuration near the upstream defaults, add features only when needed, and explain each setting briefly. | The `dotfiles` skill. |
| `feedback_docs_no_cross_platform.md` | Write each file, comment, and document about its own target only, without comparing it to another platform or an earlier version. | The `concise-source` skill. |
