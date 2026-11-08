---
date: 2026-11-09
status: proposed
---

# Bind tool lookups to discovery metadata

## Context

The gate binds unsupported `http:` tools to a verified counterpart.
The existing adapter accepts one flat GitHub release-discovery shape.
Platform downloads and tags fail that binding despite successful lookups.

## Decision

Decode HTTP discovery into a closed source type before comparing the counterpart.
Support the existing GitHub release endpoint and the GitHub tags endpoint with their exact JSON selectors.
Require semantic ordering for tags.
Validate platform downloads and extraction options without discovery expressions.
Bind one successful JSONata counterpart to the declared repository, source kind, and version in the offered TOML.
Keep warnings, missing or duplicate counterparts, source mismatches, and unverified lookups as refusals.

## Alternatives

Keeping the release-only adapter blocks valid tag sources.
Ignoring unsupported tools loses lookup coverage.
Trusting a counterpart without source binding permits unrelated evidence.

## Consequences

The adapter owns more of [mise's HTTP metadata](https://mise.jdx.dev/dev-tools/backends/http.html) contract.
Renovate still performs remote lookups, and the pure freshness decision stays unchanged.
Reconsider when Renovate supports `http:` directly or either producer changes the metadata contract.

## Confirmation

Run rejecting source-binding fixtures and the existing release regressions.
Check the actual webpkit candidate through the normal local lookup and push gate.
