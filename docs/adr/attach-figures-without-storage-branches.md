---
status: accepted
date: 2026-11-07
---
# Attach figures without storage branches

## Context

Figure branches accumulate, and mandatory diagrams reject changes that prose explains.
[GitHub CLI attachments](https://docs.github.com/en/github-cli/github-cli/attaching-files-with-github-cli) replace local references and preserve alternative text.

## Decision

Render D2 locally and attach validated local images through the checked document workflow.
Keep editable D2 in a [collapsed section](https://docs.github.com/en/get-started/writing-on-github/working-with-advanced-formatting/organizing-information-with-collapsed-sections).
Require diagrams for relationships needing visual explanation.
Keep descriptions and explanations under [W3C guidance](https://www.w3.org/WAI/tutorials/images/complex/).
Migrate references resumably, preserve source and image bytes, and refuse deletion before complete reference verification.
Scoped deployment preserves unrelated installed content and prior review records.
With experimental maintenance disabled, record current installation identities without granting review credit.

## Alternatives

An asset repository adds access policy.
Per-repository branches accumulate.

## Consequences

Publication requires push access and attachment support.
Inspect partial upload failures before retrying.
Preserve concurrent edits and verify branch identity before deletion.
This decision supersedes the storage and universal diagram requirements in [the D2 decision](draw-personal-prs-and-issues-as-monochrome-d2-images.md).

## Confirmation

Test attachments, retained source, prose-only documents, upload failures, and unsafe retirement.
Verify project gates and installation on Mac, Linux, and Windows.
