# Web interface evaluation

## Acceptance before implementation

Define the intended user outcome and the complete affected journey before changing the interface.
Inventory entry points, pages, reusable elements, external handoffs, empty and unavailable states, and the result of every action.
Distinguish the operator's confirmed capabilities from a booking system's actual mechanism.
Read the governing primary sources for the current task.
Use the project's maintained checks and required design records without creating a parallel reporting system.
Create a separate assessment only when the user requests it.

Choose applicable criteria from their exact source passages and conditions.
Verify observable requirements with appropriate checks and rejecting examples.
Keep the verification confirmation short and grounded in actual results.
An unverified applicable criterion remains open.
Update the criteria when the intended experience changes, and invalidate affected results.
Don't describe advisory wording as normative when personal policy supplies the stronger adoption.

## Standards and applicability

Review every Level A and Level AA success criterion in the current [Web Content Accessibility Guidelines (WCAG) 2.2 Recommendation](https://www.w3.org/TR/WCAG22/).
Determine which conditions exist in the affected artifact, including externally hosted steps needed for its task.
Select other criteria and recommendations when the task or personal policy makes them applicable.
Use [W3C Understanding documents](https://www.w3.org/WAI/WCAG22/Understanding/) to interpret conditions without replacing the normative specification.
Check the [full-page and complete-process requirements](https://www.w3.org/TR/WCAG22/#conformance-reqs) before making a conformance claim.
An automated scan, local proof, or tested landing page supplies no evidence for an unchecked external process.

For a Japanese-language public-facing browser interface, read the relevant Digital Agency typography, layout, control, icon, and accessibility pages.
Adopt relevant reading and interaction guidance without presuming that government branding or public-service process rules govern a private interface.
For task structure, consult relevant primary guidance, such as the [British government layout guidance](https://design-system.service.gov.uk/styles/layout/), [United States Web Design System principles](https://designsystem.digital.gov/design-principles/), and [Nielsen Norman Group heuristics](https://www.nngroup.com/articles/ten-usability-heuristics/).
Keep each source's conditions and alternatives rather than assembling incompatible examples into one mandatory layout.
Treat a visual reference as prior art, not as an accessibility authority.

## Content and reading structure

Review the primary task, information priority, grouping, language, fees, prerequisites, waiting state, and next action across the whole journey.
Keep essential information visible at the decision point.
Use the [British government accordion guidance](https://design-system.service.gov.uk/components/accordion/) to assess whether hiding content has supporting evidence.
A convenient disclosure doesn't justify hiding costs or service limitations.
Preserve meaningful relationships with headings, lists, definition lists, and actual table semantics.
Use the single reading column and typography preferences in `monochrome-design`.
Keep independently maintained facts in a shared model.

Set text, line height, line length, target size, contrast, and layout thresholds before implementation.
Distinguish the [Digital Agency typography baseline](https://design.digital.go.jp/dads/foundations/typography/) from a stronger owner text preference.
Don't select the smallest permitted size merely because an automated checker accepts it.
The [button guidance](https://design.digital.go.jp/dads/components/button/usage/) and [icon guidance](https://design.digital.go.jp/dads/foundations/icon/accessibility/) govern applicable control and label choices.
An icon must clarify a recognizable operation and keep a visible label and one shared interactive target.

## Interaction and responsive checks

Check names, roles, relationships, reading order, focus order, focus visibility, target geometry, and the resulting state of each action.
Prefer semantic HTML and verify its actual keyboard behavior.
Apply the [Authoring Practices Guide disclosure navigation guidance](https://www.w3.org/WAI/ARIA/apg/patterns/disclosure/examples/disclosure-navigation/) according to its interaction conditions.
Don't assign the menu widget role to ordinary navigation links merely because the visible label says menu.
Read the [HTML disclosure contract](https://html.spec.whatwg.org/multipage/interactive-elements.html#the-details-element) before choosing native disclosure behavior.
Navigation opened by explicit activation has different dismissal conditions from content shown on hover or focus.

Choose representative narrow, intermediate, and wide widths from the content's actual breakpoints.
Include the [320 CSS pixel reflow condition](https://www.w3.org/WAI/WCAG22/Understanding/reflow.html), [200 percent text enlargement](https://www.w3.org/WAI/WCAG22/Understanding/resize-text.html), and [specified spacing overrides](https://www.w3.org/WAI/WCAG22/Understanding/text-spacing.html).
Check portrait and landscape layouts, keyboard operation, relevant contrast modes, resource failure, and app JavaScript being unavailable.
Add motion, authentication, input, validation, timing, and transaction checks only where the corresponding behavior exists.
Record a reason for each exclusion.

## Gates and direct review

For implementation or publication, use the project's maintained commands to enforce observable criteria, including native controls without styling classes.
For a proposal or review, distinguish planned checks from completed checks without introducing unrequested project tooling.
Exercise a representative failing input and a valid example for every added checker rule.
Check the actual outcome rather than matching a heading, selector name, or assertion count.
Preserve diagnostics, traces, and source identity for failures and final passing checks.
Keep semantic suitability, source interpretation, icon meaning, and the primary task as explicit direct review obligations.
Read every affected page and inspect the relevant rendered states.
State which assistive technologies were actually exercised and which remain unchecked.

When a design source is part of the task, reconcile its content, reusable components, typography, tokens, and responsive layouts with the implementation.
Keep the design editable and inspect the composed result.
A flattened screenshot doesn't establish synchronization of editable design objects.
Complete the applicable checks on the current source before publication or a claim that implementation is complete.
Delivery of a proposal or assessment requires an honest record of its findings and limits, not an unauthorized live transaction to remove an external unknown.
Report actual limits without representing a partial assessment as conformance.
