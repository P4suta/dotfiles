---
status: accepted
date: 2026-11-04
---
# Draw personal PRs and issues as monochrome D2 images

## Context

Agents explain pull requests and issues in the owner's repositories in prose alone, and the owner finds them hard to follow.
The owner wants each document to show its flow or structure as a diagram, drawn with a current diagram tool, and ruled out Mermaid.
GitHub offers no API for issue or PR attachments, so an agent can't upload an image the way the web editor does.

## Decision

`pr-workflow diagram` renders a D2 source with the ELK layout and a theme whose every color is black or white.
It refuses a source that sets its own D2 configuration, a color, transparency, an effect, an icon, or an import, and refuses a rendered SVG whose paint leaves black and white.
It stores the SVG and its full source on the repository's `diagrams` branch through the Git data API, named by the SHA-256 digest of the source, and prints the image line for a body.
GitHub serves the image from `https://github.com/OWNER/REPO/blob/diagrams/NAME.svg?raw=true` as `image/svg+xml`, so the document shows it in place.

In a personal non-fork repository, `check`, `create`, `edit`, and the issue commands refuse a body without a described image from that branch.
`ready` and `merge` read an existing body without the diagram rule, so a PR written before the rule still lands.
Every profile pins D2 through the shared mise tools, and the command resolves that pin.

## Consequences

Every new personal PR and issue carries at least one diagram, and its source stays reproducible beside the image.
The `diagrams` branch keeps growing, because a content name never changes and no document loses its image.
The body check reads the address pattern and doesn't confirm that the branch holds the file.
A theme color that a later D2 release adds would render in its default color until the theme maps it, and the image check refuses such a render.
