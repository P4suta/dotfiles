---
name: diagram-explanations
description: >-
  Explain mechanisms, changes, and decisions with source-grounded diagrams.
  Use proactively when readers need to follow relationships, sequences, states, boundaries, or alternatives in a PR, issue, document, or reply.
---

# Diagrams that answer a reader's question

Look for useful diagrams while researching and explaining a change, before the reader asks for one.
Use a diagram when it makes a relationship, intermediate operation, or consequential difference directly inspectable.
Choose the representation with [editorial-assurance](../editorial-assurance/SKILL.md) and apply [monochrome-design](../monochrome-design/SKILL.md).
A diagram has a purpose beyond satisfying a document gate.

## Establish the explanation

State the question the reader must answer and the specific takeaway before drawing.
Read the actual implementation, diff, specification, or evidence for the objects and relationships.
Trace each consequential node and edge to that evidence in the task's existing working record.
Keep observed behavior, proposed behavior, and assumptions distinct.
Don't invent a pipeline from a PR description's sequence of steps.

Choose a sequence for interactions over time, a state diagram for permitted transitions and failure recovery, and a dependency or boundary diagram for ownership and coupling.
For a behavioral fix, show the triggering input and the differing result before and after the change.
Keep both sides at the same abstraction level and under the same conditions.
The [publication storage example](assets/pr-image-publication.d2) illustrates matched views of one actual mechanism.
Adapt a model to its subject instead of copying the example into unrelated changes.
Use a flowchart when decisions and branches determine the outcome.
Use a screenshot for appearance and a chart for measured quantities.
Use complementary diagrams when different questions need different views.

## Make the relationships explicit

Apply the [C4 notation guidance](https://c4model.com/diagrams/notation) to software architecture diagrams and its [review checklist](https://c4model.com/diagrams/checklist) to review their meaning.
Give each diagram a title naming its type and scope.
Use D2's [fixed positions for titles and keys](https://d2lang.com/tour/positions/) to keep explanatory text outside the relationship layout, then check the rendered result.
Name each element, its role, and the responsibility relevant to the question.
Label each relationship with the action or information it carries and make its direction agree with that label.
Explain the meaning of shapes, boundaries, line styles, and abbreviations in a key.
State protocols and technologies where they determine an architectural relationship.
These are informative recommendations adopted as personal policy for applicable diagrams, not requirements to use C4 for every subject.

Use names from the subject instead of generic boxes such as Input, Process, Validate, and Output when the actual objects matter.
Include decision conditions, rejected paths, handoffs, and failure outcomes when they explain the change.
Show what changed.
A generic lifecycle copied across unrelated changes doesn't explain their different mechanisms.
Remove a node or edge if its removal leaves the answer to the reader's question unchanged.
Don't use a graph as a picture of a prose list.

## Render and verify

Use D2 for static technical diagrams and follow its [connection syntax](https://d2lang.com/tour/connections/).
In personal PR and issue work, prepare the source with `pr-workflow diagram --repo OWNER/REPO --source FILE.d2 --alt TEXT` and include the printed local image reference and source section in the body.
The checked document command attaches local images without creating a figure branch.
Keep editable source with the published image.
Provide a concise alternative text and an accompanying explanation of the essential relationships, following the [W3C complex-image guidance](https://www.w3.org/WAI/tutorials/images/complex/).

Open the rendered diagram at the size readers see in its destination.
Check labels, crossings, arrow direction, clipping, reading order, and whether each distinction communicates meaning without color.
Compare a representative path and the relevant failure or changed path with the source.
Read the diagram without relying on an unstated narration, then check that the surrounding prose supplies its conditions and limitations.
Passing rendering and palette checks doesn't establish that a diagram answers its stated question.
Repair a diagram that fails to answer its question.
Read back a published document and verify its attachments and editable source.
