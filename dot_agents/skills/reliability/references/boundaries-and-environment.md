# External Boundaries and Environment

## Verify the subject of a result

Bind service results to an operation identity, repository, revision, environment, and generation where these matter.
Read all required pages and distinguish pending, skipped, stale, failed, rate-limited, and complete states.
A successful request, resolved thread, accepted task, or cached status does not prove the requested work finished.
Retain returned job identities and recover an uncertain submission through the provider's idempotency contract before submitting it again.

Classify errors before retrying.
Specify attempt and elapsed-time bounds, cancellation, rate-limit behavior, and what evidence ends the retry.
Backoff and jitter can reduce load but do not make a non-idempotent operation safe.
Use a stable idempotency key for one intended mutation and validate how the provider handles duplicates and changed parameters.
[AWS's idempotent API guidance](https://aws.amazon.com/builders-library/making-retries-safe-with-idempotent-APIs/) explains why ambiguous outcomes need operation identity.
Never spend credits, increase limits, or trigger a duplicate paid review merely to obtain another result.

Verify eventual consistency through the promised state under a bounded observation deadline.
Distinguish an unavailable prerequisite from a negative business result.
Use a local deterministic adapter for success, malformed data, partial response, timeout, and ambiguous outcome, plus the necessary real contract check.
Keep live-service tests scoped and authorized.

## Make environmental inputs explicit

Pin the toolchain and dependencies, use lockfiles, and identify the source snapshot used by a check.
Specify the working directory, relevant environment, locale, timezone, encoding, and configuration precedence instead of inheriting an accidental shell session.
Preserve user-owned configuration and credentials while isolating test output and runtime state.
Do not redefine common home variables to make one profile masquerade as another.

Verify paths with spaces, native separators, case sensitivity, Unicode normalization, permissions, symlinks, line endings, and architecture when the changed boundary depends on them.
Treat floating-point order, precision, rounding, and overflow explicitly when they influence an observable result.
Use declared tolerances grounded in the numerical contract rather than widening them until tests pass.

Prefer real readiness and visible behavior for UI and process integration tests.
[Playwright auto-waiting](https://playwright.dev/docs/actionability) checks actionable state rather than relying on an arbitrary delay.
A bound ephemeral port should remain owned while it is handed to the consumer; finding a free port and releasing it creates a race.
Probe the condition that the user needs, such as a ready endpoint, rather than only whether a process exists.

Resource limits, cache eviction, disk exhaustion, file-descriptor limits, and subprocess output backpressure are external failure inputs.
Control their relevant limits in tests and retain the primary cause.
Run native Mac, Linux, and Windows checks through the owner's established machine workflow when platform behavior is part of the contract.
