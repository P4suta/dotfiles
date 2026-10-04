# Dynamic Language Coverage

Choose commands from the project's pinned versions and current official documentation.
The following examples explain separate obligations rather than prescribing permanent products.

For Python, use a strict supported type checker and enforce annotations on parameters, returns, and public attributes.
[Mypy documents](https://mypy.readthedocs.io/en/stable/command_line.html) separate controls for untyped definitions, unchecked function bodies, dynamic types, and missing imports.
Confirm the actual flags enabled by the pinned strict mode and check discovered modules and imports.
Use an annotation lint such as [Ruff's annotation rules](https://docs.astral.sh/ruff/rules/#flake8-annotations-ann) when the chosen checker permits unannotated boundaries.
Typed code importing a dynamically typed dependency still needs trustworthy stubs or a validated narrow adapter.

For JavaScript or TypeScript, prefer typed implementation and enable the current [strict compiler checks](https://www.typescriptlang.org/tsconfig/strict.html).
For retained JavaScript, require checked JSDoc contracts and include the actual JavaScript files in the type-checking project.
Strict inference does not itself require explicit public return annotations, so enforce the owner's annotation requirement through a compatible semantic lint where needed.
Keep unchecked `any`, type assertions, non-null assertions, ignored diagnostics, and unvalidated input from bypassing the intended contract.

For another dynamic language, select its maintained annotation or signature system, checker, and annotation-presence rules from official sources.
Test both a valid boundary and a deliberately invalid one through the project's real command.
If the checker cannot cover a construct, redesign or isolate that construct and retain an explicit obligation rather than calling the unchecked code typed.
