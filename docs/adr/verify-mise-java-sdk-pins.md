---
date: 2026-11-12
status: proposed
---

# Verify declared `java` versions

## Context

Renovate reports the Microsoft Java declaration as an unsupported datasource.
Mise publishes the distribution's complete versions and download metadata in its [SDK catalog](https://mise-java.jdx.dev/jvm/ga/linux/x86_64.json).
The missing lookup blocks the recovered signing helper.

## Decision

Add a Microsoft Java lookup through Renovate's [custom datasource](https://docs.renovatebot.com/modules/datasource/custom/) interface.
Use the catalog's stable Java Development Kit records and preserve every numeric version component.
Accept the unsupported Mise record only when its unique lookup matches the committed literal, distribution, and catalog origin.
Keep lookup failures and outdated versions blocked.
Reject project aliases and unsupported declaration forms.

## Alternatives

Ignoring the unsupported record removes freshness verification.
Reading another vendor's Java versions loses Microsoft build revisions.
Changing the project runtime leaves the shared integration gap.

## Consequences

The guard depends on the same public catalog as Mise.
Other distributions remain unsupported.
Reconsider when Renovate supports the declaration directly or the catalog changes.

## Confirmation

Require successful and rejecting report fixtures, a real catalog lookup, and the helper's checked push.
Owner acceptance remains pending.
