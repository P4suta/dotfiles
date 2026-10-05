---
name: nas
description: >-
  Discover existing Network Attached Storage (NAS) access and archive, retrieve, or verify large and private project artifacts using an authorized share or sync client.
  Use for NAS transfers, archive manifests, restore checks, and storage handoffs, not general cloud provisioning or NAS administration.
---

# Network storage artifacts

Resolve the storage goal and use the owner's existing authorized NAS connection.
Keep large datasets, downloaded source documents, generated models, and private operational receipts out of public Git when they lack a place in the repository.
Keep reproducible commands, evaluated results, small required evidence, and an archive manifest in the project as its checks and disclosure scope require.
Moving evidence to private storage must leave every public claim supported and every required gate passing.

## Discover access

Inspect the relevant mounts, sync roots, or configured host aliases before requesting connection details.
Check the resolved target and active connection, because an existing directory proves neither a mounted share nor a connected sync task.
Distinguish a mounted share, a client-managed local folder, an on-demand placeholder, and an independent local copy.
Inspect only the metadata the task needs, and skip unrelated personal folders and credential databases.
Reuse native authentication and the existing credential manager, and keep server names, paths, and accounts in private local inventory.
Read [Synology Drive](references/synology-drive.md) only when that client carries the transfer.
Use `tailscale` for reachability problems and `multi-machine` before operating another development host.

## Prepare and transfer

Archive only immutable inputs, or wait for writers to finish.
Record the source revision, input versions, relative member paths, byte counts, `sha256` hashes, and creation time.
Separate sensitive receipts from material approved for public sharing, and keep downloaded originals unless the user authorized their removal.
Inspect symlinks and special files instead of following them into an unrelated tree.
Use a new task-specific destination, verify free capacity, and preserve existing names, versions, and permissions.
Prepare an archive outside the sync root or publish it through the transport's atomic mechanism, so a partial archive never looks complete.
Use existing archive, checksum, and transfer tools where they suffice.
Write reusable automation as maintained Rust tooling through `rust-tooling` and `xtask`, with `formal-assurance` for an implementation change, and never add a shell or Python path.

## Verify and keep

Verify the destination bytes with the manifest, and test that the archive opens and its members restore.
Record local-copy verification, client upload acknowledgment, and an independent NAS read or restore as separate evidence states.
A local hash and a green status icon prove nothing about an independent remote restore.
Establish upload acknowledgment from per-artifact upload state or a server-side receipt, and record failures and pending status.
Before deleting the last independent source copy, read the archive through an independent NAS path, verify its hashes, and restore representative members.
Keep the source when remote verification fails or waits, and never treat elapsed time as confirmation.

Synchronization can spread deletion, corruption, and conflicts, so it never counts as an independent backup.
Free space for a verified uploaded file through the sync client's documented operation, because deleting a file inside a two-way sync root can delete its NAS copy.
Keep retention, snapshots, backup configuration, share permissions, credential changes, and storage destruction out of a transfer request unless the user authorized them.
Use `storage-scout` for local build and cache cleanup.
Report the archive reference, hashes, verification state, and any kept copies or pending restore check.
