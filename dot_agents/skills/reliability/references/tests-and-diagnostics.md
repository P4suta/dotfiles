# Tests and Diagnostics

## Reproduce the mechanism

Retain the failing command, exact source, platform, tool versions, inputs, seed, and causal trace needed to reproduce an intermittent failure.
Classify the dependency: time, schedule, order, random input, shared state, external service, platform, or resource pressure.
Find the violated invariant and the boundary that should enforce it.
Inspect neighboring paths governed by that boundary before selecting the fix.

Use a deterministic failing regression for the actual mechanism.
For a timer, drive the relevant clock across its boundary.
For a race, control the critical interleaving.
For persistence, inject a failure at the commit boundary or inspect native syscall ordering.
For a stale service result, supply distinct operation or revision identities.
Keep assertions tied to observable contracts, not the implementation's wording or a copied algorithm.

## Layer the evidence

Use fast pure specifications for decision rules, faithful adapter contracts for external data, and native integration checks for OS behavior.
Use property-based testing to cover domain boundaries and shrink a failure to a retained regression.
Record generated seeds, sizes, scheduler bounds, and relevant tool versions.
Use model checking or formal assurance when it establishes the affected contract; state bounds, assumptions, and the connection to production.
Mutation testing can check whether the chosen regressions detect the broken rule.
A sanitizer or model's passing result covers its admitted executions and supported semantics, not every possible external system.

Explore order independence and isolation explicitly when tests share global state or writable resources.
Give fixtures independent output directories, database identities, bound sockets, and configuration scopes.
A single-threaded test run can help identify a sharing defect; the final fix should define legitimate sharing rather than simply hiding the race.
Test error propagation, cancellation, retry, restart, and cleanup only where those transitions belong to the contract.

Run a bounded repeat or stress check when it answers a remaining uncertainty and record its scope.
One green rerun or many green samples do not establish that a known intermittent failure is fixed.
Do not add retries, ignore a flaky test, enlarge tolerances, or quarantine a failing gate as a permanent resolution.
A temporary exception needs an actual task-specific decision and a clear outstanding defect.

## Preserve useful diagnostics

Keep stdout and stderr distinct when their protocols differ, and preserve exit status and the primary causal error.
Include stable operation identities, phase, resource, revision, and expected versus observed state.
Capture cleanup errors without replacing the original failure.
Bound diagnostic output and redact secrets rather than suppressing the information needed to explain failure.
Make timeout reports describe which condition remained pending.

Reuse verified evidence until a changed source, platform, environment, or unresolved concern invalidates it.
Use existing project gates and native machine commands before publication.
Use `resource-coordination` for a substantial shared-machine campaign and `ci-budget` to avoid turning diagnosis into repeated hosted runs.
Report a design-level guarantee only with the mechanism and evidence that establish it.
