---
status: proposed
date: 2026-10-26
---
# Accept completed heads with prior full-review evidence

## Context

CodeRabbit can complete a clean draft review without submitting a review object on that head.
It can also report no files after a rebase or submit an empty approval for an already-reviewed head.
The owner observed these responses on [Hellooo PR 6](https://github.com/P4suta/Hellooo/pull/6).
Requiring a complete review body on every head rejects those responses.
The [review routing decision](route-coderabbit-by-current-pr-importance.md) still requires current-head completion and complete unresolved-thread inspection.

## Decision

Keep the existing acceptance path for a complete current-head `COMMENTED` or `APPROVED` review.
For a missing current-head review or an empty current-head approval, select the CodeRabbit full-review body with the highest review ID on that PR.
Validate its full metadata, reviewed commit, canonical PR base, and coverage of every assessment evidence path.
Accept prior `CHANGES_REQUESTED` reviews only after the thread gate confirms resolution of every CodeRabbit finding.
Refuse dismissed reviews and incomplete metadata.

Require the current head's CodeRabbit status with the highest status ID to report `success` and `Review completed`.
Require its creation timestamp to be greater than the full review's submission timestamp.
Reject equal timestamps.
Parse timestamps as instants, including their offsets.
Issue comments alone supply no review completion evidence.

Read [GitHub's compare API](https://docs.github.com/en/rest/commits/commits#compare-two-commits) for the reviewed and current commits.
Inspect both directions when the reviewed head has commits absent from the current head.
Require every changed path, including both names of a renamed file, to belong to the full review's selected set.
An empty comparison satisfies that condition.
Refuse an unavailable inventory or an inventory reaching the API's 300-file ceiling because GitHub may truncate it.

A rebase can introduce upstream files into the comparison while preserving the PR diff.
Also accept an identical canonical PR diff under the existing binary, full-index, no-renames SHA-256 capture.
Fetch the reviewed commit and recompute its canonical merge base before comparing that digest with the current assessment's digest.
Keep the complete API inventory and timestamp requirements for this case.
This equality check covers the PR change itself when upstream changes make the head trees differ.

Keep all existing pause, assessment, thread pagination, and live-head checks.
The [adapter tests](../../crates/pr-workflow/src/review_policy.rs) cover missing objects, empty approvals, metadata, timestamps, and refused scope.
The [workflow tests](../../crates/pr-workflow/tests/pr_workflow.rs) exercise the compare requests, rename paths, divergent histories, actual rebase diff equality, and unresolved threads.
The [pure decision](../../crates/rules/src/review_routing_rules.rs) has unit specifications, a Kani harness, and rejecting probes in the proof inventory.

## Consequences

Clean heads can complete the workflow without manufacturing a provider review object.
The workflow still trusts CodeRabbit's authenticated terminal status to describe its actual analysis of the current head.
Metadata, path coverage, chronology, and diff equality constrain that trust but can't prove the service's empirical analysis.
The Kani obligation checks the decision over supplied facts and doesn't certify GitHub responses, timestamp parsing, or the provider's behavior.
Historical evidence never overrides an unresolved finding or an incomplete comparison.

Accepting any successful status alone would omit the prior full-review scope and chronology requirements.
Accepting an empty approval alone would omit them too.
Requiring another full-review object would preserve the defect because the service may repeat its object-free response.
