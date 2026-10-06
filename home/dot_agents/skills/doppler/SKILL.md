---
name: doppler
description: >-
  Configure and use Doppler for local commands, shared project secrets, and GitHub Actions with OIDC, scoped service tokens, or native GitHub Secrets sync.
---

# Doppler

Use the existing Doppler workplace through its official CLI or API.

## Find the existing setup

Read the repository's Doppler configuration and secret-consuming workflows before changing access.
Identify the project, config, required secret names, and approval boundaries.
Use `mise x -- doppler` when mise manages the CLI.
Capture `doppler me --json` for authentication, and report only workplace metadata and the authentication type, never its `token_preview` field.
Without authentication, let the user run `doppler login` for the intended workplace.
For an explicit `--scope`, pass a resolved absolute directory or the shell's own home-directory syntax.

Inspect project metadata and secret names first:

```bash
mise x -- doppler projects
mise x -- doppler secrets --only-names --project PROJECT --config CONFIG
```

Never fetch secret values for inventory or readiness checks.
Retrieve values only for an explicitly authorized consuming command, and keep them out of the transcript.
Never dump `doppler configure`, use `--print-config`, or print a config's secrets to diagnose authentication.
Credentials and encrypted fallback files under `~/.doppler` stay machine-local and out of dotfiles.

## Run local commands

Select the project and config for the repository directory with `doppler setup --project PROJECT --config CONFIG --no-interactive`.
A committed `doppler.yaml` may hold these non-secret defaults, never an access token.
Run the consuming command through Doppler instead of writing a plaintext `.env` file:

```bash
mise x -- doppler run --project PROJECT --config CONFIG -- COMMAND
```

Use `--only-secrets NAME1,NAME2` when the command needs a subset.
For signing or release verification, add `--no-fallback` so a cached config never hides a failed fetch or a revoked token.

## Import or share credentials

The user manages 1Password, so never access it or ask for its contents.
For owner-managed setup, create missing keys as empty values with explanatory notes, configure references, and give direct config links.
Keep existing fields, and make consuming commands reject empty values.
The user fills and saves the prepared fields in Doppler.
Inspect names and access metadata after registration without retrieving values.
When the user authorizes another import source, check required names and nonempty values before writing to the destination.
Write the import as a Rust adapter over the pinned official CLI or API, and keep source values and response bodies private in memory.
For uploads on Linux, pipe a selected JSON map through piped stdin to `doppler secrets upload /dev/stdin --silent --project PROJECT --config CONFIG`.
Pass the scoped token through the child environment, never a command argument.
Keep multiline values, compare source and destination privately, and reject missing or conflicting existing fields.
Keep secret values out of chat, command arguments, shell history, logs, and tracked files.

Use cross-project secret references for credentials that many projects share.
Limit each consuming config to its purpose, and import only the fields it needs.
A broken Doppler reference can survive as a literal `${project.config.SECRET}` string, so check required values before use.
Keep project-specific names, credential references, and migration evidence in that repository.
For signing, use `code-signing`.

## Connect CI

Read [GitHub Actions authentication](references/github-actions.md) when configuring or migrating CI.
Prefer OIDC when the runner offers an identity, with scoped read-only service tokens and native GitHub Secrets sync as supported alternatives.
Check current plan limits for the chosen design, because identities without a seat charge still count toward limits.

See the [official CLI guide](https://docs.doppler.com/docs/cli) and [secret references](https://docs.doppler.com/docs/secrets).
