---
name: oss-readme
description: >-
  Write or simplify an OSS project's README to the least needed to understand, install, and use it.
  Use for README changes, not architecture records or a full documentation site.
---

# README for OSS projects

Write for someone deciding whether to use the project and trying it for the first time.
Keep only the project name, a concrete purpose, a supported installation command, the smallest useful example, essential prerequisites or limitations, and the license.
Link only what the reader needs to complete those steps.
For a library, show the smallest consumer-facing API call, and for a CLI, show a real invocation.

Verify commands, package names, platform support, and licenses in the repository and current distribution before writing them.
Show the recommended installation path and one working example, never a catalog of alternatives.
Mention a limitation only when the first attempt would otherwise mislead or fail.
Never invent installation methods, features, or maintenance promises.

Remove badges, feature inventories, comparisons, roadmaps, implementation tours, release procedures, agent instructions, and development diaries unless the owner asks for them.
Keep design rationale in ADRs and contributor-only commands in the existing contributor or agent instructions.
Never create companion documentation to hold deleted README text.
Preserve necessary security, compatibility, accessibility, and license information.

Write the README so that `prose check --channel document` accepts it.
