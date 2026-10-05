# Concurrency and ordering

## Establish ownership and order

Give each mutable resource an owner, and define the operation that commits a state transition.
Prefer immutable values, message passing, and narrow critical sections when they express the contract.
Use a lock, transaction, compare-and-swap, or versioned protocol that fits the actual sharing boundary.
A thread mutex coordinates no other process or machine.
Document the required happens-before relation and the events that can overlap.

Make each check-and-act decision atomic with the state update it permits.
Keep reservations or claims alive through the operation they protect.
Reject stale generations and duplicate completions instead of letting an older worker overwrite a newer result.
Choose a consistent lock order, and avoid holding an unrelated lock across a callback or unbounded I/O.
A semaphore bounds capacity without establishing transaction consistency.
Use bounded queues and backpressure when producers can outrun consumers.

Parallelize checks only when their mutable files, ports, caches, databases, environment, and credentials stay independent or coordinated.
Separate a read-only input snapshot from writable output directories.
A shared tool cache may hold a lock even for an operation that looks read-only.
Isolate incidental per-invocation state, and serialize only the resource whose shared contract demands it.

## Make lifecycle explicit

Represent start, readiness, completion, failure, cancellation, and cleanup as distinct states.
Track spawned tasks and processes, and join or account for them before claiming completion.
Define who owns cleanup when cancellation lands between acquisition and publication.
Inspect cancellation safety before placing an operation inside a race or selection primitive.
A canceled local await proves nothing about a remote mutation.

Treat spurious wakeups, notifications sent before waiting, and partial reads as normal boundary cases.
Wait on a predicate or channel protocol instead of treating one wakeup as success.
For observable effects that must happen once, use an explicit deduplication or transactional protocol instead of trusting the scheduler.

## Test the schedules that matter

Reproduce the relevant interleavings with barriers, channels, deterministic schedulers, or controlled adapters.
When the contract admits them, test competing acquisition, duplicate completion, out-of-order results, cancellation after acquisition, and slow consumers.
Never use sleeps to provoke a particular race.
Bounded stress can supplement these cases but never proves their absence.

Use [Loom](https://docs.rs/loom/latest/loom/) primitives and stated execution bounds for fitting Rust synchronization models.
A model using a different lock or memory-ordering protocol needs an explicit connection to production behavior.
Use sanitizers, interpreters, or platform tracing when their supported semantics answer a specific concern.
Keep a real-process test for an operating system lock or process boundary that an in-process mock leaves unproven.
