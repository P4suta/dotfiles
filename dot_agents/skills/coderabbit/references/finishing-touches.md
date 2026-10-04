# Finishing Touches Delivery

Choose a concrete task and confirm that the current authorization covers both the GitHub command and its delivery mode.
Use these provider commands only when posting them is authorized:

| Purpose | Command | Delivery |
| --- | --- | --- |
| Fix CodeRabbit findings | `@coderabbitai autofix` | Inspect the offered task and changes. |
| Fix supported CI failures | `@coderabbitai fix-ci` | Normally a stacked PR; an existing CodeRabbit-authored PR can be updated in place. |
| Fix CI directly on the current branch | `@coderabbitai fix-ci commit` | Direct commits require commit and push authorization. |
| Generate meaningful tests | `@coderabbitai generate unit tests` | Inspect the offered branch or PR delivery. |
| Resolve merge conflicts | `@coderabbitai resolve merge conflict` | Commits a resolution to the branch. |

The initial eligible Autofix, Finishing Touches, merge-conflict, or Security Fix turn may be free.
Access depends on the organization and rollout; an available button is not a promise of unlimited free work.
Steering, revision requests, retries, and follow-up turns can be billed, and paid turns are refused when the Agent add-on is inactive and no other entitlement applies.
Do not activate the add-on or a separate agent trial to continue.

The CI fixer cannot deliver changes to protected workflow files, build or CI configuration, dependency manifests, or lockfiles.
Use its diagnosis for authorized local fixes when those files are involved.
A stacked PR targets the current PR branch, so the project's normal CI may not run there if triggers only match the default branch.
Validate the resulting change with the authoritative project commands and original PR's checks.

Generated tests should exercise observable boundaries, error paths, or a reproduced regression with the existing test framework.
Do not accept tests that merely repeat the implementation, weaken existing checks, or add low-value coverage for trivial prose changes.
Keep generated documentation and source prose subject to the project's concise-source rules.
