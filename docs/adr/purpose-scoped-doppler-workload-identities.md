---
status: accepted
date: 2026-10-21
---
# Deliver purpose-scoped credentials through workload identities

## Context

Credential consumers combine shared signing identities, release Apps, local automation, and registry authentication.
Workplace migration exposed different config names and delivery methods for the same purposes.
Preserving every existing key would retain obsolete registry credentials and duplicate GitHub writers.

## Decision

Keep required stored credentials canonical in purpose-specific shared configs.
Expose explicit field references through each consuming project's config.
Use one read-only Doppler service account per repository and purpose, with no workplace role and identity claims bound to the existing GitHub subject and immutable repository IDs.
Use native registry workload identities for supported publication flows.
Treat bootstrap credentials as separate temporary inputs and retire them after their replacement works.

Keep organization planning metadata-only and require exact identities and explicit write mode for remote mutations.
Prepare before cutover, and require source-bound consumer evidence before removing its old delivery path.
Keep credentials and operation state machine-local.
Separate authentication, signing, notarization, stapling, and publication authority and evidence.

## Consequences

Repository and purpose boundaries limit secret retrieval and remove permanent GitHub credential mirrors after cutover.
A workflow context can exchange for every identity whose claims it satisfies, so separate accounts enforce their own config scopes but don't isolate stages with matching claim policies.
Stronger stage isolation requires distinct protected subjects or reusable workflow identities.
A retrieved shared App key or signing credential still has its shared authority, so this does not isolate the underlying identity between repositories.
Identity capacity and temporary migration overlap must fit the current plan.
Environment protection remains authoritative, and a pending human gate remains unverified.

Whole-config inheritance would expose unrelated fields.
A wildcard service account would merge repository trust boundaries.
Native secret sync remains a supported alternative for consumers without workload identity, with one selected delivery method per consumer.
An identity migration or successful authentication never authorizes a public release.

The production organization decisions live in `crates/rules/src/doppler_organization_rules.rs` and their source-bound specifications in `xtask/tests/doppler_organization.rs`.
The shared `doppler` credential lifecycle reference defines the reusable operating procedure.
