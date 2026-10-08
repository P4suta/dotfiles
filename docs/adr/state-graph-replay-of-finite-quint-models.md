---
status: accepted
date: 2026-10-07
---
# Replay the whole reachable state graph of each finite Quint model

## Context

A Quint obligation binds a model to its Rust step function by replaying traces.
`specs/pr_ready.qnt` commits eight sampled traces, so a step that diverges from the model on a state no trace visits passes the replay.
The pilot of issue 104 tried the action-labelled state dump of `TLC` on the subjects the manifest names, `pr_ready` and `reaper_lock`.
`reaper_lock` has no model yet, because issue 77 adds it, so the pilot ran on `pr_ready` alone.

The pilot measured:

- The `TLC` that the pinned Apalache 0.62.1 bundles dumps the fixed model with `-dump dot,actionlabels` in 3 s on one worker.
- The dump holds 971 states and 1,927 edges, and `TLC` reports 1,928 generated states with one initial state.
  Every generated transition appears as exactly one edge, 227 of them self-loops.
- Each edge label names its action and the report CI chose, such as `reportAs([Complete |-> ...])`, so the replay needs no search over parameters.
- The node names, fingerprints that change between runs, drop out of the sorted form `just graphs` writes, which came out byte-identical across two runs.
- Replaying every edge through `pr_ready_rules::step` and comparing the full next state passes, as does requiring an edge for every action the step enables from a reachable state.
- A step that reads a failed head's checks as passed when every head failed passes all eight committed traces, which never visit that state, and fails the graph replay.
- The committed graph takes 546 KB, and the replay takes 0.02 s under `cargo test` on every host with no container.

## Decision

Every finite Quint model records its reachable state graph, and a test replays it edge for edge.

- The Quint obligation names the graph in `graph`, and the manifest gate refuses a missing named graph.
- `just graphs` compiles the model to `TLA+`, dumps it with `TLC` in the pinned image offline, and writes the states sorted by value with each edge as `(from, action, to)`.
- `just proofs` dumps the graph again and refuses one that differs from the committed file, or whose edge count differs from the transitions `TLC` generated.
- The model's replay test in its crate decodes each state, replays every edge through the step function, and requires an edge for every enabled action.
  The step reaches nothing the model lacks.
- The sampled traces stay, because they also exercise the `ITF` decoding and the witnesses of each outcome.

`reaper_lock` records its graph when issue 77 adds the model.

## Alternatives

More sampled traces raise the chance of visiting a state, but never reach every one, and grow the committed files faster than the graph does.
A Rust search of the step function's own reachable states, which `pr_ready_rules` keeps, holds the implementation to its own invariant, not to the model's.
Dropping the replay would leave the divergence the pilot built undetected.

## Reconsider

Reconsider when a model's graph outgrows a reviewable diff, about 10 MB, or `TLC` can no longer dump it within the proof budget.
Such a model keeps its sampled traces and records why it has no graph.
