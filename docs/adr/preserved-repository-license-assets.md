---
status: proposed
date: 2026-10-24
---
# Preserve reviewed repository license assets

## Context

[Repository metadata](repository-metadata-and-generated-artifacts.md) generates the README and, for the existing dual-license state, both license files from templates.
An existing licensed repository can carry different license wording or extra notices.
Refreshing its README must preserve those reviewed assets without granting new terms.

## Decision

`MIT OR Apache-2.0` remains the default and continues to generate the same three files.
Its optional `preserved_licenses` table keeps the existing `mit_sha256` and `apache_sha256` fields for `LICENSE-MIT` and `LICENSE-APACHE`.
Apache-only and Affero General Public License states use `preserved_licenses.assets` entries with a root-level `path` and SHA-256 digest.
Generation verifies every preserved asset before writing the README and never writes those existing license files.
Checking rejects a missing asset or any digest mismatch.
Package manifests must match the selected existing license state, with both operand orders accepted for the existing dual license.
Contributor-consent records remain required when generating the dual license and do not apply to preserving an existing grant.
The `unlicensed` state writes no license files and adds no README license section.

The digests bind bytes reviewed by a maintainer.
They establish neither a legal grant nor the correctness of that review.
Changing a preserved asset requires reviewing its terms and updating its recorded digest.

Preserved asset paths are limited to safe root-level file names.
Normalizing existing terms would risk changing the reviewed grant, so preservation checks exact bytes instead.

## Consequences

Existing license text and notices survive a README refresh byte for byte.
Repositories using the dual-license templates keep their current output.
Line-ending changes also require an updated digest.
The pure generation decision has exhaustive unit cases, a Kani harness, and rejecting probes.
Runtime tests cover each license state, preserved bytes, and refusal before any generated write.
