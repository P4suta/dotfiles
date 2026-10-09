---
status: accepted
date: 2026-10-09
---
# Bind incident prevention to exact records and committed evidence

## Context

Each unexpected failure needs evidence that prevents its cause from recurring.
A rule-level disposition cannot distinguish a repaired failure from a later recurrence of that rule.
CLI tests deliberately exercise refusals, so their incident records must belong to their fixtures.

## Decision

Each new disposition names one existing incident identity and its rule.
Mechanized prevention also names an immutable commit whose evidence file matches the checkout byte for byte.
The named test must pass and reproduce the rule through a structured refusal or an actual incident record.
Batch recording verifies shared commit, test, and rule evidence once, then stores each disposition under its incident identity.
Legacy rule-level dispositions remain readable and cover no incident identities.
Original incident records remain intact, and later identities remain open until their own prevention passes.

Managed CLI tests use one shared constructor that binds the incident store to an existing temporary fixture.
The constructor refuses the machine incident store and the temporary root itself.
The Git fixture constructor restores this binding after clearing inherited environment variables.
Expected-failure labels apply only to the negative cases that need them.
A regression executes an unlabelled CLI failure, requires its fixture incident, and compares the machine incident log before and after.

## Consequences

A previous prevention decision cannot silently close a future failure.
Unknown identities, mismatched rules, changed evidence, missing commits, and non-reproducing tests refuse disposition recording.
Unmatched historical incidents remain open for investigation.
The closed set of exemption reasons and every existing verification budget remain unchanged.
