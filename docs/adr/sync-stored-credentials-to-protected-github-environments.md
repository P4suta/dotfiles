---
status: proposed
date: 2026-11-08
---
# Sync stored credentials to protected GitHub environments

## Context

Repository and purpose identities approach Doppler's [50-identity limit](https://docs.doppler.com/docs/platform-limits).

## Decision

Use [native GitHub synchronization](https://docs.doppler.com/docs/github-actions) when consumers can share a protected environment.
Keep canonical Doppler credentials behind purpose-specific references and GitHub-compatible names.
Require exact repository identities, restricted deployment branches, and one sync per environment.
Pass individual secrets only to consuming steps.
Keep native provider OIDC and necessary Doppler workflow restrictions.

## Alternatives

Retaining Doppler OIDC preserves claim restrictions but needs capacity relief.
Service tokens add another stored credential to rotate.

## Consequences

Native sync creates long-lived GitHub copies and replaces Doppler claim checks with environment protection.
The [identity decision](purpose-scoped-doppler-workload-identities.md) still governs consumers awaiting review.
Reconsider when providers support direct OIDC or consumers need stronger claim restrictions.

## Confirmation

Plan from metadata.
Require explicit writes and reject collisions, scope changes, unrestricted environments, and exhausted capacity.
Verify initial synchronization and authentication from reviewed main before retiring identities.
Migration permits no signing or publication.
