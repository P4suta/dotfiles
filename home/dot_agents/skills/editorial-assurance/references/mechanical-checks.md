# Mechanical editorial checks

Use the installed `prose` command.
These checks read documents without rewriting them or calling a hosted model API.
The shared house policy also applies to replies, persisted prose, human-readable metadata, and both editorial profiles.
Its native Japanese pass enforces the configured orthography rules and its token pass rejects the defined demonstratives and technical term variants.
The shared structural pass rejects authored lists and tables in Markdown and rendered HTML while preserving code, metadata, and attributed quotations.
Representation choice, diagram meaning, and interaction quality still need editorial review.
`dotfiles answer check` calls the same policy before retaining a successful answer receipt.

## Editorial candidates

```sh
prose editorial --profile expository --report editorial-pass-one.json article.md
prose editorial --profile general --format json essay.md
prose editorial --profile detailed-report --report detailed-report-pass-one.json report.md
```

The expository profile checks authored Markdown body text for recognized personal reactions, vague benefits, research-process declarations, generic admonitions, and transfers of promised work to the reader.
It also finds exact paragraph repetitions of at least 60 characters and block quotations without an attached source link, adjacent source caption, or linked source footnote.
Source links can use Markdown references.
The general profile preserves intentional personal voice and quotations used for non-attribution purposes.
All profiles identify unfinished markers, empty sections, empty bodies, and unresolved explicit Markdown references.
The `detailed-report` profile includes the expository checks and refuses reports below the required explanation length.
Its source records include `authored_body_characters`, counting non-whitespace Unicode characters in unquoted body paragraphs.
Code, quotations, headings, metadata, captions, and link destinations contribute no characters to that length.
Run the detailed profile before delivering a detailed technical report even when the general expository pass is clean.

Fenced code, inline code, link destinations, metadata, and third-party quotations don't trigger authored-text phrase rules.
Human-readable metadata remains subject to the shared house policy.
Visible HTML and metadata need the publication and language passes.
Phrase matching covers selected Japanese, English, and Chinese formulations rather than every paraphrase.
A nearby link establishes a citation's structural presence.
It doesn't establish quotation fidelity or support for a claim.
Opening the cited record and comparing its passage remain part of editorial review.
The native annotations describe grammar and leave technical meaning and project authority to source-based editorial review.
Apply the source, meaning, attribution, and link-purpose criteria to the finished prose before reporting editorial completion.
A passing matcher test doesn't establish that the selected editorial criteria cover the writing task.

Each report records its profile, input paths, SHA-256 source hashes, projected block counts, rule identifiers, source locations, snippets, and suggested repairs.
Reports use new paths and never overwrite an earlier pass.
Exit zero means the checker found no candidates.
Exit one means the checker found candidates.
Exit two means an input or execution failed.
An empty body can't receive a clean result.
Treat a phrase finding as a candidate and read its context before changing the sentence.
Preserve justified wording and record your reason for keeping a candidate instead of corrupting the text to get zero findings.
Repeat the command after each source change and record its actual exit status.

## Native-language copyediting

```sh
prose copyedit --language ja --report language-pass-one.json article.md
prose copyedit --language en --report english-pass-one.json article.md
prose copyedit --language zh --report chinese-pass-one.json article.md
prose copyedit-verify --report language-pass-one.json
```

Select the language explicitly.
The existing locked engines supply linguistic annotations, native style checks, paragraph reuse, and dependency-chain candidates.
They cover prose, human-readable metadata, captions, and table cells.
The first explicit run can install the pinned local runtime.
An engine failure or incomplete coverage fails the run.

Fix real diagnostics and create a new report.
For a reproducible review-severity false positive, use the existing exact-report decision workflow.

```sh
prose copyedit-review --report language-pass-two.json --output language-decisions.json
prose copyedit-verify --report language-pass-two.json --decisions language-decisions.json
```

Complete every pending decision with a specific reason and literal source evidence covering that diagnostic.
This workflow can't waive error-severity diagnostics.
Verification checks source and configuration identities and reruns the engines.
Old decisions can't justify a changed draft.
The decision records an editorial judgment rather than proving that the sentence is natural or factually true.

## Other contracts

Use the repository's existing `prose check` policy for its supported persisted-English channels.
Use the publication project's own Markdown, metadata, asset, link, build, and preview checks.
Use the actual compiler, runtime, test suite, or calculator for executable and numeric claims.
Run `dotfiles answer check` where the owner's answer-evidence contract applies.
None of these tools replaces the claim-source comparison, the completed explanation, or inspection of the rendered proof.
