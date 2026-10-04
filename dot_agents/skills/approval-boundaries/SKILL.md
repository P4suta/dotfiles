---
name: approval-boundaries
description: >-
  Assess or record an AI-assisted PR or workflow approval with exact scope, evidence, and user authorization.
  Use for approval actions and protected release gates, not ordinary read-only code review.
---

# Explicit Approval Boundaries

A review recommendation and a submitted approval are separate actions.
Without strong explicit user permission for the relevant scope, prepare the concrete review and leave the actual approval to the user.
Requests to review, investigate, configure CI, or make a workflow pass do not by themselves authorize a production approval.
Do not derive signing, registry publication, or Release publication permission from commit, push, PR, or merge permission.
An explicit prohibition always limits a broader delegation.
Keep existing authorization throughout the session and do not ask again for an action that is already clearly authorized.

Before a submitted approval, bind the decision to the exact repository, PR head SHA or workflow run and attempt, environment, and operation.
For signing or publication, also bind it to the verified candidate identity, version, source SHA, and artifact hashes.
Check the current target immediately before approving; a changed head, new attempt, or different candidate needs a fresh assessment within the actual authorized scope.
Read the job that uses the environment, including called workflows, instead of inferring its effects from the environment name.
A credential-verification job and a production release job can share an environment while authorizing different effects.

Use a short English approval record with these fields:

```text
Decision: APPROVE | REQUEST_CHANGES | COMMENT | HOLD
Scope: code review | credential verification | signing | registry publication | Release publication
Target: repository; PR and head SHA, or workflow/run/attempt/environment; candidate identity when applicable
Evidence: the relevant successful checks and any unresolved finding
Authorization: the user's explicit permitted operation and applicable limits
```

Replace the alternatives with the actual values; do not post an incomplete template.
State that the assessment is AI-assisted when submitting it through the owner's authenticated account.
Do not claim a human independently reviewed the change or use another identity to approve the owner's own PR.
For a GitHub deployment approval, use its comment to record the same concise scope and evidence.
Write no credentials or private payloads in an approval record.

Do not approve while relevant checks fail, blocking findings remain, required evidence is missing, or the operation exceeds the permission.
Do not use administrator bypass, change reviewers, weaken rulesets, or disable immutability to make an approval possible.
Complete all permitted preparation and verification before requesting any genuinely missing final authorization.
An approval does not authorize publishing through another route, removing a protected gate, or reusing a different candidate.
