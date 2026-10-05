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
Try isolation and decomposition before accepting a verifier limitation.
A disproved property calls for a corrected contract or implementation, never a testing exception.

Record an alternative beside the contract with its reason, source binding, exact commands, result, limits, supported-core proofs, and revisit condition.
Make missing assessments, stale evidence, failed required proofs, empty proof inventories, and missing substitute checks fail the completion gate.
Never report a scoped assurance pass as verification of the whole program.
