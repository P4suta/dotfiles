---
name: nas
description: >-
  Discover existing NAS access and archive, retrieve, or verify large and private project artifacts using an authorized share or sync client.
  Use for NAS transfers, archive manifests, restore checks, and storage handoffs, not general cloud provisioning or NAS administration.
---

# NAS Artifacts

Resolve the storage goal and use the owner's existing authorized NAS connection.
Keep large datasets, downloaded source documents, generated models and private operational receipts outside public Git when they do not belong in the repository.
Keep reproducible commands, evaluated results, small required evidence and an archive manifest in the project as appropriate to its checks and disclosure scope.
Moving evidence to private storage must not leave a public claim unsupported or break a required gate.

## Discover access

Inspect the relevant mounts, sync roots or configured host aliases before requesting connection details.
Check the resolved target and active connection; a directory's existence alone does not prove that a share is mounted or a sync task is connected.
Distinguish a mounted share, a client-managed local folder, an on-demand placeholder and an independent local copy.
Inspect only metadata needed for the task, not unrelated personal folders or credential databases.
Reuse native authentication and the existing credential manager; keep server names, paths and accounts in private local inventory rather than this shared skill.
Read [Synology Drive](references/synology-drive.md) only when that client is the actual transport.
Use `tailscale` when reachability is the problem and `multi-machine` before operating another development host.

## Prepare and transfer

Identify immutable inputs or wait for writers to finish before creating an archive.
Record the source revision, input versions, relative member paths, byte counts, SHA-256 hashes and creation time.
Separate sensitive receipts from material approved for public sharing, and preserve downloaded originals unless their removal is authorized.
Inspect symlinks and special files rather than following them into an unrelated tree.
Use a new task-specific destination, verify available capacity and preserve existing names, versions and permissions.
Do not expose a partially written archive as the completed version; prepare it outside the sync root or use the transport's supported atomic publication mechanism.
Use existing archive, checksum and transfer tools where sufficient.
Implement reusable procedural automation as maintained Rust tooling through `rust-tooling` and `xtask`, with `formal-assurance` for an implementation change.
Do not add a second shell or Python automation path.

## Verify and retain

Verify the destination bytes against the manifest and test that the archive can be opened and its members restored.
Record local-copy verification, client upload acknowledgement and an independent NAS read or restore as separate evidence states.
A local hash and a green status icon do not establish an independent remote restore.
Use per-artifact upload state or a server-side receipt to establish transfer acknowledgement, and preserve failures or pending status explicitly.
Before deleting the last independent source copy, read the archive through an independent NAS access path, verify its hashes and perform a representative restore.
Retain the source when remote verification is unavailable; do not treat a wait interval as confirmation.

Synchronization can propagate deletion, corruption and conflicts and does not itself establish an independent backup.
Use the sync client's documented space-reclamation operation for a verified uploaded file; deleting a file inside a two-way sync root can delete its NAS copy.
Keep retention, snapshots, backup configuration, share permissions, credential changes and storage destruction outside a transfer request unless the user has authorized them.
Use `storage-scout` for local build and cache cleanup rather than turning an archive handoff into a filesystem sweep.
Report the archive reference, hashes, actual verification state and any retained copies or pending restore check.
