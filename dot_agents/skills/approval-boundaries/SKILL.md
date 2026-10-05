---
name: approval-boundaries
description: >-
  Assess or record an AI-assisted PR or workflow approval with exact scope, evidence, and user authorization.
  Use for approval actions and protected release gates, not ordinary read-only code review.
---

# Explicit approval boundaries

A review recommendation and a submitted approval differ.
Without explicit user permission for the scope, prepare the review and leave the approval to the user.
A request to review, investigate, configure continuous integration, or make a workflow pass grants no production approval.
Commit, push, pull request, or merge permission grants no signing, registry publication, or release publication permission.
An explicit prohibition limits any broader delegation.
Keep existing authorization for the session, and never ask again for an authorized action.

Bind an approval to the exact repository, pull request head commit or workflow run and attempt, environment, and operation.
For signing or publication, also bind it to the verified candidate identity, version, source commit, and artifact hashes.
Check the target immediately before approving, and reassess after a changed head, a new attempt, or a different candidate.
Read the job that uses the environment, including called workflows, instead of inferring its effects from the environment name.
A credential-verification job and a production release job can share an environment and still grant different effects.

Write a short English approval record with these fields:

```text
Decision: APPROVE | REQUEST_CHANGES | COMMENT | HOLD
Scope: code review | credential verification | signing | registry publication | Release publication
Target: repository; PR and head SHA, or workflow/run/attempt/environment; candidate identity when applicable
Evidence: the relevant successful checks and any unresolved finding
Authorization: the user's explicit permitted operation and applicable limits
```

Replace each set of alternatives with the actual value, and never post an incomplete template.
Mark the assessment as AI-assisted when submitting it through the owner's account.
Never claim an independent human review, and never use another identity to approve the owner's own pull request.
Record the same scope and evidence in the comment of a GitHub deployment approval.
Keep credentials and private payloads out of an approval record.

Never approve while relevant checks fail, blocking findings remain, required evidence goes missing, or the operation exceeds the permission.
Never use administrator bypass, change reviewers, weaken rulesets, or turn off immutability to make an approval possible.
Finish all permitted preparation and verification before requesting a missing final authorization.
An approval grants no other publication route, no removal of a protected gate, and no reuse for a different candidate.
