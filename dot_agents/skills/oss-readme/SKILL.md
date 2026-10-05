---
name: oss-readme
description: >-
  Write or simplify an OSS project's README to the minimum needed to understand, install, and use it.
  Use for README changes, not architecture records or a full documentation site.
---

# Minimal OSS README

Write for someone deciding whether to use the project and trying it for the first time.
Keep only the project name, a concrete purpose, a supported installation command, the smallest useful example, essential prerequisites or limitations, and the license.
Include a link only when the reader needs it to complete those steps.
A library's useful example is its smallest consumer-facing API call; a CLI's is a real invocation.
Do not force headings or a section for a subject that needs no explanation.

Verify commands, package names, platform support, and licenses against the repository and current distribution before writing them.
Prefer the recommended installation path and one working example over a catalogue of alternatives.
Mention a limitation only when omitting it would make the first attempt misleading or fail.
Do not invent installation methods, features, or maintenance guarantees.

Remove badges, feature inventories, comparisons, roadmaps, implementation tours, release procedures, agent instructions, and development diaries unless the owner specifically asks for them.
Keep design rationale in ADRs and contributor-only commands in the existing contributor or agent instructions.
Do not create companion documentation merely to move deleted README text somewhere else.
Preserve necessary security, compatibility, accessibility, and license information.

Write the README so that `prose check --channel document` accepts it.
Read the rendered result and check its links and commands.
The reader should be able to understand the purpose and reach a useful first result without scrolling through project history.
