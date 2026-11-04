---
status: proposed
date: 2026-10-24
---
# Bind copyedit decisions to exact reports

## Context

Pinned linguistic models and grammar rules produce reproducible candidates, including false positives caused by token boundaries and grammatical role assignments.
Publication needs explicit dispositions without hiding whole rules or carrying a decision into changed prose.
A recorded editorial decision doesn't prove grammatical or factual truth.

## Decision

Keep all diagnostics in the immutable copyediting report.
An optional decision file identifies each retained review candidate and records a reason and a literal source quote covering its location.
Bind that file to the complete typed report's digest.
Reject missing, duplicate, unknown, pending, blank, or unevidenced decisions and any error-severity diagnostic.
The default verification path continues to require zero diagnostics.

After checking binding and source bytes, repeat the applicable engines.
Require identical coverage, diagnostics, linguistic metadata, and annotations.
Compare grammar results independently of recorded execution dates while preserving engine, active-rule, annotation, and calibration equality.
Changed source bytes, configuration, rules, producer builds, or results invalidate the prior decision set.
Any such change removes the retention.
It grants no exception to a future document or report.

The [pure decision core](../../crates/rules/src/copyedit_rules.rs) decides whether review coverage is complete.
Kani checks that function over its admitted value domain.
The proof includes errors and stale binding.
The [native integration test](../../crates/prose/tests/copyedit.rs) exercises actual engines, quoted evidence, tampered reports, and changed files.
SHA-256 collision resistance, external engine behavior, source interpretation, and the truth of a review reason remain trust boundaries.

## Alternatives

Disabling an upstream rule would conceal future real defects.
Automatically retaining all candidates would replace a refusal with an unexamined success.
Changing prose only to appease a mistaken parser would damage valid names and constructions.
An exact report binding limits a decision without any of those effects.

## Consequences

Retained candidates stay inspectable and never count as a diagnostic-free report.
Each new pass needs new decisions, which costs review effort but prevents silent reuse.
Reason and source-quote validation establishes presence and binding, not the correctness of human linguistic interpretation.
Reconsider this boundary when a qualified rule or model can resolve a candidate deterministically without regressing accepted and rejected controls.
