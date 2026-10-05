---
description: Commit staged changes with a Conventional Commit message
agent: build
---

Create one commit from the staged changes.

1. Inspect them with `git diff --staged --stat`, then `git diff --staged`.
   With nothing staged, show `git status` and ask what to stage.
   Never stage anything yourself.
2. Write an English Conventional Commit message:
- `type(scope): summary`, with the scope only when it clarifies
- imperative mood, all-lowercase summary, no trailing period, at most 72 characters
- an optional body explaining why, not what
3. Commit with `git commit`, passing the message through a heredoc.
   Never append attribution footers.
4. Show `git log -1 --stat` to confirm.

Reply in Japanese.
