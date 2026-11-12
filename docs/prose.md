# Multilingual copyediting

The shared house policy covers replies, persisted prose, human-readable metadata, and editorial checks.
Personal replies and project notes use direct address and omit ownership labels for the user.
The shared address check preserves qualified technical ownership terms, literal identifiers, and source quotations.
Its canonical dictionary lives in `home/dot_config/prose/house.toml` and compiles into the checker.
Japanese technical prose uses the preferred English terms and preserves literal identifiers and quoted source material.
Japanese demonstratives in authored explanations produce error-severity findings.
Native tokens and grammatical roles preserve ordinary words and non-demonstrative auxiliary forms.
The pinned Japanese style checks use compatible rules from the Japan Translation Federation and the documented choices for the selected edition.
An answer can't receive an evidence receipt until the same policy passes.
The existing client Stop adapters then require that exact receipt for the displayed draft.
The Stop boundary follows display and doesn't prevent the initial display of an unchecked reply.

Use the shared [editorial-assurance skill](../home/dot_agents/skills/editorial-assurance/SKILL.md) for source review, explanatory completeness, revision, and the rendered proof.
The local `prose editorial` command identifies structural gaps and recognized editorial shortcuts before the linguistic pass.
Its [checker contract](../home/dot_agents/skills/editorial-assurance/references/mechanical-checks.md) defines genre profiles, source hashes, exclusions, and exit statuses.

`prose check` keeps the persisted English policy.
`prose copyedit` adds explicit local checks for English, Japanese, and simplified Chinese.
The checks parse visible Markdown prose, headings, captions, table cells, and human-readable YAML fields.
Those fields include titles, descriptions, excerpts, and feature image captions and alternative text.
Code, link destinations, raw markup, and machine metadata stay outside that text projection.
The command reads articles without rewriting them.

Detailed technical reports use `prose editorial --profile detailed-report` as their delivery gate.
The profile includes expository checks and rejects short authored explanations with `editorial.report-too-short`.
Its canonical threshold lives in `rules::editorial_rules::MIN_DETAILED_REPORT_CHARACTERS`.
The report exposes `authored_body_characters` for each source.
The metric counts non-whitespace Unicode characters in unquoted body paragraphs and excludes code, headings, metadata, captions, quotations, and link destinations.
Source coverage and explanation quality remain separate editorial obligations.

```sh
prose copyedit --language en --report english-pass-one.json article.md
prose copyedit --language ja --report japanese-pass-one.json article.md
prose copyedit --language zh --report chinese-pass-one.json article.md
prose copyedit-verify --report chinese-pass-one.json
```

Use `--source /path/to/dotfiles` before the subcommand to select a checkout's configuration.
Use `--quiet` to print a collection's summary.
The JSON report includes every finding and annotation.
A new pass needs a new report path.
Run `just copyedit-check` to typecheck the adapter and exercise the multilingual regression corpus.
Exit status zero means no applicable diagnostics, one means unresolved diagnostics, and two means the run couldn't establish complete coverage.

## Shared contract

The pinned spaCy pipelines supply tokens, part-of-speech tags, dependency relationships, sentence boundaries, and named entities.
The checker validates source identity, complete token coverage, character offsets, sentence membership, and dependency roots before applying rules.
YAML parsing preserves decoded titles and captions, including quoted escapes and folded text, with enclosing source spans.
Ambiguous, aliased, or non-string human fields fail the run.
LanguageTool adds its English and Chinese grammar, spelling, punctuation, and usage rules.
English also runs the existing Vale policy.
Japanese also runs the pinned technical writing preset through textlint.
OpenCC supplies a dictionary-based simplified Chinese comparison.

