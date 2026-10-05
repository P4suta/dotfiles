# Dotfiles

One chezmoi source for Mac, Linux, Windows, and Windows Subsystem for Linux.
Provisioning, refresh, hooks, validation, and rollback run through Rust in `xtask/` and the native Rust helpers.
Shell and PowerShell launchers only forward arguments.

Claude Code, Codex, and OpenCode share the skills in `~/.agents/skills`.

Install mise, Git, and chezmoi, then run `mise install` in this checkout.
Keep the native profile and Git identity in a private configuration outside this checkout, following [the example](examples/chezmoi.toml).
Set `paths.projects` to the native checkout directory and select `mac`, `linux`, `windows`, or `wsl`.
Supply a public SSH signing key from 1Password, which keeps the private keys.

```text
mise run check:rust
mise run check:proofs
just diff CONFIG_ABSOLUTE_PATH NATIVE_HOME_ABSOLUTE_PATH PRIVATE_STATE_DIRECTORY
just apply CONFIG_ABSOLUTE_PATH NATIVE_HOME_ABSOLUTE_PATH PRIVATE_STATE_DIRECTORY FRESH_BACKUP_DIRECTORY
just refresh CONFIG_ABSOLUTE_PATH
```

Doppler holds API secrets.
Authenticate the Doppler command-line tool locally, and set its project, config, and per-tool secret names in the private configuration.
OpenCode's GitHub Model Context Protocol server reuses the read-only `gh auth login` instead.
Run `dotfiles-xtask --root SOURCE agent opencode --config CONFIG --` for the configured OpenCode secrets, or `run-secrets --project PROJECT --config CONFIG --names API_KEY -- COMMAND` for an explicit consumer.
The secret runner requires resolved, nonempty values, and disables Doppler fallback files.
App login state stays machine-local.
See [the architecture and verification boundaries](docs/adr/0009-public-native-profiles.md).
