# Authenticate the GitHub MCP server with the GitHub CLI login

Status: Accepted.

## Context

The Mac OpenCode configuration reaches GitHub's remote MCP server with a bearer token in `GH_MCP_TOKEN`.
[ADR 0009](0009-public-native-profiles.md) routes API secrets through Doppler, and the token was planned as a dedicated fine-grained personal access token that the owner creates, stores in Doppler, and recreates when it expires.
The same agent already runs the GitHub CLI in its shell with the owner's login, whose scopes include repository write access, so a narrower token restricts only the MCP channel while adding a second credential with its own lifecycle.
GitHub's remote server documents personal access tokens and host-registered OAuth applications, not dynamic client registration, and its read-only mode and toolset selection are request headers.
The configuration selected toolsets with an undocumented `?toolsets=` query, and a probe with it received HTTP 400 for `tools/list`.

## Decision

The OpenCode launcher passes the GitHub CLI's token, read with `gh auth token`, as `GH_MCP_TOKEN` in the agent's environment, and starts the agent without it, with a warning, when the CLI is not logged in.
The GitHub CLI's keychain login is the only origin of that credential; selecting `GH_MCP_TOKEN` from Doppler as well is refused.
OpenCode runs MCP servers in a shared background service whose environment was fixed when it started, so the launcher starts the interface and `run` with `--standalone`, a private server that inherits the credential; other subcommands and a launch that chooses its own server pass through unchanged.
The configuration sends `X-MCP-Readonly: true` and `X-MCP-Toolsets` instead of the query, so the server exposes only read tools from the selected toolsets.
Doppler remains the origin for API secrets that have no login of their own.

## Alternatives

A fine-grained personal access token is the only way to make the token itself read-only, but it needs manual recreation at each expiry and protects nothing the agent cannot already do through the GitHub CLI.
OpenCode's MCP OAuth would need an OAuth application registered with GitHub, grants private repository access only through the full `repo` scope, and stores its tokens in a file under `~/.local/share/opencode`.
OIDC federation needs an identity provider for the workload, which a personal workstation does not have.

## Consequences

`gh auth login` is the only setup the GitHub MCP server needs, and the GitHub CLI keeps the credential current.
A session started through the launcher does not share the background service's live state, such as browser pairing, and OpenCode subcommands run outside the launcher still lack the credential.
Read-only access is enforced by the MCP server's mode rather than by the token's scopes; an agent that wants to write can still use the GitHub CLI, under the same Git and review gates as before.
