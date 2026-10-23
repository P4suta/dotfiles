---
status: accepted
date: 2026-10-23
---
# Keep independent work reasons in owner-local state

## Context

The [stack decision](stack-first-pull-requests-through-a-merge-queue.md) requires a reason for independent work and places it in both local state and the public PR body.
The reason explains the owner's allocation of work and can include context outside the proposed patch.
Public PR documents need the change's problem, behavior, and validation.
Work admission needs a checked local declaration without publishing that private context.

## Decision

Store independent-work reasons only in owner-local state.
Keep the existing stack decision's work-unit limit and merge behavior.
Both `start --independent <reason>` and `create --independent <reason>` declare an independent unit.
Bind the declaration to the authenticated login, destination and source repository identities, branch, issue, and starting commit.
A declaration from `start` applies to descendant commits on that branch for the same issue.
After creation, bind it to the returned PR number so a reused branch name doesn't inherit the old declaration.
Listings and refusals name the local reason, while PR bodies and GitHub arguments carry no copy of it.

The [work adapter](../../crates/pr-workflow/src/work.rs) owns the private records and live inventory.
The [work-unit rules](../../crates/rules/src/wip_rules.rs) validate the stack and decide whether another unit fits.
An independent declaration keeps the normal issue, review, dependency, and required-check gates.

## Consequences

The owner can inspect why work left the stack without publishing the explanation.
Records remain local to each host.
Another host must record its own declaration for an existing independent PR before admitting more work when the inventory lacks that evidence.
Malformed or mismatched records fail admission, and incomplete or ambiguous live inventories supply no work-unit credit.

## Alternatives

Publishing the reason in the PR body distributes it through GitHub, but exposes owner workflow context beyond the patch.
A shared private state service would distribute declarations across hosts, but adds a service, credentials, and a new availability dependency.
