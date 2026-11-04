# Representation selection

Choose a representation by the reader's task, the information's structure, and the publication medium before choosing a tool.
The [Australian Government Style Manual](https://www.stylemanual.gov.au/content-types/images) connects visual choices to user needs, relevance, consistent design, and legibility across devices.
Apply the shared house prohibition of lists and tables throughout authored prose and replies.
Literal source material and machine formats keep their required structure.
Don't invoke image generation or editing models, or delegate their use through another tool or service.
Produce graphics from inspectable code, data, or source imagery with verified provenance.

## Find the explanatory work

Identify what the reader must locate, compare, follow, infer, or explore.
Distinguish exact values, conditions, relationships, operations, state changes, spatial arrangements, and conceptual imagery.
Name the intended takeaway and identify the evidence needed to support it.
Record consequential representation choices and reasons with the working editorial evidence, without adding process commentary to the finished text.

Use connected prose when scope, conditions, attribution, or reasoning carry the explanation.
Use a dependency, flow, sequence, or state diagram when readers would otherwise have to reconstruct the relationships and intermediate operations from prose.
Show the objects, direction, boundaries, and relation types that make the mechanism work.
Number labels may identify states or locations in a diagram, but don't use a graphic as a numbered list of independent statements.

Use a chart when actual quantities, distributions, trends, or uncertainty determine the point.
Select encodings that preserve the relevant comparison, and identify units, scales, denominators, and sources.
Don't invent measurements or use area, distance, duration, or arrow width to imply unsupported quantities.
Keep necessary exact values available in selectable prose when the house style excludes a table.

Use an illustration, photograph, annotated screenshot, or spatial rendering when appearance, location, or physical structure supplies information the reader needs.
Cover illustrations follow the same provenance requirements as body figures.
Keep code and identifiers selectable rather than replacing executable examples with pictures of text.

## Choose interaction

The [Public Health Agency of Canada's design manual](https://health-infobase.canada.ca/design-manual/dataviz/dataviz/types.html) distinguishes showing an established result from supporting exploration.
Choose a static representation for a result or path the reader needs to inspect directly.
Choose interaction when readers need to change meaningful inputs, compare scenarios, advance a state, inspect detail, or explore relationships themselves.
Define what the reader controls and how the resulting state or output answers the task.
Navigation, decorative motion, and hover effects alone don't establish an explanatory benefit.

For an interactive explanation, compare the model, input domain, initial state, transitions, and displayed results with the same evidence as the prose.
Make essential operations available without hover or fine pointer control, and provide a usable textual or static account of the result.
Respect the target's actual support for scripts, embedding, offline use, print, and reduced motion.
When the intended medium can't preserve necessary interaction, choose a faithful sequence of states or a supported companion artifact and explain the essential result in the publication itself.

## Build and review the representation

Original covers, diagrams, charts, and illustrations use GitHub Light unless the task specifies another theme.
Take colors from the official [Primer light palette](https://github.com/primer/primitives/blob/main/src/tokens/base/color/light/light.json5) and [semantic background roles](https://github.com/primer/primitives/blob/main/src/tokens/functional/color/bgColor.json5).
Use the default white background, muted surfaces, dark foreground, subtle borders, and semantic accent colors for their intended roles.
Preserve the actual appearance of source photographs, screenshots, and literal third-party imagery.
Check text, paths, and state distinctions after changing the theme rather than replacing only the outer background.

Select maintained tools that fit the required output and remain editable and reproducible.
Use plotting tools for measured data and diagram or vector tools for relations, labels, and original illustrations.
Use an interactive artifact tool when the reader task justifies interaction and the target supports it.
A newer tool or a fashionable appearance doesn't establish accuracy, usability, or explanatory value.

Give related visuals a consistent type hierarchy, palette, line vocabulary, spacing, and level of detail.
Inspect exports at their intended reading size and on narrow screens, including labels, contrast, clipping, and connection paths.
Use labels and other visible cues alongside color, and keep the main point legible without opening another file.
The [W3C complex-image guidance](https://www.w3.org/WAI/tutorials/images/complex/) requires a short identification and a text alternative covering the essential information.
The surrounding prose can supply the detailed account when its relationship to the image is clear.

Review whether the chosen form actually reduces the work required to understand the intended relationship.
Compare diagrams with the mechanism, charts with the data, and interaction with its state and input model.
Remove redundant visuals and revise a form that obscures the argument, even when the export is attractive.
Repeat the source, editorial, and rendered checks after changing a representation.
