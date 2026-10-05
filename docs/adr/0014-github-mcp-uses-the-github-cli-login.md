# Reuse the `gh` login for the remote GitHub tool server

Status: accepted.

## Context

The Mac OpenCode configuration reaches GitHub's remote MCP server with a bearer token in `GH_MCP_TOKEN`.
Architecture decision record [0009](0009-public-native-profiles.md) routes API secrets through Doppler.
The plan made the token a dedicated fine-grained personal access token that the owner creates, stores in Doppler, and recreates when it expires.
The agent already runs `gh` with the owner's login, whose scopes include repository write access.
A narrower token adds a second credential and restricts only the MCP channel.
GitHub's remote server documents personal access tokens and host-registered OAuth 2.0 applications, not dynamic client registration.
It takes read-only mode and toolset selection as request headers.
An undocumented `?toolsets=` query received HTTP 400 for `tools/list`.

## Decision

The OpenCode launcher reads the token of `gh` with `gh auth token` and passes it as `GH_MCP_TOKEN` in the agent's environment.
When `gh` has no login, the launcher warns and starts the agent without the token.
The keychain login of `gh` remains the only origin of that credential, and the launcher refuses a configuration that also selects `GH_MCP_TOKEN` from Doppler.
OpenCode runs MCP servers in a shared background service that keeps the environment it started with.
The launcher thus starts the interface and `run` with `--standalone`, a private server that inherits the credential.
Other subcommands and a launch that chooses its own server pass through unchanged.
The configuration sends `X-MCP-Readonly: true` and `X-MCP-Toolsets` instead of the query, so the server exposes only read tools from the selected toolsets.
Doppler remains the origin for API secrets that have no login of their own.

## Alternatives

Only a fine-grained personal access token can make the token itself read-only.
Such a token needs recreation at each expiry and protects nothing beyond what the agent can do through `gh`.
OpenCode's MCP OAuth 2.0 support would need an OAuth 2.0 app registered with GitHub.
It grants private repository access only through the full `repo` scope and stores its tokens in a file under `~/.local/share/opencode`.
OpenID Connect federation needs an identity provider for the workload, and a personal workstation has none.

## Consequences

The GitHub MCP server needs no setup beyond `gh auth login`, and `gh` keeps the credential current.
A session started through the launcher lacks the background service's live state, such as browser pairing.
The MCP server's read-only mode, not the token's scopes, enforces read-only access.
An agent that wants to write can still use `gh` under the same Git and review gates as before.
