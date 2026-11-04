---
name: monochrome-design
description: >-
  Apply the owner's black-and-white design preferences to interfaces, websites, diagrams, documents, and other visual artifacts.
  Use for visual design and design review, with literal copy and explicit typographic hierarchy.
---

# Monochrome design

Apply these defaults to every visual design task.
Explicit requirements for the current artifact take precedence.

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

## Verification

Inspect desktop and narrow layouts with the primary task visible immediately.
For browser interfaces, add design assertions to the project's required check command and its existing browser tests.
Inspect parsed style sheet rules and computed styles, including relevant states and pseudo-elements.
Reject authored colors other than black and white, gradients, shadows, partial opacity, filters, and blending.
Run prohibited fixtures and a valid black-and-white fixture to check that the scanner rejects each prohibited style and accepts the valid example.
Require the primary heading to name the service.
Use native object properties for other artifact formats and state any inspection limit.

Don't test screenshots for exactly two pixel values, because text antialiasing creates intermediate pixels.
Don't claim that a palette check establishes semantic clarity or detects arbitrary poetic language.
Review every visible phrase and visual distinction for its concrete meaning.
Keep the reusable preference here and the format-specific checker in the project's maintained tooling.
