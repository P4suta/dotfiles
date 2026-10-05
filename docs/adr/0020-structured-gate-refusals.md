# Report every gate refusal as one structured record

Status: Accepted.

## Context

The Git wrapper, the commit and push hooks, and `pr-workflow` each refused in their own prose.
An agent that was refused had to read that prose, infer which rule applied, and work out the next command by itself.
Some refusals named no next command at all, and none could be parsed without matching their wording.

## Decision

Every gate refusal is one record with five fields: the rule, the cause, the evidence, the next command, and the waiver.
The rule is a stable dotted identifier such as `git.force` or `pr.issue`.
The evidence lists the observed facts the refusal rests on, such as the refused command, a commit, or a marker path.
The next command is one runnable line; `<name>` marks a value only the author can supply.
A next command never does something other than what the refused command intended: when the gate cannot rebuild such a command, `next` is null and the cause says why.
A suggested Git command passes the gate that suggested it, however many refused parts the original line combined.
The gates read the options of `git push` and `git commit` as Git reads them, including a unique prefix of a long option and a cluster of short options, so a suggested command never takes an option value for a branch.
A refused line with an option Git rejects as written gets no suggestion, because its other words may not mean what the gate read.
The waiver repeats exactly the refused operation with a recorded one-time override, and is absent when the gate cannot be waived, its override is not recorded, or the refused operation is unknown.
The Git wrapper passes the command it runs to that command's hooks, so a hook's waiver repeats the Git command that triggered it, and a refused commit message's next step repeats that commit with the saved message opened for editing.
That retry keeps what the dropped message options implied: an `amend:` or `reword:` fixup may be empty, a reword leaves staged changes out, and `-C` or `-c` copies the author and date of the reused commit.
During a pick Git takes the author from the picked commit instead, and `--reset-author` from the committer; without the reuse and `--amend` the committer is already the author, so the retry drops `--reset-author`, which Git would reject there.
When the gate cannot read that authorship or tell whether a pick is in progress, the step suggests no command.
A hook run by a Git that bypassed the wrapper names the override in its cause instead, and its next step marks the unknown options of the commit with `<options>`.
A push hold has no waiver, because only the owner lifts it.

A refusal prints readable text followed by exactly one machine-readable line on standard error:

```text
dotfiles-refusal/1 {"rule":"...","cause":"...","evidence":["..."],"next":"...","waiver":null}
```

The line is the versioned prefix and a compact JSON object with the five keys in this order, so an agent reads the last line that starts with the prefix.
`next` and `waiver` are strings or null.
The format lives once in `guard/src/refusal.rs`, which is std-only so `dotguard` and the xtask binaries compile the same source.

The Git wrapper denials, the empty-commit and signing rollbacks of `post-commit`, the history and signature gates of `pre-push`, the language and dependency gates, the push hold, staged-tree, and lefthook gates of the hook dispatcher, and the `pr-workflow` prerequisites emit this record.
The hook dispatcher relays a child gate's record as the last line, and reports a child that failed without one as `hook.gate`.
A `pr-workflow` failure that no specific rule describes is reported as `pr.failed` with the subcommand's help as its next step, through the standalone binary and through xtask alike.
The choices of a signing step and of a history step, the reading of a long option name, whether a refusal suggests a command, and what a commit retry adds are pure functions with Kani harnesses.
Tests run each adopted gate as installed and require a complete record as the last line of its standard error.
Two records are tested where they are built, because their condition depends on the host: the dependency gate needs its lookup container, and a missing lefthook cannot be arranged where the fixed tool directories hold one.

## Alternatives

Keeping prose and documenting how to parse it was rejected because wording changes would silently break every consumer.
Emitting only JSON was rejected because the same output is read by people at a terminal.
A JSON dependency in `dotguard` was rejected because the crate stays dependency-free and the record has a fixed shape that a small std-only writer and reader cover exhaustively.
GitHub workflow-command annotations were rejected because they carry no structure beyond a message and a location.

## Consequences

An agent acts on a refusal by running `next`, or by reading the cause when `next` is null, and applies `waiver` only when the owner has authorized it.
A new gate adopts the format by building a `Refusal` and emitting it instead of printing prose.
Changing a field's meaning or the line's shape requires a new prefix version.
