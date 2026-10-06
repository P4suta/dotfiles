---
status: accepted
---
# Keep every decision in one pure core crate behind its adapters

## Context

The rule modules sat beside their callers in seven crates.
Each compiled alone under Kani, which kept them free of other crates.
Nothing kept them from reading a file, the clock, or the environment, from panicking, or from deriving a framework trait.
Three of them returned bare text, and three derived `serde` or `clap` traits, so the core depended on adapter frameworks.
Other projects on these machines had no gate for the same structure.

## Decision

Every pure decision lives in `crates/rules`, which has no dependency and declares `#![no_std]`.
The compiler then refuses a file, process, network, clock, environment, or random read in a rule, and the crate denies the Clippy lints through which a rule could panic.
Its own `clippy.toml` refuses interior mutability.
Each owning crate re-exports its rule modules, so callers keep their paths.
Each adapter maps a core enum to its serialized or command-line name through the enum's `key`.

`xtask/src/structure.rs` refuses a core with a dependency, without `no_std`, missing a denied lint, or with a rule module outside it, through the pure `boundary_rules`.
It also runs the shared structural catalog in `home/dot_config/assure`, which every host receives at `~/.config/assure`.
The catalog's `ast-grep` rules refuse an effect, bare text, a panic, or shared state in the core paths of any repository, name their basis, and carry the corrective next action.
Each rule has fixtures that it must refuse and the corrected forms it must admit.
The `functional-architecture` skill holds the judgment the gates can't make, and `formal-assurance` loads it.

## Alternatives

A per-tool core crate would keep each rule beside its caller, but it multiplies crates without strengthening the boundary that one crate without dependencies already gives.
Disallowing each effectful method in `clippy.toml` would have to list every such path, while `no_std` refuses them all at once.

## Consequences

A rule that needs a fact from the host receives it as a value from its adapter.
A new rule module goes into `crates/rules/src` with its harnesses and an entry in the proof inventory, and the structural gate refuses one anywhere else.
