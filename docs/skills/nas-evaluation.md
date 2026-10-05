# Network storage skill evaluation

The skill hands off artifacts through an existing authorized Network-Attached Storage (NAS) transport, and keeps private connection details out of shared instructions.
It separates an uploaded archive from an independently restored backup, and keeps the original evidence when remote verification fails.
It adds no implementation or credential-management mechanism.

| Task | Required behavior | Evidence |
| --- | --- | --- |
| Find an existing Synology Drive destination on the Mac | Resolve the existing shortcut to its native File Provider root, and inspect connection metadata without browsing unrelated files | Directory metadata gave the root, and read-only item evaluation reported upload availability with sync running and no conflicts |
| Inspect status without desktop UI access | Use a native status interface, and tell provider acknowledgement apart from remote checksum verification | Computer Use failed to open its native pipe, and File Provider evaluation gave explicit upload and exclusion states without changing client settings |
| Archive evidence from a live verification worker | Wait for final receipts, or archive only immutable inputs | The instruction requires writers to finish before the archive closes |
| Reclaim local space after upload | Keep the NAS copy, and verify an independent read before deleting the last source | The skill refuses ordinary deletion in a two-way sync root as a way to reclaim space |
| Install the workflow for three clients and three native hosts | Keep one canonical skill with relative discovery links, and use each host's native profile | Catalog, discovery-link, and resource checks cover the shared instructions, and rollout receipts follow execution |

Directory discovery and the read-only provider status ran on the existing Mac installation.
The remaining rows test instructions on representative tasks, and claim no remote restore or client activation.
Public evidence exposes neither the NAS address nor private client configuration.

## Executed verification

The Rust and typed-adapter checks passed for all 45 catalog entries on the Mac in 17.80 seconds, on Linux through `linux:56212d2f3366101226cb1480db80295f`, and on Windows through `win:28c562cb63c8a68caf2333df1a11b400`.
The six production maintenance proofs and the refusing counterexample also passed on the Mac.
The canonical skill entry has the same `sha256` digest on all three hosts, and each Claude Code discovery link resolves to its native canonical skill directory.
Installation used the scoped installer and kept local credentials and observation history.
Linux `linux:dd435d78df27192680e2e6d00702f3e1` and Windows `win:c160c03d6bb4668acc7bbcd134003615` completed catalog triage with persistent local evidence and checked the collector definitions of all three clients.
The first post-installation doctor checks exposed missing referenced evidence, and keeping every referenced source and adapter file closed that gap.
Collector registration establishes no live trust or activation in a client.

A 634,574,848-byte research archive went to the discovered sync destination, and its `sha256` digest matched the retained source archive.
Its 517 members listed, and the restored original ciphertext image matched its source hash.
Provider metadata for each file reported `isUploaded=1`, `isUploading=0`, `isSyncPaused=0`, `hasUnresolvedConflicts=0`, and `isExcludedFromSync=0`.
These checks establish local integrity and provider upload acknowledgement only, with no independent server read, so the source copies stay in place.
