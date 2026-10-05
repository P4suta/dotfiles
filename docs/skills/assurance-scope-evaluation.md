# Proof-first scope evaluation

Every implementation change needs an assessment, and every supported deterministic property needs a source-bound proof.
An alternative counts as evidence only at a defined boundary.

| Task | Required decision | Regression evidence |
| --- | --- | --- |
| Change finite coordinate arithmetic | Import the production helper into a native proof over all admitted values | The native gate refuses missing, failed, unreachable, and stale proof results |
| Keep reciprocal assignment arrays consistent | Split the production body into all assignment-state cases and quantify an arbitrary index | Five direct Rust proofs establish exact updates and framing in 6 to 13 seconds each, where the whole-array contract timed out |
| Change a GPU heuristic with a finishing cap | Prove deterministic invariants, and require known-key recovery plus the same search on shuffled controls | The Lean counterexample refutes unconditional candidate retention, and hardware records require exact test commands and one passing test run |
| Check an HTTP or cloud adapter | Prove supported decisions, state external semantics, and require native integration and failure checks | Classification refuses a timeout as its reason, and omission, duplicates, empty rationale, and missing controls fail the assessment |
| Add a theorem about a small abstract graph | Record its model bound and implementation refinement before claiming production coverage | An exploratory four-edge theorem never discharges a required production contract or authorizes complete search |

The companion implementation in cipher-break binds each required result to its production sources.
This record makes no claim that prose checks prove instruction compliance.

## Native verification and installation

The changed catalog, aliases, resources, and tracked dispositions passed the Rust and typed-adapter checks on all three hosts.
The production maintenance contracts also passed the native Kani gate on the Mac, including the false-claim control.
Linux check job `linux:4fac1896412442e3d6ed0f209710d90e` and Windows check job `win:b00661fd51a27372eb2d188f467e2d95` passed through domyjob.

Installation finished on the Mac and through `linux:c3dfd674fd0d394e9a5a378bd82a2ab7` and `win:37303eacad6bcc8743a40d143db7e22c`.
Each host used its own chezmoi profile for the Codex, Claude Code, and OpenCode policy files, and the policy diff changed only the formal-assurance rule.
The installer kept local observation history and credentials.
