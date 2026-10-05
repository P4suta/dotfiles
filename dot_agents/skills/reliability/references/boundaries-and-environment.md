# External boundaries and environment

## Verify the subject of a result

Bind service results to an operation identity, repository, revision, environment, and generation where these matter.
Read all required pages, and distinguish pending, skipped, stale, failed, rate-limited, and complete states.
A successful request, resolved thread, accepted task, or cached status proves nothing about finished work.
Keep returned job identities, and recover an uncertain submission through the provider's idempotency contract before submitting it again.

Classify errors before retrying.
Define attempt and elapsed-time bounds, cancellation, rate-limit behavior, and the evidence that ends the retry.
Backoff and jitter reduce load but never make a non-idempotent operation safe.
Use a stable idempotency key for one intended mutation, and check how the provider handles duplicates and changed parameters.
[Amazon's idempotent API guidance](https://aws.amazon.com/builders-library/making-retries-safe-with-idempotent-APIs/) explains why ambiguous outcomes need operation identity.
Never spend credits, raise limits, or trigger a duplicate paid review to get another result.

Verify eventual consistency through the promised state under a bounded observation deadline.
Distinguish an unavailable prerequisite from a negative business result.
Use a local deterministic adapter for success, malformed data, partial response, timeout, and ambiguous outcome, plus the necessary real contract check.
Keep live-service tests scoped and authorized.

## Make environmental inputs explicit

Pin the toolchain and dependencies, use lockfiles, and identify the source snapshot each check uses.
Fix the working directory, relevant environment, locale, timezone, encoding, and configuration precedence instead of inheriting an incidental shell session.
Preserve user-owned configuration and credentials while isolating test output and runtime state.
Never redefine common home variables to make one profile masquerade as another.

When the changed boundary depends on them, verify paths with spaces, native separators, case sensitivity, Unicode normalization, permissions, symlinks, line endings, and architecture.
Handle floating-point order, precision, rounding, and overflow explicitly when they influence an observable result.
Ground tolerances in the numeric contract instead of widening them until tests pass.

Prefer real readiness and visible behavior for UI and process integration tests.
[Playwright](https://playwright.dev/docs/actionability) waits for an element to accept input instead of sleeping for an arbitrary delay.
Keep a bound ephemeral port owned while handing it to the consumer, because releasing a found free port creates a race.
Probe the condition the user needs, such as a ready endpoint, not only process existence.

Treat resource limits, cache eviction, disk exhaustion, file-descriptor limits, and subprocess output backpressure as external failure inputs.
Control their relevant limits in tests and keep the primary cause.
When the contract includes platform behavior, run native Mac, Linux, and Windows checks through the owner's machine workflow.
