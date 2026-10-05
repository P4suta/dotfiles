# Finishing touches delivery

Choose a concrete task, and confirm that the authorization covers both the GitHub command and its delivery mode.
Post these provider commands only with authorization:

| Purpose | Command | Delivery |
| --- | --- | --- |
| Fix CodeRabbit findings | `@coderabbitai autofix` | Inspect the offered task and changes. |
| Fix supported CI failures | `@coderabbitai fix-ci` | Normally a stacked PR, or an in-place update of an existing CodeRabbit-authored PR. |
| Fix CI directly on the current branch | `@coderabbitai fix-ci commit` | Direct commits require commit and push authorization. |
| Generate meaningful tests | `@coderabbitai generate unit tests` | Inspect the offered branch or PR delivery. |
| Resolve merge conflicts | `@coderabbitai resolve merge conflict` | Commits a resolution to the branch. |

The first eligible Autofix, Finishing Touches, merge-conflict, or Security Fix turn may cost nothing.
Access depends on the organization and rollout, and an available button promises no unlimited free work.
Steering, revision requests, retries, and follow-up turns can incur charges, and the service refuses paid turns while the `Agent` add-on and every other entitlement stay off.
Never turn on the add-on or a trial to continue.

The CI fixer never changes protected workflow files, build or CI configuration, dependency manifests, or lockfiles.
For those files, use its diagnosis for authorized local fixes.
A stacked pull request targets the current pull request branch, where the project's CI may skip triggers that match only the default branch.
Check the result with the project's own commands and the original pull request's checks.

Accept generated tests only when they exercise observable boundaries, error paths, or a reproduced regression with the existing test framework.
Reject tests that repeat the implementation, weaken existing checks, or add coverage for trivial prose changes.
Generated documentation and source prose follow the project's `concise-source` rules.
