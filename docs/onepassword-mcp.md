# Configure 1Password

The agent configuration uses the desktop app's [1Password Environments MCP Server](https://www.1password.dev/environments/mcp-server).
The server manages Environments and secret names without returning stored secret values.

Local Mac and Linux profiles register the integration when the official server is executable through the client's search path.
The server command is `1password-mcp`.
Windows, WSL, SSH sessions, and installations without the executable receive no registration.
Codex removes its managed server registration when the executable becomes unavailable, and Claude Code disables its managed plugin.

In 1Password, enable **Settings > Developer > Integrate with MCP clients**, apply the agent settings, and restart the clients.
Approve connections in the 1Password app when prompted.
Credentials and approval state stay on the machine.

Codex and OpenCode launch the resolved executable through `stdio`.
Claude Code registers the [official 1Password plugin](https://github.com/1Password/1password-claude-plugin) and its marketplace through user settings.
The settings merge preserves other plugins and preferences.
The plugin includes the MCP configuration and a skill that describes how to manage Environments.

Profile manifests render with an empty tool path, so the integration remains absent from those portable snapshots.
The desktop integration check exercises installed and missing executables and local and SSH sessions.
