# Scoping required assurance

List the affected properties, production entry points, admitted inputs, trust boundaries, and observable failures before selecting evidence.
Record one disposition for each property.

| Disposition | Required evidence |
| --- | --- |
| Native deterministic property | A machine-checked proof importing the implementation, with nonempty inputs, reachable assertions, and adequate unwinding or induction |
| Mathematical specification | A kernel-checked theorem and a checked refinement for any implementation property claimed from it |
| External semantic boundary | Proofs of the supported decision core, documented external assumptions, and native integration or fault-injection checks of the adapter |
| Empirical or heuristic claim | Proofs of deterministic invariants, reproducible measurements, planted or known-answer fixtures, and matched null or negative controls |
| Assessed verifier limitation | Concrete unsupported semantics or a demonstrated modeling limit, attempted decomposition or another verifier, mandatory substitute checks, a bounded claim, and a reconsideration condition |

A small arithmetic helper needs a complete native proof, never an exception backed by unit tests.
An HTTP adapter can prove retry classification and state transitions and test transport cancellation and the server boundary.
A probabilistic ranker under an arbitrary cap may lose correct candidates, so prove its ordering and bounds, test known recovery, and compare it with its null.
A passing four-edge model proves nothing about a production-size graph, and a rewritten Lean algorithm proves nothing about executable Rust.

Try isolation and decomposition before accepting a verifier limitation.
Pointwise proofs over an arbitrary array index establish preservation for every entry without expanding a whole-array postcondition.
Keep assumptions and exceptional paths visible, and never drop a case because it costs time.
A disproved requested property calls for a corrected contract or implementation, never a testing exception.

Record an alternative beside the contract with its reason, source binding, exact commands, result, limits, supported-core proofs, and revisit condition.
Make missing assessments, stale evidence, failed required proofs, empty proof inventories, and missing substitute checks fail the completion gate.
An exploratory theorem may stay incomplete only outside the promised contract and with a visible status.
Never report a scoped assurance pass as verification of the whole program.
