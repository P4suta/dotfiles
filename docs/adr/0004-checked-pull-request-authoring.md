# Checked pull request authoring

Status: accepted.

## Context

Local AI output starts as a draft, and a CodeRabbit integration can generate the title and summary during review.
The first enforcement scope covers the shared skill's command, not every `gh` call.

## Decision

Add a document checker, a pure transition core, and the installed `pr-workflow check/create/edit/ready` entry point to the Rust xtask.
Default to `local` generation, and require an explicit `coderabbit` selection for its standard placeholders.
Local creation always makes a draft, and `ready` accepts only a checked, open draft.
A CodeRabbit request may keep the service placeholders until generation, and final checks refuse them.
Disabling Conventional Commits syntax needs a repository-scoped rules URL and a rationale.
The checker enforces no personal section headings or title length.
The scanner checks a documented finite set of unfinished markers.
Parse Markdown to skip code examples, and match only explicit placeholder forms.

Read the body once, check that owned value, and publish its private temporary copy with `gh --body-file`.
Use explicit repository and PR identities, argument vectors without a shell, and one mutation attempt that keeps the failure context.
Read the current remote title, body, and draft state before `ready`.
GitHub offers no compare-and-swap for that transition, so concurrent remote edits stay an external assumption.
The skill requires readback and a final live-document check.

Three Kani harnesses import the production transition core and quantify every operation, generation mode, check outcome, and draft state.
Acceptance and refusal witnesses must stay reachable, every expected harness must run, and a false claim that local creation may skip the draft must fail with a counterexample.
The `check:proofs` gate runs these obligations beside the maintenance proofs in isolated model directories.
Native tests drive the CLI through a compiled `gh` fixture.
A refused document never starts a mutation, an accepted one passes exact arguments and body bytes, and a `gh` error neither retries nor reports success.
Nothing here proves Markdown semantics, prose quality, service availability, or the truth of a validation statement.
Revisit the boundary evidence when the parser, process adapter, remote API, or generation protocol changes.

## Consequences

A later guard on ordinary `gh` calls can reuse the checker and transition core.
A generation request leaves the PR task open.
Native rollout applies only the new skill, its discovery entry, and its executable.
