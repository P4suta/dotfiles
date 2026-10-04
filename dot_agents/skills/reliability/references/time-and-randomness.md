# Time and Randomness

## Choose the meaning of time

Use elapsed monotonic time for in-process durations and deadlines, and wall-clock instants for calendar or externally specified expiry.
Specify units, precision, inclusive or exclusive boundaries, and behavior when conversion or arithmetic overflows.
Check whether the platform clock advances during system suspend when that affects a deadline.
Persisted expiry needs an explicit restart and clock-adjustment contract; do not serialize a process-local monotonic instant as a portable timestamp.
Distributed ordering needs protocol evidence such as a revision or sequence number rather than comparing unsynchronized machine clocks.

Inject the clock at the I/O boundary and keep expiry decisions pure.
Test just before, exactly at, and just after each boundary, plus rollback, forward jumps, restart, and stale observations where applicable.
For safety or quota decisions, define how unavailable or regressed time is refused instead of granting fresh capacity.
Do not use wall-clock filenames as exclusive resource identities; reserve unique files atomically and exercise repeated clock readings in the regression.
Keep the reason for real-time dependence explicit when the domain genuinely requires it.

Use the [Rust Instant contract](https://doc.rust-lang.org/std/time/struct.Instant.html) and the runtime's actual clock behavior.
[Python's monotonic clock](https://docs.python.org/3/library/time.html#time.monotonic) has an unspecified origin, so elapsed differences carry meaning rather than a calendar date.

## Wait for evidence

Synchronize on a completion, readiness signal, condition, or observed state.
A sleep only delays execution; it does not prove another operation finished.
Wait under a bounded deadline and retain diagnostics that identify the unmet condition.
Keep polling read-only, cancelable, and bounded when a remote system provides no event mechanism.
Do not turn a local test into a live service polling loop.

Use virtual time for timer behavior when the runtime supports it.
Check which clocks are virtualized and which executor modes support the feature.
[Tokio paused time](https://docs.rs/tokio/latest/tokio/time/fn.pause.html) controls Tokio's clock and requires its supported runtime; it does not freeze every system clock.

## Control randomness and unstable selection

Pass an RNG or seed into algorithms that need reproducibility, and record the seed and generator version with a failure.
Combine stable regression seeds with generated exploration rather than proving only one fixed sequence.
Keep cryptographic production randomness separate from deterministic test fixtures.
Use secure randomness for secrets; reproduce the protocol around it without making production secrets predictable.
Specify a stable tie-breaker for equally ranked items.
Do not rely on hash-map order, directory enumeration, randomized test order, or a race winner unless the API explicitly permits every resulting outcome.
Normalize incidental ordering only when order is outside the observable contract.
