# Tests and diagnostics

## Reproduce the mechanism

Keep the failing command, exact source, platform, tool versions, inputs, seed, and causal trace needed to reproduce an intermittent failure.
Classify the dependency: time, schedule, order, random input, shared state, external service, platform, or resource pressure.
Find the violated invariant and the boundary responsible for enforcing it.
Inspect neighboring paths under that boundary before choosing the fix.

Write a deterministic failing regression for the actual mechanism.
For a timer, drive the relevant clock across its boundary.
For a race, control the critical interleaving.
For persistence, inject a failure at the commit boundary or inspect native syscall ordering.
For a stale service result, give each result its own operation or revision identity.
Tie assertions to observable contracts, not the implementation's wording.

## Layer the evidence

Use fast pure specifications for decision rules, faithful adapter contracts for external data, and native integration checks for OS behavior.
Use property-based testing to cover domain boundaries and shrink a failure to a kept regression.
Record generated seeds, sizes, scheduler bounds, and relevant tool versions.
Use model checking or formal assurance when it establishes the affected contract, and state bounds, assumptions, and the connection to production.
Mutation testing checks whether the chosen regressions detect the broken rule.

When tests share global state or writable resources, explore order independence and isolation explicitly.
Give fixtures independent output directories, database identities, bound sockets, and configuration scopes.
A single-threaded run can help find a sharing defect, but the fix must define legitimate sharing instead of hiding the race.

Run a bounded repeat or stress check when it answers a remaining uncertainty, and record its scope.
Green reruns never prove a known intermittent failure fixed.
Never adopt retries, ignored flaky tests, larger tolerances, or a quarantined gate as a permanent resolution.

## Preserve useful diagnostics

Keep stdout and stderr distinct when their protocols differ, and preserve exit status and the primary causal error.
Include stable operation identities, phase, resource, revision, and expected versus observed state.
Capture cleanup errors without replacing the original failure.
Bound diagnostic output and redact secrets without suppressing the information that explains the failure.
Make timeout reports name the condition still pending.

Use existing project gates and native machine commands before publication.
Use `resource-coordination` for a large shared-machine campaign and `ci-budget` to keep diagnosis off hosted runs.
