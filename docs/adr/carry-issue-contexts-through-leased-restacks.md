---
status: accepted
date: 2026-11-03
---
# Carry issue contexts through leased restacks

## Context

The [issue-context decision](authenticate-issue-context-at-edit-and-integration-boundaries.md) records the starting commit of a branch when `pr-workflow start` succeeds, and every later edit, commit, and change check requires that commit to stay an ancestor of the branch head.
The pull-request skill forbids a manual rebase and names `pr-workflow stack sync` and `stack rebase` as the only way to restack.
Both rewrite each stack branch onto its updated parent, so the recorded commit leaves the branch history, as [issue 551](https://github.com/P4suta/dotfiles/issues/551) records.
The only recovery was a new branch for the same issue.

## Decision

`stack sync` and `stack rebase` record each local stack branch's issue context, head, and own change before they restack under the stack lease.
The own change uses the digest that carries a signed assessment through a base update, over the branch's diff from its merge base with its stack parent.
After the restack, a branch whose head no longer contains its recorded start commit carries its context to the new head only when the context held before the restack and the restack kept the branch's own change.
The carry authenticates the receipt it replaces, signs a new receipt for the new head with the same destination, issue, scopes, and initial snapshot, and persists it as a refresh of the same branch and issue.
Any other rewritten branch keeps its old context, and the command names it as needing a new start.
An adopted baseline needs a new start after a restack, because its one-time approval is already consumed.
A manual rebase or amend still leaves the recorded commit behind, so the edit guard and change check still refuse it.
The [restack rules](../../crates/rules/src/restack_rules.rs) decide the carry, and Kani harnesses with rejecting probes check them.

## Alternatives

A lineage record that the context check follows from the old start commit to the rewritten head would change the context check, the local state schema, and the edit guard for the same outcome.
Carrying every context of a rewritten branch without comparing the change would let a restack that resolved conflicts pass as the same work.

## Consequences

A restack keeps the issue context of each member whose change survived, so the work continues on the same branch and issue.
A restack that changed a member's own diff, such as one that resolved a conflict, still needs a new start for that member.
