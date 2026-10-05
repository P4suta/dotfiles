# Dynamic language coverage

Choose commands from the project's pinned versions and current official documentation.
The examples below illustrate distinct obligations, not permanent product choices.

For Python, use a strict supported type checker, and enforce annotations on parameters, returns, and public attributes.
[Mypy](https://mypy.readthedocs.io/en/stable/command_line.html) has distinct controls for untyped definitions, unchecked function bodies, dynamic types, and missing imports.
Confirm the flags the pinned strict mode turns on, and check discovered modules and imports.
When the chosen checker permits unannotated boundaries, add an annotation lint such as [Ruff's annotation rules](https://docs.astral.sh/ruff/rules/#flake8-annotations-ann).
Typed code that imports a dynamically typed dependency still needs trustworthy stubs or a validated narrow adapter.

For JavaScript or TypeScript, prefer typed code and turn on the current [strict compiler checks](https://www.typescriptlang.org/tsconfig/strict.html).
For retained JavaScript, require checked JSDoc contracts and include the JavaScript files in the type-checking project.
Strict inference never requires explicit public return annotations, so enforce the owner's annotation rule through a compatible semantic lint where needed.
Keep unchecked `any`, type assertions, non-null assertions, ignored diagnostics, and unvalidated input from bypassing the intended contract.

For another dynamic language, choose its maintained annotation or signature system, checker, and annotation-presence rules from official sources.
Test both a valid boundary and an invalid one through the project's real command.
If the checker misses a construct, redesign or isolate that construct and keep an explicit obligation instead of calling the unchecked code typed.