The Japanese LanguageTool rules failed an accepted control by marking a valid plural suffix as an error.
Japanese uses its native textlint rules and the common linguistic checks.
The [dictionary entry](https://www.kanjipedia.jp/kotoba/0004557800) and [textbook publisher's explanation](https://www.kyoiku-shuppan.co.jp/textbook/shou/kokugo/guidanceq002-00.html) support that control.
The regression corpus preserves the example, and adopting another Japanese grammar engine requires it to pass.

Model and dictionary versions, installed package versions, active grammar rules, source hashes, and the checker's source build hash stay in the report.
`copyedit-verify` refuses stale reports and unresolved findings, then repeats the applicable engines over the unchanged source files.
An unavailable tool, invalid response, empty source set, or partial annotation fails the run.
The checker never treats a failed engine as a clean result.

## Candidate review

Fix actual defects and run a new pass before reviewing remaining candidates.
Create a pending decision template when a reproducible candidate is a false positive.

```sh
prose copyedit-review --report chinese-pass-one.json --output chinese-decisions.json
prose copyedit-verify --report chinese-pass-one.json --decisions chinese-decisions.json
```

For each diagnostic identity, replace its pending resolution with a retention containing a specific reason and a literal evidence quote that covers the diagnostic in its projected source text.

```json
{
  "status": "retain",
  "reason": "民\u95f4 modifies \u4f20\u8bf4 as a compound; \u95f4 is not a numeral classifier here.",
  "evidence": "河童属于日本的民\u95f4\u4f20\u8bf4，不同地方的故事与造型各有\u53d8化。"
}
```

The decision file binds the complete report, including source bytes, model identities, configuration, rules, annotations, and diagnostic identities.
Every candidate needs one decision.
Missing, duplicate, unknown, pending, blank, or unevidenced decisions fail verification.
Review decisions never waive error-severity diagnostics.
Changing the report, source, checker build, or configuration requires a new pass and new decisions.
Verification repeats the engines and requires the same coverage, diagnostics, model metadata, and linguistic annotations.
The grammar report records execution dates because calendar-sensitive rules can run on another date.
Changed findings still fail verification.
Diagnostics remain visible in the original report, and successful reviewed verification reports their retained count.
A recorded reason remains an editorial judgment, not machine proof of its linguistic or factual truth.
The [decision boundary](adr/bind-copyedit-decisions-to-exact-reports.md) defines this contract.

## Language standards

English keeps its existing contractions, terminology, sentence layout, attribution, and style rules.
The linguistic pass also checks grammatical roles, including present-tense subject agreement, singular determiners with plural nouns, and repeated function words.
Preserve semantic spaces between English words and between a numeric value and its unit symbol.

Japanese uses natural subject-predicate relationships and consistent register.
Check particles, auxiliary verbs, redundant constructions, repeated words, punctuation, and sentence layout with the native rules.
Use paragraph structure for indentation and alignment.
Preserve verified names, readings, quotations, and required Latin word boundaries.
Verify a Japanese reading or romanization through the shared `japanese-lexicon` workflow before adding it to prose or an identifier.

Simplified Chinese uses consistent character forms outside verified names and literal quotations.
Check function words, modifier roles, punctuation, and the distinction between attributive and adverbial constructions.
Keep names and book titles consistent throughout a collection.
Use the dictionary comparison as a diagnostic rather than an automatic transliteration or factual correction.
Chinese and Japanese prose mustn't use character spacing, full-width indentation, repeated spaces, or invisible separators for visual alignment.
The shared checks also identify unusual Unicode spacing and bidirectional formatting controls.
Nonbreaking numeric unit separators remain valid controls.

The paragraph check compares normalized text and content-word lemma trigrams within each document.
Near-duplicate candidates require at least 90% overlap and matching entity, number, negation, auxiliary, and tense annotations.
Different factual anchors and different source documents don't count as near duplicates.
Long dependency chains produce separate review candidates only when word count, clause count, and dependency distance all meet their language profile's thresholds.

## Evidence and limits

A diagnostic identifies a reproducible candidate rather than proving that the sentence is wrong.
A clean result doesn't prove naturalness, completeness, or historical accuracy.
The small statistical models can assign an incorrect grammatical role.
The regression corpus includes accepted controls and rejected examples.
The corpus includes names, mixed scripts, code, links, ordinary unit spacing, compound subjects, and modal verbs.
The paragraph controls include different names, numbers, negation, and modal statements.

Separate factual claims from language decisions.
Use primary sources for names, dates, geography, attribution, operational details, and numeric claims.
Distinguish a recorded visit from current guidance, and state the date when a claim can change.
Keep a source's statement distinct from an inference or a recommendation.
Don't turn a belief, legend, marketing claim, or personal impression into a universal fact.
Entity and numeric annotations identify material to verify.
Record source evidence for each factual claim.

Complete a source pass and a language pass, then check the corrected draft again.
Keep short articles focused and add useful context only when the sources support it.
Remove repeated conclusions, unfinished notes, stale invitations, and unrelated filler.
Preserve recorded experiences without inventing visits or observations.

## Runtime and assurance

The optional linguistic runtime installs on its first explicit copyediting run.
Its tool paths use separate runtime metadata so older commands can still read the existing two-tool schema.
The locked Python graph includes the three CPU models, their tokenizer dictionaries, and OpenCC.
LanguageTool comes from its pinned upstream archive, verified by SHA-256 before extraction.
The checker verifies the extracted file tree before using the cache.
The grammar adapter compiles with the pinned Java release, all compiler warnings enabled, and warnings treated as errors.
Its compile dependencies are explicit.
The Java process uses one active processor, a bounded heap, UTF-8, and a fixed locale and time zone.

The English and Chinese spaCy models use the `MIT` license.
The Japanese spaCy model uses the `CC-BY-SA-4.0` license.
OpenCC uses `Apache-2.0`, and LanguageTool uses `LGPL-2.1-or-later`.
Dependency installations keep their upstream notices and licenses.

Kani checks the deterministic decision core and rejecting probes.
Native regression fixtures check the external engines and source projection.
These checks establish their stated contracts rather than a proof of linguistic or factual correctness.
