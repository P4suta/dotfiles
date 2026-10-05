# Time and randomness

## Choose the meaning of time

Use elapsed monotonic time for in-process durations and deadlines, and wall-clock instants for calendar or externally specified expiry.
Define units, precision, inclusive or exclusive boundaries, and behavior when arithmetic overflows.
Check whether the platform clock advances during system suspend when that affects a deadline.
Persisted expiry needs an explicit contract for restarts and clock changes.
Never serialize a process-local monotonic instant as a portable timestamp.
Distributed ordering needs a revision or sequence number, not comparisons of unsynchronized machine clocks.

Inject the clock at the I/O boundary and keep expiry decisions pure.
Test just before, exactly at, and just after each boundary, plus rollback, forward jumps, restart, and stale observations.
For safety or quota decisions, refuse unavailable or regressed time instead of granting fresh capacity.
Never use wall-clock filenames as exclusive resource identities.
Reserve unique files atomically and exercise repeated clock readings in the regression.

Follow the [Rust Instant contract](https://doc.rust-lang.org/std/time/struct.Instant.html) and the runtime's actual clock behavior.
[Python's monotonic clock](https://docs.python.org/3/library/time.html#time.monotonic) has an unspecified origin, so only elapsed differences carry meaning.

## Wait for evidence

Synchronize on a completion, readiness signal, condition, or observed state.
A sleep proves nothing about another operation.
Wait under a bounded deadline, and keep diagnostics that identify the unmet condition.
When a remote system offers no event mechanism, keep polling read-only, cancelable, and bounded.
Never turn a local test into a live service polling loop.

Use virtual time for timer behavior when the runtime supports it.
[Tokio paused time](https://docs.rs/tokio/latest/tokio/time/fn.pause.html) controls only Tokio's clock and requires its supported runtime.

## Control randomness and unstable selection

Pass an RNG or seed into algorithms that need reproducibility, and record the seed and generator version with a failure.
Combine stable regression seeds with generated exploration.
Keep cryptographic production randomness apart from deterministic test fixtures.
Define a stable tie-breaker for equally ranked items.
Rely on hash-map order, directory enumeration, randomized test order, or a race winner only when the API permits every resulting outcome.
