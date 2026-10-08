---
name: oss-readme
description: >-
  Write the repository.toml that generates an owner repository's README, license files, and GitHub description.
  Use for README changes, not architecture records or a full documentation site.
metadata:
  tools: "tools.toml"
---

# README for owner repositories

Every owner repository carries a `repository.toml`, and `dotfiles repository generate` writes the README, `LICENSE-MIT`, and `LICENSE-APACHE` from it.
`dotfiles repository check --remote OWNER/REPO` refuses any drift, and names the command that fixes it.
Never edit the generated files by hand, and keep anything longer than the summary, installation, usage, and license out of the README.

The template can't choose the one-line summary, so write it for someone deciding whether to use the project.
Name the concrete purpose in the reader's terms, keep it to one line without a final period, and let it double as the GitHub description.
Verify the install command and the usage with the repository and its current distribution before writing them, and never invent a feature or an installation method.
Show one working example, never a catalog of alternatives.

A repository takes `MIT OR Apache-2.0`.
Before relicensing a repository that others contributed to, record each contributor's consent under `[[consent]]`.
