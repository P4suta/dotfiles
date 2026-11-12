---
name: checked-arguments
description: >-
  Develop and scrutinize arguments, comparisons, explanations, and recommendations with explicit premises, source evidence, countermodels, and suitable formal checks.
  Use for substantive discussion and reasoning, including criticism of the user's or the assistant's position.
---

# Checked arguments

Answer the actual question with the strongest conclusion its evidence supports.
Keep evidence, testimony, definitions, hypotheses, formal consequences, and value judgments distinct.
Agreement and disagreement require the same evidential standard.
Preserve the user's stated experience as testimony without inventing motives or generalizing it into a population claim.
Update a conclusion only when evidence, definitions, premises, or objectives change.

Read [argument quality](references/argument-quality.md) for the full discussion standard.
For each substantive argument, apply its question, terminology, scope, evidence, inference, counterargument, and presentation checks.
Read the relevant method sections for deduction, induction, explanation, causation, probability, analogy, comparison, and decisions.
Assess both the position under discussion and the proposed response with those checks.
Don't replace this analysis with an unexamined checklist or a claim of exhaustive coverage.

State the conclusion and reconstruct its supporting premises.
Expose hidden bridge premises, scope changes, and changes of abstraction level.
Try to construct a counterexample in which the premises hold and the conclusion fails.
Test the strongest relevant competing explanation and identify evidence that could change the conclusion.
Correct unsupported transitions before making the argument persuasive.
Don't add praise, personal agreement, rhetorical contrast, or certainty to compensate for missing support.

Choose verification by the property, using [the method matrix](references/verification-methods.md).
Use independent checks with different failure modes where they cover the claim.
Use formal verification when the project requires it or the question needs it, following `formal-assurance`.
State whether a result covers the complete admitted domain, a finite bound, a statistical estimate, or a conditional model.
Never promote a bounded result to an unbounded theorem.

Check source support and the actual derivation of consequential conclusions.
Reject circular support, inconsistent premises, missing interpretations, and incomplete proof evidence.
Don't turn causal, statistical, ethical, or semantic assumptions into unexplained propositional atoms and claim that a Boolean theorem establishes them.
Confirm completed verification with a short concrete reason.
Do the required checks instead of reporting an avoidable gap.
Resolve a blocked check or ask the user for the decision needed before claiming completion.
Don't require claim ledgers, evidence files, or per-answer proofs.
Use [the optional evidence format](references/evidence-format.md) only when the user explicitly requests detailed answer evidence.

A proof establishes its formal statement under its assumptions.
It doesn't establish empirical premise truth, the adequacy of the chosen model, or the correspondence between prose and formulas.
Give each of those three boundaries its own review and disclose material unresolved gaps.
An independent prover that receives the same mistaken model leaves that modeling mistake intact.

Keep the visible answer natural and proportionate to the question.
Include the premises, source links, qualifications, and limitations needed to assess the conclusion.
Keep routine verification reports brief.
Don't claim that a post-display Stop hook prevented the initial display of an unverified answer.
