---
status: amended
changed_by: [keep-the-assure-owner-map-out-of-projects.md]
date: 2026-10-07
---
# Require verification in proportion to code shape on every repository

## Context

Decision logic, protocols, and parsers reached production on these machines with nothing beyond example tests to verify them.
The Secure Software Development Framework, `NIST SP 800-218`, requires code review and testing in its practices `PW.7` and `PW.8`.
The functional safety standard `IEC 61508-3` recommends formal verification in proportion to consequence in its table `A.5`.
No gate on the development machines required either.
Each repository owns its verification code and gates, as [the obligation manifest](checked-obligation-manifest.md) records for this one.
The machines can demand verification, but can't run one shared verifier for every project.

## Decision

This repository owns the rule and its enforcement, and every repository owns its verification.

`home/dot_config/assure/shapes.toml` names each code shape these machines require a repository to verify, the signals that find it, and the marker its owner carries:

| Shape | Signal | Owner |
| --- | --- | --- |
| A pure decision | a `*_rules.rs` file | a Kani harness with a rejecting probe, in the file itself |
| A protocol or state machine | a public `step` function | a test that replays a Quint model's traces |
| A parser or decoder | a public `parse` or `decode` function, or a hand-written `FromStr` or `Deserialize` implementation | a cargo-fuzz target with a non-empty corpus that the repository's gate replays |

The signals start precise, so a site they find almost always has the shape.
They read no file under a test, fixture, or fuzz directory.
A repository names the owner of each site in `assure.toml` at its root, unless the site's own file owns it.
The same file names the deterministic budgets no change may weaken, like the count of surviving mutants.
An exemption sits in the comment block before the site as `Exempt as <reason>: <justification>`.
It names one closed reason: `semantic-boundary`, `no-decision`, or `disproportionate`.
No exemption lives outside the repository, and nothing switches the rule off for a whole repository.

`crates/rules/src/shape_rules.rs` decides each site's standing and each enforcement point's verdict, and `crates/dotfiles/src/assure.rs` gathers the facts from Git:

- A session start reports every gap with what it needs and its next action, and never refuses.
- `pre-commit` and `pre-push` refuse only a change that adds a gap, never one in a repository that merely has old gaps.
- The agent stop hook refuses to end a turn while a file the turn changed holds a gap, and continues automatically at most once.
- `pr-workflow ready` refuses a PR that adds a gap or weakens a budget, and in a repository with gaps one that closes none.

The same points apply [the anti-pattern catalog](generic-anti-pattern-catalog.md) to every file whose extension its rules read.
Each finding counts as a gap, with the rule's identifier and its note as the next action.
A gap stays the same gap across revisions while its shape, path, site text, and occurrence stay the same.
Moving code around a gap doesn't make it new.
A commit or push reads only the changed files, unless the change touches the owners file or an owner, which can change the standing of any site.

This repository meets the rule itself.
Its rule modules own their decisions, and the trace replay of `specs/pr_ready.qnt` owns the protocol of `pr_ready_rules::step`.
`fuzz/` holds a cargo-fuzz target with a corpus for each parser, which `just proofs` replays.
A fixture target whose corpus makes it panic shows that the replay can fail.

## Alternatives

A shared obligation runner installed into every repository would let the machines check the proofs themselves.
Each repository would then depend on this one, which the per-repository ownership of verification refuses.
Broad signals, such as every `Deserialize` derive, would find more sites but flag many that need nothing.
A gate that refuses on noise teaches people to exempt it.
Refusing every commit in a repository with old gaps would block all work in one that adopts the gate late.
Commits refuse only new gaps instead, and each PR must close one.

## Consequences

A repository with gaps shrinks them by one with every PR.
A new signal or shape changes what every repository must verify.
It needs fixtures in `crates/dotfiles/tests/assure.rs` that show it finds the shape and admits it once owned.
Only the budgets a repository declares hold the shapes without a signal yet, data contracts and resource-bounded paths.
