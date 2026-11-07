---
status: superseded
date: 2026-11-04
changed_by: [attach-figures-without-storage-branches.md]
---
# Draw personal PRs and issues as monochrome D2 images

## Context

The owner wanted personal PRs and issues to explain their flow or structure visually, using D2 rather than Mermaid.
The original implementation used repository branches for image storage.
The original claim that attachments were unavailable was incorrect.
[GitHub CLI supports attachments](https://docs.github.com/en/github-cli/github-cli/attaching-files-with-github-cli).

## Decision

Render monochrome D2 with ELK and reject unsupported source configuration and rendered paint.
Store SVG and D2 source on each repository's `diagrams` branch under content-derived names.
Require personal non-fork PR and issue authoring to include a described image referencing that branch.
Exempt existing bodies at ready and merge transitions.

## Alternatives

Direct attachments were overlooked in the original decision.

## Consequences

Dedicated branches accumulate, and documents require diagrams regardless of explanatory value.
URL-pattern validation establishes neither file existence nor diagram relevance.
[Direct attachments](attach-figures-without-storage-branches.md) supersede storage branches and universal diagram requirements.

## Confirmation

The original gate checked source configuration, SVG paint, and branch-qualified image URLs.
Diagram meaning required direct review.
