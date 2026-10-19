# Concurrency and ordering

## Establish ownership and order

Give each mutable resource an owner, and define the operation that commits a state transition.
Prefer immutable values, message passing, and narrow critical sections when they express the contract.
Use a lock, transaction, compare-and-swap, or versioned protocol that fits the actual sharing boundary.
A thread mutex coordinates no other process or machine.

Make each check-and-act decision atomic with the state update it permits.
Keep reservations or claims alive through the operation they protect.
Reject stale generations and duplicate completions instead of letting an older worker overwrite a newer result.
Choose a consistent lock order, and avoid holding an unrelated lock across a callback or unbounded I/O.
Use bounded queues and backpressure when producers can outrun consumers.

Parallelize checks only when their mutable files, ports, caches, databases, environment, and credentials stay independent or coordinated.
Separate a read-only input snapshot from writable output directories.
A shared tool cache may hold a lock even for an operation that looks read-only.

## Make lifecycle explicit

Represent start, readiness, completion, failure, cancellation, and cleanup as distinct states.
Track spawned tasks and processes, and join or account for them before claiming completion.
Define who owns cleanup when cancellation lands between acquisition and publication.
Inspect cancellation safety before placing an operation inside a race or selection primitive.
A canceled local await proves nothing about a remote mutation.

Treat spurious wakeups, notifications sent before waiting, and partial reads as normal boundary cases.
Wait on a predicate or channel protocol instead of treating one wakeup as success.
For effects that must happen once, use an explicit deduplication or transactional protocol.

## Test the schedules that matter

Reproduce the relevant interleavings with barriers, channels, deterministic schedulers, or controlled adapters.
Test competing acquisition, duplicate completion, out-of-order results, cancellation after acquisition, and slow consumers when the contract admits them.
Never use sleeps to provoke a particular race.
Bounded stress can supplement these cases but never proves their absence.

Use [Loom](https://docs.rs/loom/latest/loom/) for Rust synchronization models, with its stated execution bounds.
A model using a different lock or memory-ordering protocol needs an explicit connection to production behavior.
Keep a real-process test for an OS lock or process boundary that an in-process mock leaves unproven.
