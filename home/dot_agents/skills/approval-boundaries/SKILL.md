---
name: approval-boundaries
description: >-
  Assess and submit a PR or workflow approval with exact scope, evidence, and user authorization.
  Use for approval actions and protected release gates, not ordinary read-only code review.
---

# Explicit approval boundaries

Without explicit user permission for the scope, prepare the review and leave the approval to the user.
A request to review, investigate, configure CI, or make a workflow pass grants no production approval.
Commit, push, PR, or merge permission grants no signing, registry publication, or release publication permission.
An explicit prohibition limits any broader delegation.
Never ask again for an action the user already authorized.

Bind an approval to the exact repository, PR head SHA or workflow run and attempt, environment, and operation.
For signing or publication, also bind it to the verified candidate identity, version, source SHA, and artifact hashes.
Check the target immediately before approving, and reassess after a changed head, a new attempt, or a different candidate.
Read the job that uses the environment, including called workflows, instead of inferring its effects from the environment name.

Write the approval or review comment in the owner's voice, as `pull-request` describes for published text.
State what the review checked and anything still open in one or two plain sentences.
Leave out field labels, authorization notes, and any description of who or what wrote it.
A deployment approval comment names the same scope and evidence in the same voice.
Never approve the owner's own PR from another identity.
Keep credentials and private payloads out of every comment.

Never approve while relevant checks fail, blocking findings remain, required evidence goes missing, or the operation exceeds the permission.
Never use administrator bypass, change reviewers, weaken rulesets, or turn off immutability to make an approval possible.
Finish all permitted preparation before requesting a missing final authorization.
An approval covers no other publication route, no removal of a protected gate, and no other candidate.
