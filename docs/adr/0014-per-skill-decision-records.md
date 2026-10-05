# Bind tracked skill decisions to each skill revision

Status: Accepted.
Supersedes the tracked catalog review of [ADR 0003](0003-local-skill-operations.md).

## Context

ADR 0003 bound every tracked catalog decision to one report hash over the whole catalog, the policy, and the maintenance engine sources.
Every change to a skill or an engine file had to recompute that hash, so concurrent PRs always conflicted on the same line of `docs/skills/review.json`.
Each host's local gate also asked for every catalog decision again once observations accumulated, even when the installed catalog was exactly a reviewed revision.

## Decision

Track one decision per skill in `docs/skills/decisions/<skill>.json`, bound to that skill's content revision.
The file name is the skill identity, and the record holds the revision, outcome, reason, evidence, and revisit condition of ADR 0003.
The required check refuses a skill whose decision is missing, incomplete, or bound to another revision, and a decision file that names no skill.
Each refusal names the skill and `just decide NAME`, which drafts the decision from the previous one and prints the revision's findings and the content diff since the recorded decision.
The draft keeps the previous outcome, evidence, and revisit condition and leaves the reason empty, so the check still refuses it until the author writes the reason for the new revision.

While a skill's entrypoint exceeds the policy's word limit, its decision also holds a size disposition with its own outcome, reason, and revisit condition.
The required check evaluates this against the current policy, so lowering the limit refuses every newly oversized skill without one, and raising it refuses a size disposition that is no longer required.
`just decide NAME` drafts the addition or removal of the size disposition for a decision that is otherwise current.

The tracked state no longer carries an engine or policy identity; the commit identifies the engine that checked it.
Installation records the reviewed revisions in the local manifest, excluding deferred decisions, and marks the revisions whose size disposition is settled and not deferred.
The engine and its manifest are installed together, and the engine refuses a manifest without reviewed revisions and names `mise run install:skill-ops`.
The local gate adopts a catalog finding whose skill revision matches a reviewed revision, and a size finding only when that revision's size disposition is adopted as well.
When the gate becomes pending, it analyzes the current evidence and completes if every finding is adopted, so a reviewed host needs neither `analyze` nor a disposition file.
Otherwise triage covers only the remaining findings, and a runtime disposition may still cover an adopted finding.

The pure rules for a tracked decision's state, its size disposition, adoption, triage completion, and gate completion live in `xtask/src/skill_rules.rs` with Kani harnesses and rejecting counterexamples beside the existing ones.

## Alternatives

Keeping one review file with a merge driver was rejected because a per-skill record removes the shared line instead of resolving it after the fact.
Binding each decision to the engine identity as well was rejected because an engine change does not change a skill's judgment, and the required checks already run on the engine of the commit.
Copying the previous reason into the draft was rejected because the check would then accept an unchanged judgment for changed content.
Letting the catalog decision settle the size finding was rejected because an oversized entrypoint would then need no judgment on its size.

## Consequences

PRs that change different skills touch different decision files, and a PR that changes only engine code needs no decision change.
A policy change that moves the word limit requires size dispositions only for the skills it moves across the limit.
Removing a skill requires removing its decision file.
A host whose installed catalog matches the reviewed revisions asks only about observation-driven findings and improvement notes.
A locally edited skill no longer matches its reviewed revision and returns to local triage.
