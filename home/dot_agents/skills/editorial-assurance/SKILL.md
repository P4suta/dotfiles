---
name: editorial-assurance
description: >-
  Write, research, revise, copyedit, and proofread prose to meet its audience, evidence, and publication requirements.
  Use for articles, explanations, reports, translations, and requested editorial reviews, including correcting empty explanations or unsupported claims.
---

# Editorial assurance

Deliver the requested text with its research and explanations completed.
Preserve the writer's intended meaning, genre, audience, language, and house style.
Apply the owner's shared [Japanese and technical terminology policy](references/japanese-style.md) to replies, documents, comments, and authored artifacts.
For policy changes, use ideal-specification to derive requirements from governing standards and intended outcomes.
Keep enforceable requirements separate from examples and regression fixtures.
Apply the relevant criteria in [editorial review](references/editorial-review.md), including argument structure, participant identity, evidence, explanation, language, translation, and the rendered proof.
Choose the representation from the reader's task and the information's relationships using [representation selection](references/representation.md).
Choose and verify stable publication identifiers using [public article URLs](references/public-urls.md) when authoring or changing a public route.
Use connected paragraphs for authored prose and replies, with diagrams, illustrations, or interaction where the task benefits.
The shared house style prohibits bulleted lists, numbered lists, and tables.
Don't use generative image models to create or edit images.
For technical or release writing, also read [technical explanations](references/technical-explanations.md).
Detailed technical reports must pass the `detailed-report` editorial profile and its required explanation length.
Original visual assets use the GitHub Light theme described in the representation reference.

For factual exposition, remove unsolicited personal reactions, routine research declarations, empty claims of usefulness, common-sense reminders, and defensive commentary.
Supply the actual dates, behavior, code, mechanism, alternatives, and consequences required by the text.
A link supports an explanation already present in the article.
Never use it to transfer promised research to the reader.
Investigate a material gap, make the claim narrower, or remove the unsupported claim without disguising an incomplete requested deliverable as complete.
Keep necessary technical conditions and evidence-backed limitations.
Preserve intentional voice in opinion, testimony, narrative, and creative work.

Verify consequential claims using the source passage, its context, date, and applicable version.
Check consequential claims with their sources without requiring a separate claim ledger.
Put citations and necessary explanatory content beside the supported claims in the deliverable.
Distinguish literal quotations, translations, adapted examples, and original observations.
Use [checked-arguments](../checked-arguments/SKILL.md) for inference and [japanese-lexicon](../japanese-lexicon/SKILL.md) when introducing a reading or romanization.

Use the installed `prose` checks as described in [mechanical checks](references/mechanical-checks.md).
Reuse the multilingual copyediting engines instead of inventing another grammar checker.
Review diagnostics in context, repair real defects, and repeat the checks on the changed source.
Review the finished text for source meaning, attribution, technical concepts, and natural expression even when the mechanical reports contain no findings.
Inspect the intended rendered output and recheck changes made during proofreading.
Supply the requested content and resolve material findings before concluding.
Resolve a required unavailable check or ask the user for the decision needed before claiming completion.
Report editorial completion only after applying the relevant editorial criteria to the finished text.
Test counts and clean mechanical reports don't supply an unperformed editorial review.
