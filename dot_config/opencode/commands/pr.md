---
description: Open a GitHub pull request for the current branch
agent: build
---

Open a pull request for the current branch.

1. Check `git status` and `git log <base>..HEAD` to understand the branch, where base is the repository's default branch.
2. Load the `pull-request` skill and create the pull request with `pr-workflow create`.
3. Print the resulting PR URL.
