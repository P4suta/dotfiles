---
name: monochrome-design
description: >-
  Apply the owner's black-and-white design preferences to interfaces, websites, diagrams, documents, and other visual artifacts.
  Use for visual design and design review, with literal copy and explicit typographic hierarchy.
---

# Monochrome design

Apply these defaults to every visual design task.
Explicit requirements for the current artifact take precedence.
Applicable recommendations are requirements under the common agent policy.
Use `source-bound-interface-design` to establish applicability, trace decisions to primary sources, and resolve conflicts before implementation.

## Task and information structure

Review the complete task and every affected page before changing isolated visual details.
Organize information by its actual relationships and the decisions the reader needs to make.
Use headings, lists, definition lists, and groups with matching semantics instead of concatenating structured data into repeated strings.
Keep repeated facts consistent through a shared model.
Make the primary action, prerequisites, costs, and resulting state clear.
Default reading interfaces to one column, especially on narrow screens.
Extra width alone doesn't justify parallel cards, independent reading paths, or columns of metadata.
Keep a table when the data relationships require its rows and columns.

For public-facing browser content, use an 18 CSS pixel default for body text and action labels, expressed in relative units.
The owner selects 18 pixels as a reading preference.
The Digital Agency's [typography guidance](https://design.digital.go.jp/dads/foundations/typography/) permits a smaller baseline.
Choose a coherent heading scale and bounded line length around the actual content.
Keep browser font preferences, enlargement, and spacing overrides effective.
Use a unitless body line height of 1.75 and separate adjacent reading paragraphs by at least one and a half line boxes.
Keep reading measures near 40 full-width Japanese characters or fewer, while distinguishing prose from compact labels and tabular data.
Use at least 48 by 48 CSS pixels for standalone browser targets.
The target size and typography defaults are personal preferences.
Apply the [inline exception](https://www.w3.org/TR/WCAG22/#target-size-minimum) to prose links.

## Visual language

Use black and white for authored text, surfaces, controls, and rules.
Exclude other colors, gray steps, tints, shades, gradients, shadows, translucent surfaces, faded text, filters, and blending.
Keep visible controls and metadata at full opacity.
Browser text antialiasing and native rendering aren't authored gray styling.

Convey hierarchy through font size, weight, spacing, alignment, grouping, solid rules, and underlines.
Make each visual distinction communicate content, an action, a relationship, or a state.
Remove ornamental badges, decorative symbols, and visual effects that communicate none of these.
Keep selected, inactive, error, and focus states explicit through text, typography, boundaries, or shape instead of color or fading.
Keep labels readable and focus indicators visible.

## Copy

Use the literal service or artifact name as the primary heading.
Write short labels that name the content, operation, or state.
Exclude slogans, poetic hero copy, vague promises, motivational language, and decorative introductory text.
Add explanatory text only when it helps the reader understand the information or make a concrete decision.
Treat icons and imagery as communication, and use them only when their meaning is clear.
Pair icons with visible labels for browser interfaces unless a governing source supports a necessary exception.
Keep an icon and its label in one interactive target, and hide a redundant decorative icon from assistive technology.
An icon never substitutes for readable wording or information structure.
Cite the primary sources in `source-bound-interface-design` for structure, form controls, accessibility behavior, and copy.

## Verification

Inspect desktop and narrow layouts with the primary task visible immediately.
Review the full task, information relationships, visible labels, action outcomes, and relevant states using the source requirements.
Check text resizing, text spacing, keyboard focus, and target size for browser interfaces.
Applicable requirements remain mandatory even when a palette scanner can't check them.
When implementing a browser interface, add design assertions to the project's required check command and its existing browser tests.
Inspect parsed style sheet rules and computed styles, including relevant states and pseudo-elements.
Reject authored colors other than black and white, gradients, shadows, partial opacity, filters, and blending.
Run prohibited fixtures and a valid black-and-white fixture to check that the scanner rejects each prohibited style and accepts the valid example.
Require the primary heading to name the service.
Use native object properties for other artifact formats and state any inspection limit.

Don't test screenshots for exactly two pixel values, because text antialiasing creates intermediate pixels.
Don't claim that a palette check establishes semantic clarity or detects arbitrary poetic language.
Review every visible phrase and visual distinction for its concrete meaning.
Keep the reusable preference here and the format-specific checker in the project's maintained tooling.
