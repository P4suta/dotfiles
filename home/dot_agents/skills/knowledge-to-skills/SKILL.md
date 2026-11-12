---
name: knowledge-to-skills
description: >-
  Proactively turn reusable knowledge into shared agent skills and focused references.
  Use when research, implementation, debugging, review, failures, or user feedback reveals methods, decision criteria, constraints, or pitfalls that can improve future work.
---

# Turn knowledge into skills

Whenever work reveals reusable knowledge, default to saving it in the shared skill system during the authorized task without waiting for a separate request.
Capture lessons from implementation, debugging, design choices, reviews, failures, user corrections, and research.
Use the available skill authoring guidance and [portable-skills](../portable-skills/SKILL.md) for the canonical source and installation.
This authoring workflow doesn't require experimental skill telemetry, maintenance, or background jobs.

Find an existing skill with the same decision or operation before creating another.
Update its relevant instructions or focused reference instead of creating overlapping skills for individual queries, websites, or products.
Create a narrowly named skill when the workflow has no suitable home.
When renaming or consolidating skills, update references and client discovery and remove empty former directories before checking the canonical catalog.
The catalog expects a `SKILL.md` in each skill directory.
Preserve the user's language, product, scope, and authorization boundaries.

Save the method, applicability conditions, decision criteria, failure cases, and verification that future work needs.
Keep the supporting primary sources, reproducible observations, or explicit user preference that justify the lesson.
Link external sources by exact URL and identify the supporting section.
Distinguish source requirements, recommendations adopted as owner policy, project conventions, and unresolved interpretations.
Keep task-specific paths, credentials, personal data, and operational state out of shared instructions.
Generalize the reusable method without copying confidential source or customer context.
Keep an observed fix, a causal hypothesis, and a general rule distinct, and state the conditions that support reuse.
Don't turn an unverified search snippet or a failed experiment into a rule.

Keep current prices, versions, availability, and other changing facts separate from durable instructions.
Record the source and relevant observation date when a reference needs historical context, and require a fresh check before a future decision depends on that fact.
Saving a source doesn't establish that its contents remain current.
Keep a concise rationale when a finding adds no reusable guidance, rather than manufacturing a new skill from transient data.

Use existing frontmatter, prose, link, catalog, and installation checks for changed skills.
Try a realistic task when the new guidance changes substantive decisions.
Verify that the guidance leads to the intended outcome, rather than testing whether its wording contains a list of preferred phrases.
Install authorized shared changes through each affected machine's native dotfiles profile and verify discovery.
