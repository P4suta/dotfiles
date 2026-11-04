---
status: accepted
date: 2026-10-24
---

# Enforce shared house style before answer evidence

## Context

The editor and copyeditor could accept Japanese demonstratives and inconsistent technical terminology because no shared executable policy covered those requirements.
A skill's broad reference to a style guide didn't select governing conventions or enforce the owner's narrower choices.
Answer evidence could record successful provenance checks for a draft whose writing violated the owner's house rules.

## Decision

Define the preferred technical terms and Japanese demonstrative forms in one source-controlled policy.
Compile that policy into the shared checker and include its source in the installed tools' build identity.
Require matching installed configuration before native checks use the policy.

Preserve original English software engineering terminology in Japanese prose and keep literal identifier spelling and source quotations.
Use the selected guide from the Japan Translation Federation for Japanese orthography with explicit house choices.
The owner-specific terminology and demonstrative restrictions aren't universal rules from that guide.

Use existing Markdown projection and native Japanese annotations to check word boundaries, grammatical roles, and source locations.
Check the configured Japanese orthography rules for authored Japanese content.
Preserve marked literal material and keep the relevant source and sense checks in editorial review.
Don't use substring replacement to turn ordinary Japanese words into English.

Invoke the same implementation from prose, editorial, copyediting, and answer-evidence checks.
Refuse an answer receipt when the shared policy reports findings or can't complete its applicable checks.
Existing client Stop adapters bind completion to the exact draft and producer receipt.
The adapters operate after display, so they don't establish a barrier before the first display.

## Validation

Native controls cover authored demonstratives, metadata, headings, compound terminology, quoted source, code, ordinary words, and auxiliary expressions.
Answer checks exercise refusal before receipt creation and acceptance of a corrected draft through the same policy boundary.
Kani covers the deterministic language, origin, and match decision and rejecting probes.
The owned fuzz target exercises the policy decoder and source-span production with validated annotations.
The language model and source interpretation remain external trust assumptions.
