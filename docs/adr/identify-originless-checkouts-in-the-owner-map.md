---
date: 2026-10-26
status: accepted
---

# Identify originless checkouts in the owner map

## Context

A local project needs to register verification ownership before it has a published repository.
An absent origin prevented resolving an owner-side map.
Adding a fictional GitHub origin would incorrectly change the project's publication identity.

## Decision

Amend [owner-side verification maps](keep-the-assure-owner-map-out-of-projects.md) for checkouts without an origin.
Use `repos/local/<sha256>.toml` beneath the installed catalog directory.
Hash the UTF-8 bytes of the canonical checkout path to select that machine-local identity.
Keep supported GitHub origins on their existing repository keys.
Keep unsupported configured origins unsupported.
Keep the same ownership, corpus, comparison, and budget requirements.

## Consequences

Equivalent spellings of one canonical local path use the same owner map.
Distinct checkout paths use distinct identities under the SHA-256 collision-resistance assumption.
Moving an originless checkout requires moving its owner-side registration.
Private paths and registrations remain on their own machine.
The implementation uses portable Rust path and hashing APIs.
Each deployed host needs a native command check.
