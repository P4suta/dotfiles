---
description: Review the current branch against its base branch
agent: reviewer
---

Review every change on the current branch.

1. Determine the base branch: `git symbolic-ref --short refs/remotes/origin/HEAD`, falling back to `main`, then `master`.
2. Diff the whole branch: `git merge-base <base> HEAD`, then `git diff <merge-base>...HEAD`.
3. Apply your review rules: severity-tagged findings with `path:line` and concrete fixes, no style noise.
4. Order findings by severity and end with a one-line overall verdict.
