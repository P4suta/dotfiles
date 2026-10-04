# Checked Pull Request Authoring

Status: Accepted.

## Context

Shared PR authoring needs consistent document checks before publication while respecting destination templates and contribution rules.
Local AI output should start as a draft, and an operational CodeRabbit integration can generate the title and summary during the normal review workflow.
The first enforcement scope is the shared skill's dedicated command; intercepting all `gh` use is outside this change.

## Decision

Extend the existing Rust xtask with a reusable document validator, a pure transition core, and the installed `pr-workflow check/create/edit/ready` entry point.
Use `local` generation by default and require explicit `coderabbit` selection for its exact standard placeholders.
Local creation always drafts; ready eligibility requires a validated, open draft.
CodeRabbit requests may retain the service placeholders until generation, but final checks reject them.
Require a repository-scoped rules URL and rationale for disabling Conventional Commits syntax, without enforcing personal section headings or title length recommendations.
The scanner checks a documented finite set of unfinished markers; semantic completeness and validation truth require execution records and agent inspection.
Use the existing Markdown parser to exclude code examples and restrict word and template detection to explicit placeholder forms, preserving ordinary prose, collapsible sections, and workflow expressions.

Read the body once, validate that owned value, and publish its private temporary copy with `gh --body-file`.
Use explicit repository and PR identities, argument vectors without a shell, and one mutation attempt with preserved failure context.
Read current remote title, body, and draft state before `ready`; GitHub's transition lacks a compare-and-swap for the inspected document, so concurrent remote edits remain an explicit external assumption.
Require readback and final live-document checking in the skill rather than claiming an atomic remote document guarantee.

The three Kani harnesses import the production transition core and quantify every operation, generation mode, validation outcome, and draft state.
Acceptance and rejection witnesses must be reachable, every expected harness must execute, and a false claim that local creation may be ready must fail with an assertion counterexample.
The existing `check:proofs` gate runs these obligations alongside the maintenance proofs in isolated model directories.
Native integration tests through the actual CLI and a compiled `gh` boundary fixture require that refused documents never start a mutation, accepted documents pass exact arguments and body bytes, live state is checked, and `gh` errors do not retry or report success.
The filesystem, Rust string routines, `gh` process, GitHub state, and CodeRabbit service are external semantic boundaries; native known-answer and fault-injection checks supplement the supported decision proofs.
This assessment does not claim to prove arbitrary Markdown semantics, natural-language quality, service availability, or factual validation statements.
Revisit the boundary evidence when the parser, process adapter, remote API, or generation protocol changes.

## Consequences

Codex, Claude Code, and OpenCode use one canonical skill and the same maintained native command.
Future ordinary-`gh` guards can reuse the validator and transition core without introducing another title or document policy.
Destination-specific exceptions remain narrow and reviewable, and publishing a generation request does not complete the PR task.
Native rollout applies only the new skill, its discovery alias, and its executable while preserving existing managed skills, client settings, and local observation history.
