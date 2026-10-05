# Report every gate refusal as one structured record

Status: accepted.

## Context

The Git wrapper, the commit and push hooks, and `pr-workflow` each refused in their own prose.
An agent that met a refusal had to read that prose, infer which rule applied, and work out the next command itself.
Some refusals named no next command at all, and none could parse without matching its wording.

## Decision

Every gate refusal holds five fields: the rule, the cause, the evidence, the next command, and the waiver.
The rule has a stable dotted identifier, like `git.force` or `pr.issue`.
The evidence lists the observed facts behind the refusal, like the refused command, a commit, or a marker path.
The next command takes one runnable line, and `<name>` marks a value only the person at the terminal can supply.
A next command never does something other than what the refused command intended.
When the gate can't rebuild such a command, `next` stays null and the cause says why.
A suggested Git command passes the gate that suggested it, even when the original line combined more than one refused part.
The gates read the options of `git push` and `git commit` the way Git reads them, including a unique prefix of a long option and a cluster of short options.
A suggested command never takes an option value for a branch.
A refused line with an option Git rejects as written gets no suggestion, because its other words may mean something else than the gate read.
The waiver repeats exactly the refused operation with a recorded one-time override.
It stays absent when the gate can't waive the refusal, when the override goes unrecorded, or when the refused operation stays unknown.
The Git wrapper passes the command it runs to that command's hooks.
A hook's waiver repeats the Git command that triggered it.
The next step after a refused commit message repeats that commit with the saved message open for editing.
That retry keeps what the dropped message options implied.
An `amend:` or `reword:` fixup may stay empty, a reword leaves staged changes out, and `-C` or `-c` keeps the attribution and date of the reused commit.
During a pick, Git takes attribution from the picked commit, and `--reset-author` takes it from the committer.
Without the reuse and `--amend`, the committer already holds that attribution, so the retry drops `--reset-author`, which Git would reject there.
When the gate can't read that authorship or tell whether a pick runs, the step suggests no command.
A hook that a Git bypassing the wrapper ran names the override in its cause, and its next step marks the unknown options of the commit with `<options>`.
A push hold has no waiver, because only the owner lifts it.

A refusal prints readable text, then exactly one machine-readable line on standard error:

```text
dotfiles-refusal/1 {"rule":"...","cause":"...","evidence":["..."],"next":"...","waiver":null}
```

The line holds the versioned prefix and a compact JSON object with the five keys in this order, so an agent reads the last line that starts with the prefix.
`next` and `waiver` hold strings or null.
The format lives once in `guard/src/refusal.rs`, which uses only the standard library, so `dotguard` and the xtask binaries compile the same source.

These gates emit the record:

- the Git wrapper denials
- the empty-commit and signing rollbacks of `post-commit`
- the history and signature gates of `pre-push`
- the language and dependency gates
- the push hold, staged-tree, and lefthook gates of the hook dispatcher
- the `pr-workflow` prerequisites

The hook dispatcher relays a child gate's record as the last line.
It reports a child that failed without one as `hook.gate`.
A `pr-workflow` failure that no specific rule describes becomes `pr.failed` with the subcommand's help as its next step.
The standalone binary and xtask report it alike.
Pure functions with Kani harnesses decide these choices:
- the signing step
- the history step
- the reading of a long option name
- whether a refusal suggests a command
- what a commit retry adds
Tests run each adopted gate as installed and require a complete record as the last line of its standard error.
Two records get tested where the code builds them, because their condition depends on the host.
The dependency gate needs its lookup container, and a missing lefthook can't arise where the fixed tool directories hold one.

## Alternatives

Keeping prose and documenting how to parse it loses because wording changes would break every consumer.
Emitting only JSON loses because people at a terminal read the same output.
A JSON dependency in `dotguard` loses because the crate stays dependency-free, and the record has a fixed shape that a small writer and reader cover exhaustively.
GitHub workflow-command annotations lose because they carry no structure beyond a message and a location.

## Consequences

An agent acts on a refusal by running `next`, or by reading the cause when `next` holds null.
It applies `waiver` only when the owner has authorized it.
A new gate adopts the format by building a `Refusal` and emitting it instead of printing prose.
Changing a field's meaning or the line's shape needs a new prefix version.
