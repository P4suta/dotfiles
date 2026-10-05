# Enforce one writing standard on persisted prose and replies

Status: accepted.

## Context

Agents write commit messages, pull request and issue bodies, documentation, comments, and chat replies.
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
- The pre-commit gate checks the language, line layout, and style of comments, which OComment reads from each source file.
- The commit-msg gate of this repository checks each message with the configuration in the checkout.
- The commit-msg hook that every repository on a host runs checks the message with the installed configuration.
- `pr-workflow check`, `create`, and `edit` check a pull request title and body, and `pr-workflow issue` checks an issue the same way.
- The Stop hooks of Claude Code and Codex and an OpenCode plugin check the final reply and request one rewrite of a failing reply.

The standard governs a destination that a personal owner holds, unless that destination forks another repository.
An external repository or a fork follows its own contribution rules, so its pull requests, issues, and commit messages skip the checker.
The commit-msg hook reads the destination from the remotes: `origin` must name a personal owner, and another remote with a foreign owner marks a fork.
It asks OpenSSH for the host name behind each SSH remote, so a host entry in the SSH configuration names GitHub too.
When the repository has no `origin`, or `origin` names no GitHub owner, such as a bare-repository hub, a personal GitHub remote names the destination.
Without such a remote, the hook reports that it skipped the check.
The repository setting `prose.standard` overrides the remotes in either direction.
The checker leaves out the text that Git generates.
That text covers the subject of a revert, a merge, or an autosquash commit, and the sentence that names a reverted or cherry-picked commit.
An offline `pr-workflow check` has no fork status, so it applies the standard to every repository of a personal owner.

`policy/prose.toml` holds the exemptions, each with paths, an optional rule list, and a reason.
Exemptions cover only text that people in this repository never write, such as the vendored style packages.
No ledger admits existing findings, so every finding outside an exemption fails.
Vale skips a document line that holds only a template action.
Kani proves the exemption, destination, and rewrite rules in `xtask/src/prose_rules.rs`.

A reply hook marks the session when it requests a rewrite.
A continued turn without that mark, such as a turn that another Stop hook continued, still gets its one rewrite request.

## Alternatives

A ledger of existing findings would keep prose that adds nothing, so this decision deletes or rewrites every existing sentence instead.
Lowering rule levels or disabling rules per file would let new prose drift, so exemptions name paths, rules, and reasons.
Downloading the packages with `vale sync` on each run depends on the network at check time, so the repository vendors them.

## Consequences

Every sentence in the repository meets the full standard.
A reply that fails the checker gets one rewrite request, and the hook then ends the turn with the findings.
OpenCode shows those findings in a notification, and it reports a missing checker with the `mise run install:prose` remedy.
The commit-msg hook refuses a commit to a personal repository until `mise run install:prose` installs the checker.
OComment reads no PowerShell script, systemd unit, or property list, so comments in those files stay outside the comment gate.
Updating a style package means vendoring the new release and recording its digests in `packages.toml`.
