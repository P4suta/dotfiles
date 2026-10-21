---
status: proposed
date: 2026-10-21
---
# Check the owner's reply language at Stop

## Context

The agent policy and Claude Code's language setting can drift during a long turn.
Replies to the owner need a language check, while persisted artifacts keep their existing English prose checks.
The owner's local configuration stores the language beside the Git author and profile choices.

## Decision

`home/.chezmoi.toml.tmpl` stores `reply_language` under `[data]`, with `Japanese` and `English` as its choices.
The shared `partials/reply_language` supplies `Japanese` for configurations created before this key existed and refuses unsupported values.
Claude Code's setting, the agent policy, and both Stop commands render that same value.
An owner can set `reply_language = "English"` in the local chezmoi configuration and apply the agent targets.

`dotfiles assure stop --reply-language Japanese` checks `last_assistant_message` before it returns the existing Stop response.
[Claude Code's Stop contract](https://code.claude.com/docs/en/hooks#stop-input) supplies the final text directly and cautions that the transcript can lag behind it.
[Codex 0.163.0's input schema](https://github.com/openai/codex/blob/rust-v0.163.0/codex-rs/hooks/schema/generated/stop.command.input.schema.json) supplies the same field, with `null` allowed.
No transcript read substitutes an older message for absent final text.
The language check also works outside a Git repository.
Inside a repository, its refusal and any verification refusal share one response.

The pure rule in `crates/rules/src/reply_language_rules.rs` drops fenced and inline code, URLs, paths, identifier-only table rows, and block quotes.
A remaining line with kana or kanji counts as Japanese.
A line with Latin-letter words and no Japanese counts as English.
Japanese configuration requests a rewrite only when English lines outnumber Japanese lines and those English lines contain at least four words in total.
Four words catch short status sentences while admitting acknowledgments such as "OK, done" and isolated labels.
Ties, empty replies, and code-only replies pass.
English configuration admits every reply.

Both clients supply `stop_hook_active`, so the shared handler requests a rewrite at most once in a continuation chain.
A repeated failure returns `continue: false`, the unmet requirement, and a warning instead of another block.
This shares the existing verification gate's bound and stores no session state.

OpenCode 1.18.34's plugin types expose idle notifications and text-part transformation, without a turn-completion veto or a Stop decision.
Its existing idle verification plugin can send a later prompt, but cannot block the completed turn.
It receives no language check.

## Consequences

The check is a script and word-count heuristic, so it admits some mixed-language replies by design.
The continuation bound can end a turn with an unresolved language warning.
An unrestricted retry policy would risk an endless loop, and transcript parsing could check stale text.
The Kani counting harness covers four lines with zero through five words per line and calls the same counting transition and decision as production.
Synthetic Markdown tests, representative Kani cases, and command integration tests cover stripping and both clients' input and output contracts.
