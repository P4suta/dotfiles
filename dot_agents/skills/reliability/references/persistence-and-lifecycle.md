# Persistence and lifecycle

## Define the commit point

Define what successful return promises: visibility, atomic replacement, durability, or a verified external effect.
Choose the storage protocol from that promise.
An atomic rename on one filesystem may still lack durability, portability across filesystems, or matching Windows behavior.
When the contract needs replacement, write temporary content in the destination filesystem, check it, and publish through the supported atomic operation.
Check collision handling, permissions, symlinks, and whether a previous valid value must survive failure.

Flush required file contents and metadata before publishing a durable completion marker.
Where the system requires it, also synchronize the directory entries that reference the files.
[Linux fsync](https://man7.org/linux/man-pages/man2/fsync.2.html) documents the separate directory-entry rule.
Verify native durability semantics instead of assuming Unix directory operations carry over to [Windows file flushing](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-flushfilebuffers).
Include created parent directories in the durability analysis when losing their entries would invalidate the promised result.
A successful read after writing proves no protection from power loss.

Check persisted data before it controls a quota, authorization, or state transition.
Reject missing, truncated, malformed, reordered, incompatible, or unverifiable history when reconstruction could grant an invalid state.
Distinguish a never-initialized installation from a damaged initialized one.
Keep recovery idempotent, and preserve valid history across reinstall, restart, or interrupted setup.

## Handle partial progress

Analyze failure after every meaningful external effect, including a process crash between writes.
Mark completion only after the artifacts or evidence it depends on commit.
For databases, use the required transaction isolation and constraints, because an app-level precheck alone can race.
When no single transaction spans the resources, define resumable steps and compensating actions with durable identities.
Record an uncertain outcome instead of retrying an unverified destructive operation.

Hold resources through owned handles and narrow lifetimes.
Use secure unique temporary files, explicit cleanup ownership, and output-directory isolation.
Cleanup must preserve the primary error and leave other operations' resources intact.
Process-ID reuse, stale lock files, and reused temporary names make lifecycle cases, never ownership proofs.
Keep runtime state, authentication, and user-owned history out of generated configuration and cross-host copying.

Where supported, test interrupted initialization, reinstall with history, failed publication, cleanup failure, and concurrent access.
Verify the required ordering and error propagation with deterministic fault injection or native syscall tracing.
A simulated filesystem model needs declared assumptions and a check of the real adapter.
