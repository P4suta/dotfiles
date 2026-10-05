# Bind tracked skill decisions to each skill revision

Status: accepted.
Supersedes the tracked catalog review of [ADR 0003](0003-local-skill-operations.md).

## Context

ADR 0003 bound every tracked catalog decision to one report hash over the whole catalog, the policy, and the maintenance engine sources.
Every change to a skill or an engine file recomputed that hash, so concurrent PRs always conflicted on the same line of `docs/skills/review.json`.

## Decision

Track one decision per skill in `docs/skills/decisions/<skill>.json`, bound to that skill's content revision.
The filename gives the skill identity, and the record holds the revision, outcome, reason, evidence, and revisit condition of ADR 0003.
The required check refuses a skill whose decision has gone missing, lacks a field, or names another revision, and a decision file that names no skill.
Each refusal names the skill and `just decide NAME`, which drafts the decision from the previous one.
The command also prints the revision's findings and the content diff since the newest commit with the revision that the last decision assessed.
The draft keeps the previous outcome, evidence, and revisit condition and leaves the reason empty, so the check refuses it until someone writes the reason for the new revision.

While a skill's entrypoint exceeds the policy's word limit, its decision also holds a size disposition with its own outcome, reason, and revisit condition.
The required check reads the current policy, so lowering the limit refuses every newly oversize skill without one, and raising it refuses a size disposition that no skill still needs.
`just decide NAME` drafts the addition or removal of the size disposition for an otherwise current decision.

The tracked state carries no engine or policy identity, because the commit identifies the engine that checked it.
Installation records the reviewed revisions in the local manifest, leaves out deferred decisions, and marks the revisions with a settled, undeferred size disposition.
The engine and its manifest install together, and the engine refuses a manifest without reviewed revisions and names `mise run install:skill-ops`.
The local gate adopts a catalog finding whose skill revision matches a reviewed revision, and a size finding only when it has also adopted that revision's size disposition.
When the gate turns pending, it analyzes the current evidence and completes if it adopts every finding, so a reviewed host needs neither `analyze` nor a disposition file.

The pure rules for a tracked decision's state, its size disposition, adoption, triage completion, and gate completion live in `xtask/src/skill_rules.rs` beside their Kani harnesses and refusing counterexamples.

## Alternatives

One review file with a merge driver lost because a per-skill record removes the shared line instead of resolving it afterward.
Binding each decision to the engine identity too lost because an engine change leaves a skill's judgment unchanged.
Copying the previous reason into the draft lost because the check would then accept an unchanged judgment for changed content.
Letting the catalog decision settle the size finding lost because an oversize entrypoint would then need no judgment on its size.

## Consequences

A PR that changes only engine code needs no decision change.
Removing a skill means removing its decision file.
A locally edited skill no longer matches its reviewed revision and returns to local triage.
