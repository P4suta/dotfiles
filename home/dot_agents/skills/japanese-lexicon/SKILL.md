---
name: japanese-lexicon
description: >-
  Verify Japanese readings, romanizations, and Japanese-derived technical names with identified lexical records and a named spelling standard.
  Use when introducing, checking, or correcting a reading or romanized identifier, not for unrelated translation.
---

# Japanese lexical evidence

Check the original expression, its sense and context, its kana reading, and its spelling convention as distinct inputs.
Preserve established project identifiers until their public contract and governing terminology justify a change.
Keep dictionary readings distinct from pronunciation forms, keyboard input sequences, and conventional personal or place names.

Use an applicable standard or its issuing body's terminology for technical vocabulary.
For ordinary words, prefer National Institute for Japanese Language and Linguistics lexical records and other public language resources.
Major dictionaries from established publishers also qualify.
A licensed dictionary entry must identify the dictionary, publisher, and relevant headword.
Search snippets, unsourced aggregators, model confidence, and another model's agreement don't establish a reading.

Open the actual entry and keep its URL, title, publisher, headword or record locator, relevant excerpt, and context.
Confirm that the entry supports this sense and this reading together.
A page that mentions both strings somewhere doesn't by itself establish that relationship.
Check alternative readings and proper-name conventions before selecting one.
If the evidence remains ambiguous or unavailable, keep the Japanese expression and state the unresolved point.
Keep a reading unconfirmed until its entry supports it.
Disclose any substitution of the original expression.

Choose the spelling standard before converting kana.
The [Cabinet notice](https://www.bunka.go.jp/kokugo_nihongo/sisaku/joho/joho/kijun/naikaku/roma/index2.html) governs its stated scope.
The [National Institute's conversion notes](https://chamame.ninjal.ac.jp/about_romanise.html) distinguish lexical analysis from spelling and describe ambiguity in automated readings.
Inspect automated morphological analysis in the context of the expression.
Its selected reading can conflict with the intended sense.

Use `dotfiles answer check` with the request's exact draft and lexical evidence before answering.
Read [the evidence format](../checked-arguments/references/evidence-format.md) for the command and supported spelling conventions.
The checker compares the declared kana with the retrieved record and checks the supported conversion or an attested spelling.
It doesn't infer an undeclared Japanese origin from arbitrary Latin text or prove the semantic interpretation of a dictionary entry.
For an unsupported convention, keep the result unresolved until an appropriate checker or an attested source covers it.

Report the relevant source and spelling convention with a correction.
Never report a missing source, an unsupported conversion, or an incomplete check as successful verification.
