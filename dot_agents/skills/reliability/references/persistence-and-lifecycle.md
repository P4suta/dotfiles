# Persistence and Lifecycle

## Define the commit point

Specify what successful return promises: visibility, atomic replacement, durability, or a verified external effect.
Choose the storage protocol from that promise.
A rename that is atomic on one filesystem need not be durable, portable across filesystems, or equivalent on Windows.
Write temporary content in the destination filesystem, validate it, and publish through the supported atomic operation when replacement is required.
Check collision handling, permissions, symlinks, and whether a previous valid value must survive failure.

Flush required file contents and metadata before publishing a durable completion marker.
On systems requiring it, synchronize directory entries as well as the files they reference.
[Linux fsync](https://man7.org/linux/man-pages/man2/fsync.2.html) describes the separate directory-entry requirement.
Verify native guarantees rather than assuming Unix directory operations translate directly to [Windows file flushing](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-flushfilebuffers).
Include created parent directories in the durability analysis when losing their entries would invalidate the promised result.
Do not claim protection from arbitrary power loss based only on a successful read after writing.

Validate persisted data before it controls a quota, authorization, or state transition.
Reject missing, truncated, malformed, reordered, incompatible, or unverifiable history when reconstruction could grant an invalid state.
Distinguish a never-initialized installation from a damaged initialized one.
Keep recovery idempotent and preserve valid history across reinstall, restart, or interrupted setup.

## Handle partial progress

Analyze failure after every meaningful external effect, including a process crash between writes.
Do not mark completion before the artifacts or evidence it depends on are committed.
For databases, use the required transaction isolation and constraints; an application-level precheck alone can race.
For several resources without one transaction, define resumable steps and compensating actions with durable identities.
Record an uncertain outcome rather than retrying an unverified destructive operation.

Acquire resources through owned handles and narrow lifetimes.
Use secure unique temporary files, explicit cleanup ownership, and output-directory isolation.
Cleanup must preserve the primary error and must not remove another operation's resources.
Process-ID reuse, stale lock files, and reused temporary names are lifecycle cases rather than reliable ownership proofs.
Keep runtime state, authentication, and user-owned history out of generated configuration and cross-host copying.

Test interrupted initialization, reinstall with history, failed publication, cleanup failure, and concurrent access when supported.
Use deterministic fault injection or native syscall tracing to verify the required ordering and error propagation.
A simulated filesystem model needs declared assumptions and a check of the real adapter.
