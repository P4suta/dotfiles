---
name: decision-naming
description: >-
  Name designs, documents, artifacts, and code paths by the decision or property they embody, and reserve version numbers for released contracts and persisted formats that need migration.
  Use when a plan, ADR, diagram, branch, API, schema, or file is about to get a label such as `v2`, `new`, `old`, `final`, or `next`.
---

# Name by decision, not by version

A version number orders releases of a public contract.
Before a release exists, a design has no version.
It has a current state, a history in version control, and decisions that superseded earlier ones.
Labels such as `v1`, `v2`, `new`, `old`, `legacy`, `final`, and `next` carry no meaning without that history, so they age badly and force every reader to learn a timeline first.

## Rules

- Name a design, record, diagram, module, or file by the distinguishing decision or property: `poll-public-feed-data-without-accounts`, `on-demand-fetch`, `cursor-sync`, never `design-v2`.
- Keep one living design document and rewrite it in place.
  Record why it changed in a new ADR that supersedes the earlier one by name, as the `adr` skill describes.
  Version control holds the diff, and the document holds the present.
- Refer to alternatives in discussion by their property as well, such as the polling design and the on-demand design, never as `v1` and `v2`.
- Give a product, package, or service a version only at the first public release, then follow Semantic Versioning for that released contract.
- Version a format, never a design, and only when stored or transmitted data must survive a change: a database schema number, a serialized payload tag, a wire protocol.
  Keep that number next to the code that reads it.
- Put no version in an API path before release.
  After release, evolve additively, and introduce a versioned path only for an incompatible change that ships with a sunset date for the old one.
- A branch, worktree, or workspace takes the name of its change, never a sequence number.

## Before writing a label

1. A number belongs only to a released contract that it orders.
   Without such a contract, drop the number.
2. Persisted or transmitted data that a reader must interpret gets a format number at the point that reads it.
3. Everything else takes the name of its distinguishing property, chosen so that it still makes sense to someone who never saw the earlier state.

## Enforcement

Three checks that already run on this machine refuse the labels and the foreign characters, so the rule holds without recall:

- The prose checker's `Dotfiles.VersionLabel` rule refuses a bare `v2`-style label in English documents, comments, and commit messages.
- The assurance catalog pattern `version-label` refuses an identifier segment such as `V2` or `v2` in Rust and TypeScript through the `version-label` and `typescript-version-label` rules.
- The Stop hook and `dotfiles answer check` refuse any Han character outside the Japanese Industrial Standards (JIS) X 0208 and X 0213 repertoire in a Japanese reply.
  The table in `crates/dotguard/src/jis_kanji_table.rs` derives from Unihan, so a simplified Chinese character never reaches the user.

The word rules `Dotfiles.VersionLabel`, `Dotfiles.FigurativeJargon`, and `Dotfiles.ForeignTerm` in `japanese.toml` declare refusals for Chinese compounds and metaphors made of Japanese characters.
The installed reply checker doesn't read that file yet, so those compounds rely on review until it does.

## Verification

Search the change for `\bv[0-9]+\b` and for `new`, `old`, `legacy`, `final`, and `next` as name segments in paths, identifiers, titles, and headings.
Each hit must be a released contract version, a format tag beside the code that reads it, or a quotation of an external name.
Rename everything else before reporting the change complete.
File and directory names have no automated check yet, so review them by hand.
