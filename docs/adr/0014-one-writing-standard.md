# Enforce one writing standard on persisted prose and replies

Status: accepted.

## Context

Agents write commit messages, pull request bodies, documentation, comments, and chat replies.
Only the line layout of comments had a check, so style, language, and hedging varied with each writer.
Some text named the person who asked for a change instead of the change itself.

## Decision

One checker, `prose`, judges every channel, and each enforcement point calls it.
English text runs through Vale 3.24.0 with the Google developer documentation style first, then the Microsoft, write-good, and proselint packages.
The checker counts every Vale finding of every severity as a failure, which puts each rule at error level.
A repository style named `Dotfiles` adds rules that refuse requester attribution, hedging, and platform or historical comparisons in documents.
The checker itself adds the sentence-per-line rule and a language gate that refuses non-Latin letters in persisted text.
Japanese replies run through textlint with `preset-ja-technical-writing` and through the Japanese attribution and hedging rules in `japanese.toml`.

The repository vendors the four style packages under `dot_config/prose/styles`.
`packages.toml` records each upstream archive, its digest, and a digest of the vendored tree, and `mise.toml` pins Vale.
The textlint packages carry their own `bun.lock`.
The checker refuses a missing configuration, a changed style tree, or another Vale version, and names `mise run install:prose` as the remedy.

The enforcement points cover these channels:

- `just check` and the required workflow check every Markdown document.
- The pre-commit gate checks comments, which OComment reads from each source file.
- The commit-msg gate checks the message.
- `pr-workflow check`, `create`, and `edit` check the title and body.
- The Stop hooks of Claude Code and Codex and an OpenCode plugin check the final reply and request one rewrite of a failing reply.

`policy/prose.toml` holds the exemptions, each with a reason, and a ledger of counted findings in existing prose.
A ledger count only falls: a new finding fails, and a fixed finding fails until `just prose-tighten` lowers its count.
Kani proves the ledger, exemption, and rewrite rules in `xtask/src/prose_rules.rs`.

## Alternatives

Rewriting every existing document and comment in this change would collide with parallel branches, so the ledger records that prose and keeps it from growing.
Lowering rule levels or disabling rules per file would let new prose drift, so exemptions name paths, rules, and reasons.
Downloading the packages with `vale sync` on each run depends on the network at check time, so the repository vendors them.

## Consequences

New prose in any file meets the full standard, and the ledger shrinks with each rewrite.
A reply that fails the checker gets one rewrite request, and the hook then ends the turn with the findings.
Updating a style package means vendoring the new release and recording its digests in `packages.toml`.
