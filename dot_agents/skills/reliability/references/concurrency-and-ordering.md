# Concurrency and Ordering

## Establish ownership and order

Give each mutable resource an owner and define the operation that commits a state transition.
Prefer immutable values, message passing, and narrow critical sections when they express the contract clearly.
Use a lock, transaction, compare-and-swap, or versioned protocol appropriate to the actual sharing boundary.
A thread mutex does not coordinate another process or machine.
Document the required happens-before relation and which events can overlap.

Make check-and-act decisions atomic with the state update they authorize.
Keep reservations or claims alive through the operation they protect.
Reject stale generations and duplicate completions instead of letting an older worker overwrite a newer result.
Choose a consistent lock order, and avoid holding an unrelated lock across a callback or unbounded I/O.
A semaphore bounds capacity; it does not by itself establish transaction consistency.
Use bounded queues and backpressure when producers can outrun consumers.

Parallelize checks only when their mutable files, ports, caches, databases, environment, and credentials are independent or correctly coordinated.
Separate a read-only input snapshot from writable output directories.
A shared tool cache may contain a lock even when the requested operation appears read-only.
Isolate per-invocation state when it is incidental; serialize only the resource whose required shared contract demands it.

## Make lifecycle explicit

Represent start, readiness, completion, failure, cancellation, and cleanup as distinct states.
Track spawned tasks and processes and join or otherwise account for them before claiming completion.
Define who owns cleanup when cancellation occurs between acquisition and publication.
Inspect cancellation safety before placing an operation inside a race or selection primitive.
A canceled local await does not prove a remote mutation was canceled.

Treat spurious wakeups, notifications sent before waiting, and partial reads as normal boundary cases.
Wait on a predicate or appropriate channel protocol instead of assuming one wakeup means success.
For exactly-once observable effects, use an explicit deduplication or transactional protocol rather than assuming the scheduler will avoid duplicates.

## Test the schedules that matter

Use barriers, channels, deterministic schedulers, or controlled adapters to reproduce the relevant interleavings.
Test competing acquisition, duplicate completion, out-of-order results, cancellation after acquisition, and slow consumers when admitted by the contract.
Avoid sleeps as an attempt to create a particular race.
Bounded stress can supplement these cases but does not prove their absence.

Use [Loom](https://docs.rs/loom/latest/loom/) for appropriate Rust synchronization models, with its primitives and stated execution bounds.
A model using a different lock or memory-ordering protocol needs an explicit connection to production behavior.
Use sanitizers, interpreters, or platform tracing when their supported semantics answer a specific concern.
Retain a real-process test for an OS lock or process boundary that an in-process mock does not establish.
