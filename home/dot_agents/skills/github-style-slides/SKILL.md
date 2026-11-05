---
name: github-style-slides
description: >-
  Build or restyle a slide deck in the GitHub Light look, with Primer semantic tokens, repository-style headers, and message titles.
  Use when the owner explicitly asks for GitHub-style slides; monochrome-design remains the default otherwise.
---

# GitHub-style slides

Use this style when the owner explicitly asks for a GitHub-style deck.
The monochrome-design skill states that explicit requirements for the current artifact take precedence, so the request selects this style for that artifact only.
Keep monochrome-design for every other visual task.
Use presentation-decks for the brief, the script, the deck structure, and the talk checks.

## Canvas

Use a 1920 by 1080 slide with 96-pixel side margins.
Place the header across the top 96 pixels, the title at y 150, and the body from y 280.
Right-align a reference to the appendix page at y 1010 with its right edge at x 1824.
Leave the area below the body empty instead of stretching content to fill it.

## Tokens

Bind every color, space, radius, and border width to a variable instead of a literal value.
Take each color value from the light theme of Primer's published primitives, or from the variables of an existing deck in this style.

| Role | Tokens |
| --- | --- |
| Text | `fg/default`, `fg/muted`, `fg/subtle`, `fg/onEmphasis` |
| Surface | `canvas/default`, `canvas/subtle`, `canvas/inset` |
| Border | `border/default`, `border/muted` |
| Accent | `accent/fg`, `accent/emphasis`, `accent/muted`, `accent/subtle` |
| Success | `success/fg`, `success/emphasis`, `success/muted`, `success/subtle` |
| Done | `done/fg`, `done/emphasis`, `done/muted`, `done/subtle` |
| Attention | `attention/fg`, `attention/emphasis`, `attention/muted`, `attention/subtle` |
| Severe | `severe/fg`, `severe/emphasis`, `severe/muted`, `severe/subtle` |
| Danger | `danger/fg`, `danger/emphasis`, `danger/muted`, `danger/subtle` |
| Navigation | `nav/underline` |
| Space | `space/1` 8, `space/2` 16, `space/3` 24, `space/4` 32, `space/5` 40, `space/6` 48, `space/8` 64, `space/12` 96 |
| Radius | `radius/small` 8, `radius/medium` 12, `radius/large` 20, `radius/full` 999 |
| Border width | `borderWidth/default` 2, `borderWidth/thick` 4, `borderWidth/accent` 6 |

Give each tone one meaning across the deck.
Use success for an improved or passing state, danger for the state before a change or a failure, and attention for waiting or burden.
Use done for AI actions and merged results, and accent for the current selection or a neutral highlight.
Remove a color that carries no meaning.

## Type

Set Japanese text in Noto Sans JP, interface text and numerals in Mona Sans, and code or branch names in JetBrains Mono.

| Style | Font | Size and line height |
| --- | --- | --- |
| Display | Noto Sans JP Bold | 100, 120% |
| Title | Noto Sans JP Bold | 68, 128% |
| Heading | Noto Sans JP Bold | 44, 136% |
| Subheading | Noto Sans JP Bold | 34, 145% |
| Body Large | Noto Sans JP Regular | 32, 165% |
| Body | Noto Sans JP Regular | 28, 165% |
| Caption | Noto Sans JP Regular | 24, 150% |
| Label | Noto Sans JP Medium | 22, 130% |
| UI | Mona Sans Medium | 24, 130% |
| Numeral | Mona Sans SemiBold | 40, 110% |
| Numeral XL | Mona Sans SemiBold | 220, 100% |
| Mono | JetBrains Mono Regular | 30, 165% |

Add a Strong variant in Bold only for Label, Caption, and Body Large.

## Components

Build components with auto layout and variable bindings, and compose slides from instances.

- Header: a repository icon with an "owner / deck" breadcrumb, section tabs with the active tab underlined in `nav/underline`, and an "N / total" page counter as a text property.
- Label: a rounded pill with Accent, Success, Done, Attention, Severe, Danger, and Neutral tones.
- Badge: a filled pill with Open, Merged, Accent, and Default tones.
- Counter, Check, and Progress: a count chip, a check-circle line, and a stepped progress indicator.
- Card: a bordered surface with Default, Subtle, and Accent styles.
- Callout: a box with a left bar and an icon, in Note, Tip, Important, Warning, and Caution types.
- Icons: line icons in the style of the GitHub octicons, at one stroke weight.

## Page types

Choose the page type from the relationship the content expresses.

- A title page with topic labels, the presenter, and the period.
- A key-value card for an overview.
- A list card whose rows carry status labels, like pull request checks.
- Numbered step cards joined by connectors for a sequence.
- Before and after rows on danger and success subtle surfaces for a change.
- A callout under the main figure for one supplementary point.
- Pull request cards with a Merged badge for a results summary.
- An options list beside an adopted card for a selection.
- A framed screenshot with a one-line caption for a real example.

Vary the page types across consecutive slides.

## Verification

Check each changed slide for clipped text, overlaps, and content outside the canvas.
Check that a token binds every fill, stroke, and text color.
Update every page counter after you add, remove, or reorder slides.
Capture a screenshot of each changed slide and inspect it before delivery.
