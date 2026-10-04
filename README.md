# dotfiles

One chezmoi source for Mac, Linux, Windows, and WSL.
Provisioning, refresh, hooks, validation, and rollback run through Rust in `xtask/` and the native Rust helpers.
Shell and PowerShell launchers only forward arguments.

Shared skills live in `~/.agents/skills`; Claude Code, Codex, and OpenCode use the same source.

Install mise, Git, and chezmoi, then install the pinned tools with `mise install` in this checkout.
Keep the native profile and author identity in a private configuration outside this checkout, using [the example](examples/chezmoi.toml).
Set `paths.projects` to the native checkout directory and select `mac`, `linux`, `windows`, or `wsl`.
Supply a public SSH signing key from 1Password; private keys remain in its agent.

```text
mise run check:rust
mise run check:proofs
just diff CONFIG_ABSOLUTE_PATH NATIVE_HOME_ABSOLUTE_PATH PRIVATE_STATE_DIRECTORY
just apply CONFIG_ABSOLUTE_PATH NATIVE_HOME_ABSOLUTE_PATH PRIVATE_STATE_DIRECTORY FRESH_BACKUP_DIRECTORY
just refresh CONFIG_ABSOLUTE_PATH
```

API secrets belong in Doppler.
Authenticate the Doppler CLI locally and select its project, config, and per-agent secret names in the private configuration.
Run `dotfiles-xtask --root SOURCE agent opencode --config CONFIG --` for the configured OpenCode secrets, or `run-secrets --project PROJECT --config CONFIG --names API_KEY -- COMMAND` for an explicit consumer.
The secret runner requires resolved, nonempty values and disables Doppler fallback files.
Application login state stays machine-local.
See [the architecture and verification boundaries](docs/adr/0009-public-native-profiles.md).
