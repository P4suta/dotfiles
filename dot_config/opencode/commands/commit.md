---
description: Commit staged changes with a Conventional Commit message
agent: build
---

Create one commit from the currently staged changes.

1. Inspect the staged changes with `git diff --staged --stat` followed by `git diff --staged`.
   If nothing is staged, show `git status` and ask me what to stage — do not stage anything yourself.
2. Write an English Conventional Commit message:
- `type(scope): summary` — include the scope only when it clarifies
- imperative mood, all-lowercase summary, no trailing period, max 72 chars
- add an optional body that explains why, not what
3. Commit with `git commit` using a heredoc for the message.
4. Show `git log -1 --stat` to confirm.
