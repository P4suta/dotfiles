---
name: gcp
description: Operate Google Cloud through the official gcloud CLI or Google's published gcloud MCP server, and configure or diagnose these tools in Codex, Claude Code, and OpenCode.
---

# Google Cloud

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
Never print access tokens, copy credential databases, or create service-account keys for ordinary use.
Apply the user's existing authorization to project restoration, billing changes, and resource operations without asking again.

## Choose the interface

Discover the actual MCP tool schema before calling it.
The local server accepts `run_gcloud_command` with an `args` array without the leading `gcloud`, for example `{"args": ["projects", "describe", "PROJECT_ID", "--format=json"]}`.
Pass the same explicit resource flags as on the command line.
The server denies some interactive and SSH commands, so run those through the CLI or the remote-machine workflow without weakening the server's restrictions.

Read [references/clients.md](references/clients.md) for installation, client configuration, reconnection, or upgrades.

## Paid and temporary resources

Reuse the user's stated spending ceiling, runtime, and concurrency limits.
Check current regional prices and quotas before starting paid work, including disks, external IP, storage, and expected egress.
Quota availability proves nothing about zone capacity.
A billing-budget notification sets no hard spending cap.
`billingEnabled` proves nothing about credit coverage, so confirm credit status, expiry, and applicable usage in Billing Credits.

For temporary VMs, set deletion behavior and boot-disk autodelete at creation.
`--termination-time=RFC3339_TIMESTAMP` sets an absolute deadline across restarts.
`--max-run-duration=DURATION` counts from the last start and excludes the absolute deadline.
After a failed create, check for the VM and its disk before retrying.
Remove the exact temporary resources at completion and verify deletion.
Report estimates apart from settled billing.

## Primary references

- [gcloud CLI](https://docs.cloud.google.com/sdk/docs)
- [Google's local gcloud MCP server](https://github.com/googleapis/gcloud-mcp)
- [Remote MCP server in Cloud CLI](https://docs.cloud.google.com/sdk/use-gcloud-mcp), which has separate API, IAM, and OAuth 2.0 setup
- [VM runtime limits](https://docs.cloud.google.com/compute/docs/instances/limit-vm-runtime)
