---
description: Open a GitHub pull request for the current branch
agent: build
---

Open a pull request for the current branch using `gh`.

1. Check `git status` and `git log <base>..HEAD` to understand the branch, where base is the repository's default branch.
2. Title: English, max 72 chars, imperative mood; keep the repo's existing title style if one is apparent.
3. Body in English markdown:
- `## Summary` — one paragraph
- `## Changes` — bullet list
- `## Notes` — testing done, caveats, follow-ups
4. Run `gh pr create --base <base> --title <title> --body <body>` using a heredoc for the body.
5. Print the resulting PR URL.

Reply to me in Japanese; the PR title and body stay English.
