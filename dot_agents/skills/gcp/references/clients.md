# Client configuration

The local MCP server reuses existing gcloud user authentication and needs no new OAuth 2.0 client, service-account key, or Application Default Credentials.
Check the current [server documentation](https://github.com/googleapis/gcloud-mcp) before installing or upgrading.
Version 0.5.3 passed verification on October 2, 2026.

Install a reviewed version into a shared user directory with npm's exact-version option and keep its package lock:

```sh
npm install --prefix "$HOME/.local/share/gcloud-mcp" --save-exact --omit=dev --no-audit --no-fund @google-cloud/gcloud-mcp@0.5.3
```

Resolve the absolute Node executable and `node_modules/@google-cloud/gcloud-mcp/dist/bundle.js` path.
Launch Node directly with the bundle as its argument.
Give the server a PATH that includes gcloud and required system tools.
Set `CLOUDSDK_CORE_DISABLE_PROMPTS=1` for noninteractive resource operations.
Keep authentication outside MCP and preserve unrelated client settings when updating configuration.

## Codex

Register a user stdio server with `codex mcp add gcloud --env PATH=SERVER_PATH --env CLOUDSDK_CORE_DISABLE_PROMPTS=1 -- NODE_ABSOLUTE_PATH BUNDLE_ABSOLUTE_PATH`.
The command writes `[mcp_servers.gcloud]` to `~/.codex/config.toml`.
Check it with `codex mcp get gcloud`.
Codex discovers the shared skill at `~/.agents/skills/gcp/SKILL.md`.

## Claude Code

Run `claude mcp add --scope user --transport stdio gcloud --env PATH=SERVER_PATH --env CLOUDSDK_CORE_DISABLE_PROMPTS=1 -- NODE_ABSOLUTE_PATH BUNDLE_ABSOLUTE_PATH`.
The command writes `mcpServers.gcloud` with `command`, `args`, and `env` to `~/.claude.json`.
Check `claude mcp get gcloud` or `/mcp` in a new session.
Link `~/.claude/skills/gcp` to the shared skill directory when Claude Code lacks it.

## OpenCode 1.x

Add the server under `mcp.gcloud` in `~/.config/opencode/opencode.json`:

```json
{
  "mcp": {
    "gcloud": {
      "type": "local",
      "command": ["NODE_ABSOLUTE_PATH", "BUNDLE_ABSOLUTE_PATH"],
      "environment": {"PATH": "SERVER_PATH", "CLOUDSDK_CORE_DISABLE_PROMPTS": "1"},
      "enabled": true,
      "timeout": 30000
    }
  }
}
```

Check `opencode mcp list` and skill discovery with `opencode debug skill`.
OpenCode 1.x reads the shared `~/.agents/skills` location.

## Verify transport and access

Verify MCP initialization, `tools/list`, and one read-only call through `run_gcloud_command`.
A connected transport proves nothing about resource-creation permission, GPU capacity, or credit applicability.

- [Codex MCP](https://learn.chatgpt.com/docs/extend/mcp?surface=cli) and [skills](https://learn.chatgpt.com/docs/build-skills)
- [Claude MCP](https://code.claude.com/docs/en/mcp) and [skills](https://code.claude.com/docs/en/skills)
- [OpenCode MCP](https://opencode.ai/docs/mcp-servers/) and [skills](https://opencode.ai/docs/skills/)
