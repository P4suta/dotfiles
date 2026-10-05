---
description: Open a GitHub pull request for the current branch
agent: build
---

Open a PR for the current branch with `gh`.

1. Read `git status` and `git log <base>..HEAD`, where base names the repository's default branch.
2. Write an English title of at most 72 characters in imperative mood, following the repository's existing title style.
3. Write an English Markdown body:
- `## Summary`: one paragraph
- `## Changes`: a bullet list
- `## Notes`: testing, caveats, follow-ups
4. Run `gh pr create --base <base> --title <title> --body <body>`, passing the body through a heredoc.
5. Print the URL of the new PR.

Reply in Japanese.
