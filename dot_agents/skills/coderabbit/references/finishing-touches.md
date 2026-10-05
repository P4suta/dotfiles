# Delivery

Confirm that the authorization covers both the GitHub command and its delivery mode.
Post these commands only with authorization:

| Purpose | Command | Delivery |
| --- | --- | --- |
| Fix CodeRabbit findings | `@coderabbitai autofix` | Inspect the offered task and changes. |
| Fix supported CI failures | `@coderabbitai fix-ci` | Normally a stacked PR, or an in-place update of an existing CodeRabbit-authored PR. |
| Fix CI directly on the current branch | `@coderabbitai fix-ci commit` | Direct commits require commit and push authorization. |
| Generate meaningful tests | `@coderabbitai generate unit tests` | Inspect the offered branch or PR delivery. |
| Resolve merge conflicts | `@coderabbitai resolve merge conflict` | Commits a resolution to the branch. |

Only the first eligible turn may cost nothing.
Steering, revision, retries, and follow-up turns can incur charges.
Never turn on the Agent add-on or a trial to continue.

The CI fixer never changes protected workflow files, build or CI configuration, dependency manifests, or lockfiles.
For those files, use its diagnosis for authorized local fixes.
A stacked PR targets the current PR branch, where the project's CI may skip triggers that match only the default branch.
Check the result with the project's own commands and the original PR's checks.

Accept generated tests only when they exercise observable boundaries, error paths, or a reproduced regression with the existing test framework.
Reject tests that repeat the implementation, weaken existing checks, or add coverage for trivial prose changes.
