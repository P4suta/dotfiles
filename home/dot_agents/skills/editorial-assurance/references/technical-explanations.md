# Technical explanations and release coverage

## Establish the actual release

Record the exact release, publication date, applicable platform, and stable or experimental status from its primary records.
Put those facts in the article when they determine which behavior the reader receives.
Don't link to a release history as a substitute for supplying the dates.
Separate an accepted design, a merged implementation, an experimental option, and a released default using the concrete facts for that feature.

Create a source map covering the release's material changes and promised scope.
For broad coverage, account for every official release-note section before claiming completeness.
Explain major changes individually and group minor changes into connected paragraphs with the actual API, behavior, and affected condition.
Include patch releases when the requested scope includes them.
Don't replace a requested full release article with a few selected highlights.

## Detailed reports

Treat a detailed technical report as a completed investigation with full explanations of its material changes.
Run `prose editorial --profile detailed-report` as a required delivery gate for each report.
The checker defines the required length and reports the non-whitespace character count of authored body paragraphs.
Code, quotations, headings, metadata, captions, and link destinations don't count toward that length.
Passing the length threshold establishes neither complete coverage nor a reason to stop the investigation.

Build the required depth through source-supported mechanisms, previous behavior, worked examples, failure cases, alternatives, and migration consequences.
Account for the official release-note sections and resolve the promised examples and operational questions within the report.
Don't pad the report with repetition, general programming tutorials, unrelated history, oversize code listings, or paraphrased conclusions.
Keep the length result separate from the coverage map, factual review, executable examples, and editorial findings.

## Explain the mechanism

For a design change, establish the previous behavior, the concrete failing or constrained case, the proposed alternatives, the adopted mechanism, and the resulting behavior.
Use the official discussion's example when it demonstrates the actual problem.
Attribute it and identify any adaptation.
Explain each package, type, value, operation, and stage needed to reproduce the reasoning.

For generic methods, show the receiver definition, required interface methods, call through that interface, and concrete type arguments.
Explain where the implementation would generate the executable code.
A sentence saying that code generation was difficult supplies none of those details.
For link-time instantiation, identify the linker's available information and the extra analysis it would need.
Show the case that the documented analysis can't handle.
A runtime alternative needs its proposed mechanism and the documented implementation or performance cost.
Use the evidence to explain the choice rather than attributing an unstated design philosophy to its authors.

For an API change, show the exact old and new input-output behavior.
Distinguish syntax, API shape, implementation, defaults, wire format, and migration effects by stating their observable consequences.
Don't stop after saying that these categories are different.
Show the flag, option, field, or call that selects the behavior.
Preserve relevant errors and boundary conditions.

## Verify examples and impact

Run executable examples with the identified version and options.
Keep the source, command, actual output, and expected result in the working evidence.
Verify an intentionally invalid program by checking its actual failure, not by treating any nonzero exit as the intended diagnostic.
Label pseudocode and historical proposals so readers don't mistake them for code accepted by the current compiler.
Keep snippets complete enough for their stated purpose and explain omitted setup.
Present an executable example's result beside the responsible expression, using valid language comments, assertions, or a separate block of actual output.
For compiler failures, pair the rejected source with the relevant diagnostic and required options.
Keep version and platform conditions with the example when they affect its result.
Replace accounts of running or inspecting an example with the reproducible operation and its outcome.
Use code fences for source, commands, diagnostics, and literal output.
Keep explanations and relationship diagrams in their appropriate prose or graphic form.

Check the workload, baseline, measurement, and source behind performance claims.
Don't transfer a microbenchmark percentage to an app without measurement or a supported argument.
For a diagnostic feature, show what evidence it collects, the algorithm or criterion it uses, and a concrete detected case.
Include a relevant missed case when it determines what the reported result means.
Avoid generic advice that applies equally before and after the release.

Name a tool's version and verify the specific operation before claiming compatibility.
Remove an optional compatibility claim when you can't establish its supporting detail.
Resolve a required compatibility check or report the precise remaining gap outside the article.
Don't write an instruction that readers must check every tool themselves.

## Finish the explanation

Assess the article from the experienced reader's starting knowledge.
Every section must add a fact, mechanism, worked example, supported comparison, or usable result.
Replace “helps understanding” with the actual explanation.
Replace “the specification says” with its relevant content and a source.
Replace “see the documentation for details” with those details when the article promised them.
Remove an unrequested personal conclusion or a list of research questions that the article should have answered.
