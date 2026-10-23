---
status: proposed
date: 2026-10-24
---
# Preserve shell argument source spellings

## Status

Proposed.

## Context

Decoded shell words lose the quoting that controls tilde expansion, variables, whitespace, and empty arguments.
Reconstructing a suggestion from those values can change the arguments a command receives.
Redirection operands also need a distinct representation from command arguments.
The existing profiles already pin shfmt for shell tooling.

## Decision

Use the pinned parser's [typed JSON](https://github.com/mvdan/sh/blob/v3.14.1/cmd/shfmt/shfmt.1.scd) with an explicit Bash dialect.
Send the source through standard input without executing it.
Bound input, output, and process occupancy, and report unreadable results as incomplete inspection.
Validate each word's byte positions and UTF-8 boundaries against the same immutable source before copying its original spelling.
Keep classification values separate from those source slices in the [syntax adapter](../../crates/dotfiles/src/legacy/syntax.rs).
Treat the parser's grammar and [position semantics](https://pkg.go.dev/mvdan.cc/sh/v3/syntax#Pos.Offset) as external assumptions.

Copy argument spellings through template replacements and the supported grep and find transformations.
Require a recognized literal before splitting or modifying an attached option value.
Keep source ordinals with copied nonliteral words, and give guidance if a translation changes their order.
Keep dynamic short-option names out of literal flag clusters.
The [Bash 3.2 manual](https://github.com/apple-oss-distributions/bash/blob/main/bash-3.2/doc/bash.1) defines the quoting and expansion assumptions.
Give tool guidance when a safe transformation is unavailable.
Keep PowerShell on its own syntax boundary and give guidance when its argument spelling lacks verification.
Continue inspecting stdin scripts, inline scripts, and literal files within the existing depth limit.
Keep known refusals when another inspection cannot finish.

The [range and order rules](../../crates/rules/src/legacy_rules.rs) own nonempty source bounds and strictly increasing copied ordinals under observed facts.
Their source-bound harnesses specify those conditional decisions.
Native [Bash argument observations](../../crates/dotfiles/tests/legacy_suggestions.rs), malformed metadata tests, and the registered decoder corpus cover distinct adapter boundaries.
The caller retains the shell, environment, working directory, and expansion context of the original invocation.
These checks establish no universal theorem about parser correctness, expansion environments, or tool option equivalence.

## Consequences

Suggestions preserve source spelling without evaluating user expansions.
Bash inspection depends on the existing pinned parser, and a missing parser leaves inspection incomplete.
Some compound option spellings receive guidance instead of an executable suggestion.
Parser updates need the source-position and native argument regressions.
Heavy proof and fuzz campaigns remain at the release gate.

Reconstructing quoted words from decoded values cannot recover their original expansion context.
Evaluating the source to discover arguments would execute its substitutions and other effects.
