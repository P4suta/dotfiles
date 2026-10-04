---
name: doppler
description: >-
  Configure and use Doppler for local commands, shared project secrets, and GitHub Actions with OIDC, scoped service tokens, or native GitHub Secrets sync.
---

# Doppler

Use the existing Doppler workplace through its official CLI or API.
This skill is shared by Claude Code, Codex, and OpenCode; use the host's tools rather than requiring a client-specific plugin.

## Find the existing setup

Read the target repository's instructions, Doppler configuration, and secret-consuming workflows before changing access.
Identify the project, config, required secret names, and existing approval boundaries.
Use `mise x -- doppler` when mise manages the CLI.
Check `doppler --version` before configuring a new login.
For authentication, capture `doppler me --json` and report only workplace metadata and the authentication type; do not print its `token_preview` field.
If authentication is missing, let the user complete `doppler login` for the intended workplace.
A directory-scoped login is useful when different workplaces share one machine.
For an explicit `--scope`, use a resolved absolute directory or the host shell's correct home-directory syntax; do not assume Bash variable expansion in every shell.

Inspect project metadata and secret names first:

```bash
mise x -- doppler projects
mise x -- doppler secrets --only-names --project PROJECT --config CONFIG
```

Use metadata-only inspection by default.
Do not fetch secret values for inventory or readiness checks.
Retrieve values only for an explicitly authorized consuming command, without returning them to the agent's transcript.
Do not dump `doppler configure`, use `--print-config`, or print a config's secrets to diagnose authentication.
CLI credentials and encrypted fallback files under `~/.doppler` are machine-local and excluded from dotfiles.

## Run local commands

Select the project and config for the repository directory with `doppler setup --project PROJECT --config CONFIG --no-interactive`.
A committed `doppler.yaml` may contain these non-secret defaults; it must not contain an access token.
Run the consuming command through Doppler rather than writing a plaintext `.env` file:

```bash
mise x -- doppler run --project PROJECT --config CONFIG -- COMMAND
```

Use `--only-secrets NAME1,NAME2` when the command needs a subset.
For signing or release verification that requires current authorization and values, add `--no-fallback` so a cached config cannot hide a failed fetch or revoked token.

## Import or share credentials

The user manages 1Password directly; do not access it through agent tools or ask for its contents.
For owner-managed setup, prepare missing keys as empty values with explanatory notes, configure references, and provide direct Config links.
Preserve existing fields and make consuming commands reject empty values.
The user then fills and saves the prepared fields in Doppler.
Inspect names and access metadata after registration; verification must not retrieve values into the transcript.
If the user explicitly authorizes another import source, validate required names and nonempty values before writing to the specified destination.
Use a Rust adapter with the pinned official CLI or API, keeping source values and response bodies private in memory.
For Linux CLI uploads, send a selected JSON map through piped stdin to `doppler secrets upload /dev/stdin --silent --project PROJECT --config CONFIG`.
Pass the scoped token through the child environment, never a command argument.
Preserve multiline values, compare source and destination privately, and reject missing or conflicting existing fields.
Do not expose secret values through chat, command arguments, shell history, logs, or tracked files.

Use cross-project secret references for credentials intentionally shared by several projects.
Keep each consuming config limited to its purpose; avoid importing an entire shared config when only a few fields are needed.
A missing Doppler reference can remain as a literal `${project.config.SECRET}` string, so validate required values before consuming them.
Keep project-specific names, credential references, and migration evidence in that repository rather than this shared skill.
For signing, use the code-signing skill when available and preserve existing certificates and provider credentials.

## Connect CI

Read [GitHub Actions authentication](references/github-actions.md) when configuring or migrating CI.
Prefer OIDC when the runner provides a suitable identity; scoped read-only Service Tokens and native GitHub Secrets sync are also supported choices.
Do not treat OIDC as a prerequisite for adopting Doppler.
Check current plan limits for the chosen architecture rather than assuming non-human identities are unlimited because they have no seat charge.

[Official CLI guide](https://docs.doppler.com/docs/cli) and [secret references](https://docs.doppler.com/docs/secrets) are the sources for current behavior.
