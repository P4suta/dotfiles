---
status: accepted
date: 2026-10-07
---
# Enforce formal obligations and deterministic metrics through one repository manifest

## Context

The agent instructions ask for formal verification, but the gates enforced only the Kani harnesses that `xtask/src/skill_proofs.rs` listed by hand.
No record said which verifier owns which property, how a model binds to the Rust code it describes, or which measurements may decide a merge.
The proof-gate paragraph of [ADR 0003](0003-local-skill-operations.md) describes a Linux CI proof gate on every PR that the required workflow no longer runs in that form.
Each repository on these machines owns its verification code and its gates, so this repository holds its own obligations and depends on no shared obligation tool.

## Decision

`policy/obligations.toml` lists every verification obligation of this repository, and `policy/baselines.toml` holds the deterministic metrics.
Types that deny unknown fields read both files, and `just gen` exports their JSON Schema to `schemas/`.
`just proofs` discharges the obligations as the one proof gate, and `just check` refuses an obligation set that no longer covers the code.
On Windows the gate still runs in the pinned Linux container that [ADR 0012](0012-windows-proofs-in-a-linux-container.md) describes.

Each public function of a rule module in `crates/rules` counts as a subject.
The gate discovers subjects and Kani harnesses from the source.
An unlisted harness, an unknown name, and a subject without an obligation each fail it.
A proved subject that no production code calls fails it too.

Each property has exactly one owning verifier:

| Property | Owner | Binding to the Rust code |
| --- | --- | --- |
| A pure decision over a bounded domain | Kani harnesses, each with a rejecting probe | The harness calls the production function |
| A function contract | Kani contracts, with their flags pinned beside the Kani version | `stub_verified` at each caller harness |
| A pure decision over a small finite domain | An exhaustive test over a shared `spec_*` function | The same `spec_*` function runs under Kani |
| A protocol, lock, or journal across processes | Quint, checked by `TLC`, or by Apalache for an inductive invariant or a symbolic constant | A trace replay that compares the full state after every step and covers every action |
| A relational structure such as a path tree or a target manifest | Alloy 6 | Each counterexample becomes a Rust validator test |
| A theorem no bounded checker can state | Lean 4 core, without Mathlib | A Kani equality harness or committed vectors over the Rust function |
| Test strength | cargo-mutants | Zero surviving mutants on rule modules |
| A rendered configuration that another tool reads | JSON Schema, vendored by SHA-256 | The schema validates every mapped target of every profile |
| Shell and `PowerShell` code | ShellCheck and `PSScriptAnalyzer` at pinned versions | Each runs on every rendered target it covers |

The gate refuses a model without its binding to the Rust code, because a model that no code follows proves nothing about the code.

### Exemptions

Every subject carries an obligation by default.
A subject may go without one only through an exemption that sits next to the exempted code and names one closed reason:

- `semantic-boundary`: the property depends on an external system or operating system behavior that no verifier models.
- `no-decision`: the subject passes values through with no branch and no arithmetic.
- `disproportionate`: the cost of the obligation outweighs the risk.

Each exemption names its substitute evidence, such as an integration test, a real-host check, or a recorded measurement, and the condition under which to reconsider it.
The gate refuses a free-form reason and a stale exemption.
A `no-decision` exemption goes stale once its subject gains a branch or arithmetic.
A `semantic-boundary` exemption never holds in the pure core, which reaches no external system.
Any exemption goes stale once its substitute evidence disappears.
Every `#[mutants::skip]` and every `exclude_re` entry of the mutation configuration counts as an exemption under the same rules.

### Metrics

Every gating metric stays deterministic: proved subjects and probe counts, surviving and timed-out mutants, and unsafe sites as an exact count.
The same holds for instruction counts, peak heap bytes, binary size, process counts, and distinct model states.
A metric baseline only moves toward the better value.
Measured wall time stays advisory, and the manifest types can't express a wall-time threshold.
The fixed per-harness budget of `proof_budget_rules` bounds a run instead of measuring it.
A harness past the budget counts as one that never finished, and the gate never compares two runs by time.

### Pilots and rejected tools

A pilot tries a tool on named subjects for one issue.
The four pilots try Aeneas extraction in issue 102, Kani loop contracts in issue 103, `TLC` state-graph replay in issue 104, and bolero in issue 105.
A pilot runs advisory and discharges no obligation.
The gate fails once a pilot's issue closes without a decision record that adopts, rejects, or defers the tool with its evidence.

Every rejected tool names the subject that already owns its property and one closed reason, `duplicate` or `disproportionate`:

| Tool | Competing subject | Reason |
| --- | --- | --- |
| Z3 as a subprocess | Kani for bounded arithmetic and Lean 4 for unbounded theorems | `duplicate` |
| Verus | The Kani harnesses, which Verus would need rewritten as deductive proofs | `disproportionate` |
| A hand-written `TLA+` specification | Quint, which `TLC` and Apalache check through its own translation | `duplicate` |
| proptest | Exhaustive tests, and the bolero pilot beyond the Kani bound | `duplicate` |
| Wall-time benchmarks | Instruction counts and peak heap bytes | `disproportionate` |

### Superseded and extended records

This record replaces the proof-gate paragraph of [ADR 0003](0003-local-skill-operations.md): the proofs run wherever the change classifier selects them, through `just proofs`.
It extends [ADR 0012](0012-windows-proofs-in-a-linux-container.md), whose container runs every verifier of the manifest, not only Kani.
The capacity-budget formula keeps the single final floor of [ADR 0008](0008-layered-review-capacity.md), and Kani owns its arithmetic, so this record doesn't reopen it.

## Alternatives

A shared obligation runner on every machine would let other repositories reuse the manifest format.
Each repository would then depend on this one, which the per-repository ownership of verification refuses.
Keeping the hand-written harness lists lost because a harness or rule could exist that no list named.
A free-form exemption reason lost because nothing can decide when it goes stale.

## Consequences

A new rule function needs a harness, a probe, or an exemption beside it before the gate admits it.
Adding a verifier means adding its owning row here and its type to the manifest.
Another repository meets the same rule through its own gate and its own manifest.
