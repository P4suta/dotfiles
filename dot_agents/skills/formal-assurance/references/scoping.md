# Scoping Required Assurance

Start from the behavior the user needs and the guarantee the implementation promises.
List the affected properties, production entry points, admitted inputs, trust boundaries and observable failures before selecting evidence.
The purpose is to prevent incorrect behavior and wasted work; increasing theorem count is not an outcome.

For each property, choose and record one of these dispositions.

| Disposition | Required evidence |
| --- | --- |
| Native deterministic property | A machine-checked proof importing the implementation, with nonempty inputs, reachable assertions and adequate unwinding or induction |
| Mathematical specification | A kernel-checked theorem and an explicit, checked refinement for any implementation guarantee claimed from it |
| External semantic boundary | Proofs of the supported decision core, documented external assumptions, and native integration or fault-injection checks of the adapter |
| Empirical or heuristic claim | Proofs of deterministic invariants, reproducible measurements, planted or known-answer fixtures, and matched null or negative controls where applicable |
| Assessed verifier limitation | Concrete unsupported semantics or a demonstrated modeling limitation, attempted decomposition or another suitable verifier, mandatory substitute checks, a bounded claim and a reconsideration condition |

A small arithmetic helper normally needs a complete native proof, not an exception backed by a few unit tests.
An HTTP adapter can prove retry classification and state transitions while testing actual transport cancellation and the documented server boundary.
A probabilistic ranker cannot promise to retain every correct candidate under an arbitrary cap; prove its ordering and bounds, test known recovery and compare the same procedure with its null.
A four-edge model does not prove a production-size graph merely because it passes, and a separately rewritten Lean algorithm does not automatically prove executable Rust.

Attempt meaningful isolation and decomposition before accepting a verifier limitation.
Pointwise proofs over an arbitrary array index can establish preservation for every entry without expanding a whole-array postcondition at once.
Keep assumptions and exceptional paths visible; a failed case cannot be removed simply because it is expensive.
A disproved requested guarantee requires correcting the contract or implementation rather than registering a testing exception for that same guarantee.

Record an alternative beside the contract with its reason, source binding, exact commands, result, limits, supported-core proofs and condition for revisiting it.
Review this assessment with the implementation; ordinary technical classification does not require a new permission question when the task already authorizes the work.
Make missing assessments, stale evidence, failed required proofs, empty proof inventories and missing substitute checks fail the completion gate.
An exploratory or stronger theorem may remain incomplete only when it is outside the explicitly promised contract and its status remains visible.
Never report a scoped assurance pass as complete verification of the entire program.
