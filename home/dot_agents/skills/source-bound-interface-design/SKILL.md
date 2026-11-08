---
name: source-bound-interface-design
description: >-
  Ground interface structure, form controls, accessibility behavior, and user-facing copy in primary sources read for the task and cited by exact URL.
  Use when designing or reviewing interfaces, forms, accessibility, or interface copy, especially for Japanese public services, and use monochrome-design for visual style.
---

# Source-bound interface design

Base each interface decision on a primary source read for the current task.
Memory and secondary summaries don't count as sources.
Explicit requirements for the current artifact take precedence, and the sources still guide how to meet them.
Treat applicable recommendations as required completion criteria under owner policy.
Preserve the source's own normative status when describing that adoption.

## Applicability and completion

Review the complete user task, information hierarchy, grouping, navigation, controls, states, and copy before refining individual elements.
An isolated visual repair doesn't establish that the interface meets the task.
Read the relevant source passages and connect each applicable decision to its supporting URL.
Verify the applied rule through project checks and direct review, then give a short confirmation.
Keep detailed design records when the project requires them or the user requests them.
Revisit applicability when the task scope changes.

Determine applicability from the audience, platform, task, source conditions, and explicit artifact requirements.
When a source offers alternatives, choose a supported alternative and record why it fits.
Explain each exclusion or conflict with evidence.
Convenience, an advisory word, or a passing automated scan doesn't justify omitting an applicable recommendation.
Never describe informative guidance as a normative source obligation merely because owner policy adopts it.

For implementation and publication tasks, enforce observable requirements in maintained project checks with representative rejecting fixtures and a valid example.
For proposals and reviews, describe the needed checks and distinguish performed checks from planned verification without adding unrequested tooling.
Review information relationships, wording, icon meaning, and task completion directly.
Tests establish only their checked properties.
An unresolved applicable rule, unread governing source, or unperformed required review blocks a claim that the affected implementation satisfies the contract.
Finish a requested proposal or assessment by reporting its findings and verification limits.
Don't claim an implementation or external process passed without the corresponding checks.

## Before proposing or changing an interface

Establish acceptance criteria before the first interface change, covering the complete affected journey, pages, responsive states, and failure outcomes.
For browser interfaces, read [web interface evaluation](references/web-interface-evaluation.md) to derive measurable criteria and review applicability.
Keep source minima, stronger owner preferences, empirical observations, and semantic judgments distinct.
Use the project's required records and check output without creating a parallel evidence inventory.
Use the complete criteria to find omissions before requesting visual feedback.
When feedback changes the intended experience, update the criteria and affected checks before changing the implementation.

List the decisions about page structure, element semantics, form controls, accessibility behavior, and user-facing copy.
Open the relevant source page for each decision with a browser or web fetch and read its current text.
Prefer the specific success criterion, tutorial section, pattern, or component page over a landing page.
Quote the governing rule or cite its exact URL in the proposal, review, commit, or PR that makes the decision.
Reread a page consulted in an earlier task, because these documents change.

## Sources

Web accessibility and semantics:

[Web Content Accessibility Guidelines (WCAG) 2.2](https://www.w3.org/TR/WCAG22/) for success criteria, [Understanding WCAG 2.2](https://www.w3.org/WAI/WCAG22/Understanding/) for their intent, and the [quick reference](https://www.w3.org/WAI/WCAG22/quickref/) for techniques and failures.

The W3C tutorials on [forms](https://www.w3.org/WAI/tutorials/forms/), [images](https://www.w3.org/WAI/tutorials/images/), [tables](https://www.w3.org/WAI/tutorials/tables/), and [page structure](https://www.w3.org/WAI/tutorials/page-structure/).

The W3C [Authoring Practices Guide](https://www.w3.org/WAI/ARIA/apg/) for the roles, states, and keyboard behavior of custom widgets.

The [HTML Standard](https://html.spec.whatwg.org/multipage/) for element semantics, including its [forms section](https://html.spec.whatwg.org/multipage/forms.html).

The Web Accessibility Infrastructure Committee [documents for JIS X 8341-3:2016](https://waic.jp/guideline/jis-x-8341-3_2016/).
This Japanese Industrial Standards (JIS) part aligns with ISO/IEC 40500:2012.
That standard from the International Organization for Standardization and the International Electrotechnical Commission (IEC) is WCAG 2.0.

Japanese public services:

The [Digital Agency design system](https://design.digital.go.jp/dads/), its [components](https://design.digital.go.jp/dads/components/), its [accessibility guidance](https://design.digital.go.jp/dads/guidance/accessibility/), and its [accessibility policy](https://design.digital.go.jp/dads/webaccessibility/).
Read a component's usage and accessibility pages where they exist.

The Digital Agency [web accessibility guidebook](https://www.digital.go.jp/resources/introduction-to-web-accessibility-guidebook).

The Immigration Services Agency and Agency for Cultural Affairs [plain Japanese guideline](https://www.moj.go.jp/isa/support/portal/plainjapanese_guideline.html) for copy addressed to foreign residents.

The Council for Cultural Affairs [recommendation on writing official documents](https://www.bunka.go.jp/seisaku/bunkashingikai/kokugo/hokoku/93650001_01.html) for notation and wording in public-service copy.

Native Apple platform apps:

The Apple [accessibility guidelines](https://developer.apple.com/design/human-interface-guidelines/accessibility) for VoiceOver, Dynamic Type, and hit targets, which web standards don't define for native controls.

## Conflicts and gaps

When sources differ, cite each one, state which governs this artifact, and give the reason.
Meet every applicable source when they can hold together.
A legal, procurement, or contractual commitment governs over a recommendation when they can't.
State the resulting rule and unresolved finding instead of weakening the completion criteria.
For example, the Digital Agency accessibility policy targets JIS X 8341-3:2016 level AA, while W3C encourages WCAG 2.2.
The [WCAG overview](https://www.w3.org/WAI/standards-guidelines/wcag/) states that content conforming to WCAG 2.2 also conforms to WCAG 2.0 and notes the obsolete 4.1.1 Parsing criterion.
When no listed source covers a decision, find another primary source and cite it.
When no primary source covers it, say so and label the choice as judgment.

## Reporting

Pair each decision with its source URL and the rule it applies.
Report the implemented rule, verification result, and any remaining boundary.
Report an unreachable source as unread, and never fill the gap from memory.
Leave visual style to `monochrome-design`, Japanese readings to `japanese-lexicon`, and disputed reasoning to `checked-arguments`.
