---
date: 2026-11-10
status: proposed
---

# Bind official tarballs to npm workspace lockfiles

## Status

Proposed.

## Context

Renovate can omit the locked version for an official tarball while reporting a successful custom-manager lookup.
Requiring that field rejects valid npm workspaces.

## Decision

When the report omits its locked version, read the producer-declared npm lockfile from the offered Git revision.
Require the manifest declaration, workspace link, resolved publisher URL, and locked version to agree with the verified lookup.
Use npm's [package-location graph](https://docs.npmjs.com/cli/v11/configuring-npm/package-lock-json/) and Node's [module-resolution order](https://nodejs.org/api/modules.html#all-together).
Refuse conflicting ownership, unsupported formats, and incomplete data.
Keep explicit report versions authoritative and preserve all existing freshness checks.

## Alternatives

Rejecting every omitted field blocks valid projects.
Trusting the URL alone leaves the installed dependency unverified.

## Consequences

The adapter supports npm lockfile formats 2 and 3 without changing the core admission policy.
Unknown formats fail closed.
Reconsider the adapter when npm changes its lock graph.

## Confirmation

Replay the workspace report and reject altered versions, URLs, links, and Git revisions.
Exercise the installed push gate after repository checks pass.
