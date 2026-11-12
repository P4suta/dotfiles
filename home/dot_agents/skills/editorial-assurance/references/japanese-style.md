# Shared Japanese and technical terminology policy

Apply the policy to replies, documentation, comments, and authored artifacts across projects.
Keep ordinary prose in the required language.
Use connected paragraphs for authored prose and replies in every language.
The house style prohibits bulleted lists, numbered lists, and tables, including their HTML forms.
Preserve required structure in code, machine-readable data, and literal attributed quotations.
Choose a different representation for its explanatory function, rather than turning prohibited structures into images or disconnected cards.
Don't use generative image models to create or edit images, or include model-generated imagery in authored deliverables.
Use inspectable source code or data with deterministic renderers for diagrams, charts, and illustrations.
Use canonical English terminology from the applicable technical specification or project's official documentation.
Keep an English phrase's internal spaces and preserve identifier spelling and case.
Use the English forms consistently rather than alternating Japanese translations, transliterations, capitalization, and abbreviations.

## Authority and concrete choices

Current user and project instructions govern their scope.
Preserve literal identifiers, commands, UI labels, product names, and source quotations.
The shared terminology dictionary defines preferred English forms and their primary terminology records.
The selected edition of the [Japan Translation Federation Japanese style guide](https://www.jtf.jp/jp/style_guide/pdf/jtf_style_guide.pdf) governs Japanese orthography where the house policy makes no narrower choice.
The shared policy selects version 4.0, published on July 25, 2026.
The guide's introduction permits coherent adjustments for an organization's rules and identifies technical localization as its foundation.
Its scope covers orthography rather than translation technique or explanation quality.

The user's English terminology preference and prohibition of Japanese demonstratives are explicit house rules.
They aren't claims that the guide or every engineering organization requires the same vocabulary.
For comparison, the [Japanese localization guide](https://contribute.cncf.io/techdocs/localization/ja/) preserves project-specific English terminology and supplies a controlled term table.

Use full-width Japanese punctuation and half-width Latin letters and digits.
Apply the guide's rules for punctuation usage and character width.
Ordinary Japanese prose normally avoids colons and semicolons, with the guide's stated exceptions for labels and required literal syntax.
Don't transfer source-language separators into Japanese sentences without checking their grammatical function.
Keep one Japanese register within the relevant prose unit.
Keep semantic spaces within an English phrase or a numeric quantity and its unit, and add no visual spacing between Japanese and Latin text.
Keep the full vowel mark in Japanese loanwords that remain Japanese.
Use no separator within an established Japanese compound loanword unless clarity requires one.
The guide permits a consistent choice among compound separators, so the older checker's mandatory separator doesn't govern that choice.

## Terminology and meaning

Identify the concept, domain, technical role, and applicable source version before selecting a spelling.
Check specialized meanings through the named objects, operations, and constraints in the primary record.
Explain a concept's relevant properties when the expected reader can't recover them from its name.
Prefer the terminology of the governing specification, published API, or project's official documentation over a secondary translation.
Keep distinct concepts and states distinct even when ordinary language uses one word for them.
Name the operation and condition that determine behavior when an abstract state label leaves them ambiguous.
Use ordinary grammatical capitalization for prose and the exact published case for literal names.
Preserve status qualifiers that change a claim's meaning.
Don't create compressed labels that obscure those distinctions.

Extend the canonical dictionary with the concept, preferred form, rejected variants, and primary record when new terminology needs a durable decision.
Keep the source and sense specific to the project.
Don't replace ordinary Japanese vocabulary with English merely because the vocabulary also occurs in technical prose.

Use ideal-specification when establishing or revising a durable rule.
Derive the rule from the intended outcome and governing sources before selecting representative examples and regression cases.
The dictionary records independently supported terminology decisions rather than defining the general policy by enumeration.

## Enforcement

The canonical `house.toml` compiles into the shared checker and binds answer evidence to its producer.
Install matching configuration and tools after changing the dictionary.
The prose, editorial, copyediting, and answer-evidence entry points call the same policy.
The native Japanese annotations establish word boundaries and grammatical roles before diagnostics identify forbidden demonstratives or term variants.
Literal code and marked source quotations keep their original wording.
Use explicit literal markup when discussing a forbidden spelling as data.
Keep the source comparison and editorial review for quotation fidelity, term sense, and explanations that a token check can't establish.

The pinned style-guide preset implements an older edition and supplies compatible mechanical rules.
The selected current edition and the documented house choices govern differences.
Neither a preset's name nor an empty report establishes full compliance with every editorial rule.
Keep unresolved source, meaning, attribution, and natural-language review findings open after a mechanical pass.
