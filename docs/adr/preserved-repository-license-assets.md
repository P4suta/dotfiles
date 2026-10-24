---
status: proposed
date: 2026-10-24
---
# Preserve reviewed repository license assets

## Context

[Repository metadata](repository-metadata-and-generated-artifacts.md) generates the README and both license files from templates.
An existing dual-licensed repository can carry different license wording or extra notices.
Refreshing its README must preserve those reviewed assets.

## Decision

The default continues to generate all three files.
An optional `preserved_licenses` table records `mit_sha256` and `apache_sha256` for the existing `LICENSE-MIT` and `LICENSE-APACHE` bytes.
The parser requires both fields together.
Generation verifies both assets before writing any generated file, then writes only the README.
Checking rejects a missing asset or any digest mismatch.
The license expression, package manifests, and contributor-consent checks keep their existing requirements.

The digests bind bytes reviewed by a maintainer.
They establish neither a legal grant nor the correctness of that review.
Changing a preserved asset requires reviewing its terms and updating its recorded digest.

The configuration accepts only the two fixed license paths.
Normalizing existing terms would risk changing the reviewed grant.

## Consequences

Existing license text and notices survive a README refresh byte for byte.
Repositories using templates keep their current output.
Line-ending changes also require an updated digest.
The pure generation decision has exhaustive unit cases, a Kani harness, and rejecting probes.
Runtime tests cover preserved bytes and refusal before any generated write.
