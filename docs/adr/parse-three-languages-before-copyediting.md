---
status: proposed
date: 2026-10-23
---
# Parse three languages before copyediting

## Context

The persisted English policy checks language, sentence layout, and style.
Publication also needs repeatable English, Japanese, and simplified Chinese checks that can inspect grammatical relationships.
An accepted language or a successful parser doesn't establish grammatical correctness or factual truth.

## Decision

Add an explicit `prose copyedit` command alongside the existing English gates.
Read Markdown and human-readable YAML fields through their parsers, preserving visible text, captions, titles, and source locations while protecting code, destinations, and machine metadata.
Run the pinned CPU spaCy pipelines for all three languages through a small typed library adapter.
Keep editorial decisions and process orchestration in Rust.
The adapter only validates requests and projects spaCy annotations into a versioned interchange format.

Lock the Python runtime, packages, model archives, and tokenizer dictionaries.
Validate returned token, sentence, entity, and dependency spans before using them.
Refuse missing models, empty applicable coverage, invalid output, and mismatched engine identities.
Keep source and configuration hashes with diagnostics and annotation inventories.
Run the existing English Vale rules or Japanese textlint rules alongside the common checks.
Add pinned LanguageTool grammar and spelling checks for English and Chinese.
Its Japanese rules incorrectly removed the valid plural suffix in the accepted regression control, so Japanese uses the native rules and common parser.
The [dictionary](https://www.kanjipedia.jp/kotoba/0004557800) and [textbook reference](https://www.kyoiku-shuppan.co.jp/textbook/shou/kokugo/guidanceq002-00.html) support preserving that suffix.

The observable contract covers visible Markdown prose in explicitly selected languages.
Malformed input or an unavailable engine fails the run rather than producing a passing report.
The command writes no article changes.
Its deterministic rules identify specific defects.
Model-dependent suggestions remain identifiable as suggestions.
Entity and numeric inventories support a separate evidence check and never count as verified facts.

## Alternatives

Regular expressions alone can't establish token boundaries or dependency roles.
A generative proofreader would make repeated results depend on a remote model and its changing behavior.
Transformer pipelines would add memory and runtime costs to an on-demand local check without establishing correctness.
The small CPU pipelines provide inspectable annotations with bounded inputs.

## Consequences

The English fast gate keeps its current behavior and installation cost.
On-demand linguistic checks require a separate locked runtime and model installation.
Store their tool paths in separate runtime metadata to preserve compatibility with older commands.
An installation regression reads the produced legacy metadata through that older schema.
The Japanese model uses the `CC-BY-SA-4.0` license.
The English and Chinese models use the `MIT` license.
Model archives stay dependencies rather than copies of repository source.
Their upstream license and attribution metadata remain installed with them.

Pure rule decisions receive Kani obligations and rejecting probes.
Known-answer multilingual fixtures and accepted controls measure the external annotation boundary.
Those measurements don't prove grammatical completeness, naturalness, or historical accuracy.
Reconsider the model choice when a pinned release can't parse a supported language or its measured controls regress.
