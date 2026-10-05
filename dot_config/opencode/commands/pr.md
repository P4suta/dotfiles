---
description: open a GitHub pull request for the current branch
agent: build
---

Open a PR for the current branch.

1. Read `git status` and `git log <base>..HEAD`, where base names the repository's default branch.
2. Load the `pull-request` skill and create the PR with `pr-workflow create`.
3. Print the URL of the new PR.
