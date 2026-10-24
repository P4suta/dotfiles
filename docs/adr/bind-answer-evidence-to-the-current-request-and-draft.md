---
status: proposed
date: 2026-10-23
---
# Bind answer evidence to the current request and draft

## Context

Instructions alone don't check Japanese readings, argument dependencies, or evidence freshness.
Formal proof checks a formal statement under declared assumptions.
Source retrieval and proof don't establish the correspondence between prose, a lexical record, and the intended model.
Stop hooks run after a client displays an answer, as [the persisted prose decision](persisted-prose-gates.md) records.

## Decision

Use separate shared skills for lexical evidence and checked arguments.
Keep their semantic review requirements and explicit trust boundaries in the skills.
Put deterministic checks in the maintained `dotfiles answer` command and its pure decision core.
Keep the command independent of experimental skill maintenance.

A UserPromptSubmit hook creates a new request generation.
The checker covers each nonempty draft line with a declared claim and binds completed evidence to that generation, prompt, draft, and producer.
Source checks retrieve identified textual records from registered authority classes over HTTPS.
The evidence records hashes of the retrieved contents.
Lexical checks distinguish reading evidence from the spelling standard and refuse unsupported conversion.
Dictionary records from established publishers qualify alongside public language resources.

Supported formal arguments use a bounded propositional syntax.
A complete evaluator searches its declared Boolean domain for countermodels and inconsistent premises.
Generated proof terms receive a proof-assistant check, an axiom report, and independent kernel replay in the pinned bounded image.
The generator accepts no arbitrary proof program from the evidence document.
Source, testimony, and observation claims remain externally justified premises.
Hypothetical premises keep their conditional status in derived conclusions.

Comparison checks require matched declared axes and a correct ranking for a stated numeric criterion.
Parallel checks require matched declared semantic roles.
Their classification and the prose-to-formula interpretation remain semantic review obligations.
The command describes those obligations as semantic review requirements.

The final Stop hook checks the exact reply rather than accepting a success from another draft or request.
The gate refuses missing, malformed, stale, unsupported, or incomplete evidence.
A bounded correction ends with an explicit incomplete result if the evidence still fails.
Client settings updates preserve unrelated handlers and preferences.

## Alternatives

A skill alone leaves deterministic requirements unenforced.
A model-generated approval or a citation count establishes neither entailment nor premise truth.
One bare solver status leaves proof artifacts and assumptions unchecked.
Tools that share a mistaken model leave its semantic mistake intact.
A post-display hook checks text that the client has shown.

## Consequences

The gate checks explicit provenance, finite logic, structured comparisons, and evidence freshness.
It doesn't provide universal natural-language validation or isolation from an agent with the owner's filesystem privileges.
Other proof domains require suitable verification before their conclusions can claim formal assurance.
Client trust and reload remain part of deployment verification.
Bounded runs preserve the owner's resource policy, and hours-long campaigns still require a resource window.
