---
date: 2026-10-19
status: accepted
---
# Make native verification authoritative and keep hosted CI lightweight

## Status

The owner accepted this verification boundary on 2026-10-19 under [issue 165](https://github.com/P4suta/dotfiles/issues/165).

## Context

The required native gate already checks the candidate on Mac, Linux, and Windows before push.
Hosted runs repeat implementation checks, proofs, and provisioning rehearsals after those native results.
Hosted run 39176835412 spent more than an hour in its Kani partition after setup.
The owner prioritizes verified native operation and accepts removing the independent hosted verification boundary.

## Decision

The ordinary pre-push gate remains the authority for complete native Code and Real results.
Its source binding, failure refusals, full Mac proof and mutation campaign, and fixed Linux measurement budgets remain required.
The [private phase evidence contract](bind-private-verifier-phase-evidence.md) still governs native proof reuse.
Hosted results grant no native evidence or reuse credit.

The required [workflow](../../.github/workflows/ci.yml) reports its checked-out commit and tree on Linux, Mac, and Windows.
It keeps pull-request and merge-queue events, read-only permissions, pinned checkout, and the existing required status names.
The final required job accepts only successful prerequisite jobs.
CI installs no project toolchain and runs no implementation checks, proof campaigns, or provisioning rehearsals.
The actual three-OS runner matrix remains necessary because [native admission](../../crates/dotfiles/src/hosts.rs) derives its required families from workflow runners.
The [workflow checker](../../xtask/src/quality.rs) and [regressions](../../xtask/tests/workflows.rs) protect this contract.

This amends the hosted allocation in [local Mac proof campaigns](local-proof-campaigns-on-mac.md), [fixed processor budgets](fixed-cpu-budgets-beside-mac-proof-campaigns.md), and [scoped hosted proofs](local-mutation-and-scoped-ci-proofs.md).
It supersedes [mandatory hosted proof partitions](require-every-fixed-proof-partition-in-hosted-ci.md).
The explicit `just proofs-ci` command remains available for manual verification without native publication credit.
The obsolete hosted partition-admission function and its dedicated proof obligations retire with that workflow contract.
Every remaining production rule keeps its existing proof obligations.

## Alternatives

Repeating the full campaign in hosted CI preserves an independent execution boundary and its measured delay.
Portable signed native attestations need a trusted producer, protected identity, transport, and validation lifecycle that this change doesn't introduce.
Reporting unsigned native records as independently verified CI evidence would misstate the evidence.

## Consequences

Hosted status confirms only checkout, source reporting, and successful lightweight orchestration.
It doesn't independently establish that native verification ran.
An actor who bypasses local hooks or controls the owner's native evidence namespace can evade that boundary.
The normal owner-operated workflow still requires native verification before publication and native rollout after merge.
Reconsider an independent hosted verifier when deployment policy requires an authority beyond the owner's machines.
