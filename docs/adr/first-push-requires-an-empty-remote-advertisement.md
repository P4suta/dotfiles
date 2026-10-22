---
date: 2026-10-22
status: accepted
---
# First push requires an empty remote advertisement

## Status

Accepted.
Extends [the comparison input contract](keep-the-assure-owner-map-out-of-projects.md).

## Context

The first push to a new repository has no remote default commit to compare.
An absent `HEAD` advertisement also occurs when a populated repository points its default branch at an unborn ref.
A repository holding only tags has the same ambiguity.
Treating every missing default commit as an empty repository would change the comparison baseline.

## Decision

The push adapter queries Git for the remote default commit.
When that successful query returns exactly no bytes, it queries every advertised ref without a pattern or ref-type filter.
Only an exactly empty successful second advertisement selects the empty tree.
Any advertised ref preserves the refusal for an unavailable default commit.
Git failures, malformed default records, unavailable local commits, and merge-base failures remain errors.
A populated default advertisement must contain one full hexadecimal commit followed by a tab, `HEAD`, and a newline.
The existing explicit-commit validator and merge-base lookup still determine its comparison commit.

The normal [assurance push gate](../../crates/dotfiles/src/assure.rs) compares the complete initial source tree with the empty tree.
It still refuses every newly introduced gap, including a gap unchanged by the pushed tip's own diff.
An empty destination supplies no inherited comparison commit and grants no inheritance credit.
This adds no publication authority, issue adoption, review exception, or hook exception.

Git's successful advertisement is an external observation of the refs visible to the authenticated connection at that query.
The adapter can't establish the absence of server-hidden refs or prevent the server changing after the observation.
The existing shape-rule proofs establish the commit identifier and added-gap decisions.
The [native integration fixtures](../../crates/dotfiles/tests/assure.rs) exercise real empty, populated, tag-only, and unavailable bare destinations.
They check that an older source gap receives the ordinary added-gap refusal with an empty-tree baseline.
Adding its owning fuzz target and seed makes the complete initial tree eligible.
A change to Git's advertisement format or remote visibility assumptions requires reassessing this boundary.

## Alternatives

Refusing every first push prevents publishing a new repository through the normal gate.
Accepting a missing default advertisement without querying all refs confuses empty repositories with populated repositories whose default branch is unborn.
Using remote-tracking refs makes the decision depend on a checkout's stale local state.
Skipping assurance for an initial push would admit the entire unverified initial tree.

## Consequences

A first push can pass the same added-gap check as an update when Git establishes an empty destination.
The empty-destination path makes one extra bounded Git request.
Nonempty repositories without an available default commit still require repairing or identifying their comparison baseline.
