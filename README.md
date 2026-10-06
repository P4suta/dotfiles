# Dotfiles

One chezmoi source for Mac, Linux, Windows, and WSL.
Provisioning, refresh, hooks, validation, and rollback run through Rust in `xtask/` and the native Rust helpers.
Shell and PowerShell launchers only forward arguments.

Claude Code, Codex, and OpenCode share the skills in `~/.agents/skills`.

Install mise, Git, and chezmoi, then run `mise install` here.
Keep the native profile and Git identity in a private configuration outside this checkout, which `chezmoi init` writes from [the template](home/.chezmoi.toml.tmpl).
It detects `mac`, `linux`, `windows`, or `wsl` from this host and asks for the role, the Git identity, and `paths.projects`, the native checkout directory.
Supply a public SSH signing key from 1Password, which keeps the private keys.

```text
just check
just proofs
just diff CONFIG_ABSOLUTE_PATH NATIVE_HOME_ABSOLUTE_PATH
just apply CONFIG_ABSOLUTE_PATH NATIVE_HOME_ABSOLUTE_PATH FRESH_BACKUP_DIRECTORY
just refresh CONFIG_ABSOLUTE_PATH
```

`just apply` keeps chezmoi's state database beside the configuration, so a plain `chezmoi apply` reads the same record of the setup steps that ran.

Doppler holds API secrets.
Authenticate the Doppler CLI locally, and set its project, config, and per-agent secret names in the private configuration.
OpenCode's GitHub MCP server reuses the read-only `gh auth login` instead.
Run `dotfiles-xtask --root SOURCE agent opencode --config CONFIG --` for the configured OpenCode secrets, or `run-secrets --project PROJECT --config CONFIG --names API_KEY -- COMMAND` for an explicit consumer.
The secret runner requires resolved, nonempty values, and disables Doppler fallback files.
Each tool's login state stays machine-local.
See [the architecture and verification boundaries](docs/adr/0009-public-native-profiles.md).
