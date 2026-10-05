---
name: gcp
description: Operate Google cloud services through the official gcloud command-line tool or Google's published gcloud Model Context Protocol (MCP) server, and configure or diagnose these tools in Codex, Claude Code, and OpenCode.
---

# Google cloud operations

Use `gcloud` for reproducible commands and the `gcloud` MCP connection when a tool interface helps.
The local server, Google's `@google-cloud/gcloud-mcp` preview, runs as the active gcloud account with that account's permissions.

## Establish context

Resolve the project and resource location from the request or repository configuration.
Pass `--project=PROJECT_ID` and applicable `--region` or `--zone` flags explicitly.
Leave global gcloud defaults unchanged unless the request covers them.

Start with the relevant read-only checks:

```sh
gcloud version
gcloud auth list --filter=status:ACTIVE --format='value(account)'
gcloud projects describe PROJECT_ID --format=json
gcloud billing projects describe PROJECT_ID --format=json
```

Reuse existing authentication.
If user login has expired or never happened, run interactive `gcloud auth login`.
The command-line tool and local MCP server need no `gcloud auth application-default login`.
Never print access tokens, copy credential databases, or create service-account keys for ordinary use.
Apply the user's existing authorization to project restoration, billing changes, and resource operations without asking again.

## Choose the interface

Discover the actual MCP tool schema before calling it.
The local server accepts `run_gcloud_command` with an `args` array without the leading `gcloud`, for example `{"args": ["projects", "describe", "PROJECT_ID", "--format=json"]}`.
Pass the same explicit resource flags as on the command line.
The server denies some interactive and SSH commands, so run those through the command line or the remote-machine workflow without weakening the server's restrictions.
Keep argument vectors and resource names when operations must replay.
Prefer structured arguments and payload files over generated shell expressions.

Read [references/clients.md](references/clients.md) for installation, client configuration, reconnection, or upgrades.

## Paid and temporary resources

Reuse the user's stated spending ceiling, runtime, and concurrency limits.
Check current regional prices and quotas before starting paid work, including disks, external IP, storage, and expected egress.
Select the machine and accelerator for the workload and available capacity.
Quota availability proves nothing about zone capacity.
A billing-budget notification sets no hard spending cap.
When promotional credits matter, confirm status, expiry, and applicable usage in Billing Credits, because `billingEnabled` proves nothing about credit coverage.
Some credit details need the console.

For temporary VMs, set deletion behavior and boot-disk autodelete at creation.
`--termination-time=RFC3339_TIMESTAMP` sets an absolute deadline across restarts.
`--max-run-duration=DURATION` counts from the last start and excludes the absolute deadline.
After a failed create, check for the VM and its disk before retrying.
Remove the exact temporary resources at completion and verify deletion.
Report estimates apart from settled billing.

## Primary references

- [gcloud command-line tool](https://docs.cloud.google.com/sdk/docs)
- [Google's local gcloud MCP server](https://github.com/googleapis/gcloud-mcp)
- [Remote gcloud MCP server](https://docs.cloud.google.com/sdk/use-gcloud-mcp), with its own API, access, and OAuth 2.0 setup
- [VM runtime limits](https://docs.cloud.google.com/compute/docs/instances/limit-vm-runtime)
