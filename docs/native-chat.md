# Native agent chat

Every profile installs a compatible domyjob client, then runs the `chat` setup step on each apply after tool installation and client integration.
The step delegates peer pinning, the background service, and MCP registration for installed Claude Code, Codex, and OpenCode clients to `domyjob chat setup`.
It then requires `domyjob chat doctor` to succeed.
The step preserves the command's findings when setup or doctor fails.
It never replaces a changed peer identity automatically.

Configure the other machines' existing SSH aliases in the machine-local chezmoi configuration, outside the public source:

```toml
[data.domyjob.chat]
peers = ["peer-one", "peer-two"]
```

The public default stays empty and contains no hostnames, addresses, credentials, or private checkout paths.
Missing peers leave setup explicitly incomplete and name the configuration to add before applying again.
If chat can't run on a host yet, use the existing `setup.ignore` entry for `chat.sh` or `chat.ps1` with the concrete reason.
The omission remains visible in the profile's declared skips.
Use a host's configured aliases and pinned SSH keys.
The setup step doesn't discover or trust peers automatically.
The doctor's authentication findings may require the owner's normal client login.

The setup step reuses an installed domyjob when `chat setup --help`, `chat doctor --help`, and `mcp --help` all succeed.
This checks the needed command interface and trusts the native installation.
It doesn't verify its source revision or make a new cryptographic claim.
Without that interface, the existing owner-source installer uses its machine-local project root and locked Cargo dependencies.
The reusable native path needs neither a source checkout nor GitHub credentials and doesn't invoke Cargo or Git.

The delegation skill uses a room for each authorized multi-agent task and carries the room and agent identity into every delegate's task.
Status, blockers, questions, and completion travel through the room.
Ordinary room messages carry resource claims with explicit ownership and expiry, for example:

```json
{"kind":"claim","id":"build-1","resource":"cargo-target","owner":"worker-one@local","expires_at":"2099-01-01T00:00:00Z"}
{"kind":"release","id":"build-1","owner":"worker-one@local"}
```

The coordinator acknowledges claims before work starts, serializes competing requests, and matches message authorship with the stated owner.
A renewal names the same claim and a later expiry.
A takeover names the expired claim, obtains a fresh acknowledgement, and verifies that its previous process has stopped before using the resource.
Room messages can't create an operating-system lock or stop a process when its claim expires.

The isolated CLI fixture registers a coordinator and two interactive agent identities in a temporary `DOMYJOB_STATE`, then exchanges claims, releases, and status through a room.
It invokes the installed command directly and starts no AI client, service, or remote session:

```console
mise x -- cargo test --locked -p dotfiles --test chat -- --ignored
```

This fixture verifies the local chat interface and room exchange.
Native apply, peer synchronization, service health, and installed client discovery require each host's later rollout verification.
