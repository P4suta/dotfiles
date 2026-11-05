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

## Interface fidelity

Match the requested degree of GitHub interface fidelity.
Choose GitHub patterns that express the artifact's actual content and relationships.
Each borrowed interface element must provide working navigation, an available action, a documented status, or an explanation of the content.
Don't add inert search fields, unrelated repository tabs, invented file hierarchies, or imitation controls solely for visual resemblance.
Read the applicable [Primer component guidance](https://primer.style/product/components/) for the task's structure, labels, icons, and states.
For a document, use descriptive headings and a working table of contents based on [Primer's navigation guidance](https://primer.style/product/components/nav-list/).
Use repository controls, pull request states, and Actions views only when their actual functions or documented meaning belong to the artifact.
Use documented activity and real project status for counters and badges.

## Canvas

Use a 1920 by 1080 slide.
For an editorial slide layout, use 96-pixel side margins, place the header across the top 96 pixels, the title at y 150, and the body from y 280.
For a repository-interface composition, follow GitHub's header, navigation, and content-pane hierarchy while keeping the slide text readable.
Right-align a reference to the appendix page at y 1010 with its right edge at x 1824.
Leave the area below the body empty instead of stretching content to fill it.

## Tokens

Bind every color, space, radius, and border width to a variable instead of a literal value.
Take each color value from the light theme of Primer's published primitives, or from the variables of an existing deck in this style.

Text uses `fg/default`, `fg/muted`, `fg/subtle`, and `fg/onEmphasis`.
Surfaces use `canvas/default`, `canvas/subtle`, and `canvas/inset`, with `border/default` and `border/muted` for boundaries.
Navigation uses `nav/underline` for the current tab.

Each semantic tone has `fg`, `emphasis`, `muted`, and `subtle` tokens under its prefix.
The prefixes are `accent`, `success`, `done`, `attention`, `severe`, and `danger`.

Spacing follows an 8-pixel unit: `space/1` is 8, `space/2` is 16, `space/3` is 24, `space/4` is 32, `space/5` is 40, `space/6` is 48, `space/8` is 64, and `space/12` is 96.
Corner radii are 8 for `radius/small`, 12 for `radius/medium`, 20 for `radius/large`, and 999 for `radius/full`.
Border widths are 2 for `borderWidth/default`, 4 for `borderWidth/thick`, and 6 for `borderWidth/accent`.

Give each tone one meaning across the deck.
Use success for an improved or passing state, danger for the state before a change or a failure, and attention for waiting or burden.
Use done for AI actions and merged results, and accent for the current selection or a neutral highlight.
Remove a color that carries no meaning.

## Type

Set Japanese text in Noto Sans JP, interface text and numerals in Mona Sans, and code or branch names in JetBrains Mono.

Display uses Noto Sans JP Bold at size 100 with a 120% line height.
Title uses the same font at 68 with 128%, Heading at 44 with 136%, and Subheading at 34 with 145%.

Body Large uses Noto Sans JP Regular at 32 with a 165% line height, and Body uses the same font at 28 with 165%.
Caption uses Noto Sans JP Regular at 24 with 150%, and Label uses Noto Sans JP Medium at 22 with 130%.

UI uses Mona Sans Medium at 24 with a 130% line height.
Numeral uses Mona Sans SemiBold at 40 with 110%, and Numeral XL uses the same font at 220 with 100%.
Mono uses JetBrains Mono Regular at 30 with 165%.

Add a Strong variant in Bold only for Label, Caption, and Body Large.

## Components

Build components with auto layout and variable bindings, and compose slides from instances.

A Header combines a repository icon, an "owner / deck" breadcrumb, section tabs with the active tab underlined in `nav/underline`, and an "N / total" page counter exposed as a text property.
Icons follow GitHub octicons at one stroke weight.

A Label is a rounded pill with Accent, Success, Done, Attention, Severe, Danger, and Neutral tones.
A Badge is a filled pill with Open, Merged, Accent, and Default tones.
Counter supplies a count chip, Check supplies a check-circle line, and Progress supplies a stepped progress indicator.

A Card is a bordered surface with Default, Subtle, and Accent styles.
A Callout has a left bar and an icon, with Note, Tip, Important, Warning, and Caution types.

## Page types

Choose the page type from the relationship the content expresses.

A title page identifies the topic, presenter, and period.
An overview uses README-style prose, with status labels attached to the relevant work.

Show a sequence through connected stages, and show a change through its state before and after on danger and success subtle surfaces.
Place a Callout beneath the main figure when one supplementary point needs explanation.

Show results through the actual changes and their pull request states, using a Merged badge for a merged result.
Explain a selection through its alternatives and the documented reason for the chosen option.
Use a framed screenshot with a one-line caption when the real interface provides the evidence.

Vary the page types across consecutive slides.

## Verification

Check each changed slide for clipped text, overlaps, and content outside the canvas.
Check that a token binds every fill, stroke, and text color.
Update every page counter after you add, remove, or reorder slides.
Capture a screenshot of each changed slide and inspect it before delivery.
