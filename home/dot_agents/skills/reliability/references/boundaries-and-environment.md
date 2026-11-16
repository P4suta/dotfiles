# External boundaries and environment

## Verify the subject of a result

Bind service results to an operation identity, repository, revision, environment, and generation where these matter.
Read all required pages, and distinguish pending, skipped, stale, failed, rate-limited, and complete states.
A successful request, resolved thread, accepted task, or cached status proves nothing about finished work.
Recover an uncertain submission through the provider's idempotency contract before submitting it again.

Classify errors before retrying.
Define attempt and elapsed-time bounds, cancellation, rate-limit behavior, and the evidence that ends the retry.
Backoff and jitter reduce load but never make a non-idempotent operation safe.
Use a stable idempotency key for one intended mutation, and check how the provider handles duplicates and changed parameters.
[AWS's idempotent API guidance](https://aws.amazon.com/builders-library/making-retries-safe-with-idempotent-APIs/) explains why ambiguous outcomes need operation identity.
Never spend credits, raise limits, or trigger a duplicate paid review to get another result.

Verify eventual consistency through the promised state under a bounded observation deadline.
Distinguish an unavailable prerequisite from a negative business result.
Use a local deterministic adapter for success, malformed data, partial response, timeout, and ambiguous outcome in the project's reproducible gate.
Keep necessary live contract observations separate and report unavailable prerequisites as missing evidence.

## Make environmental inputs explicit

Pin the toolchain, dependencies, and external verification inputs by version or digest, and identify the source snapshot each check uses.
Fix the working directory, relevant environment, locale, timezone, encoding, and configuration precedence instead of inheriting an incidental shell session.
Preserve user-owned configuration and credentials while isolating test output and runtime state.
Never redefine common home variables to make one profile masquerade as another.

When the changed boundary depends on them, verify paths with spaces, native separators, case sensitivity, Unicode normalization, permissions, symlinks, line endings, and architecture.
Handle floating-point order, precision, rounding, and overflow explicitly when they influence an observable result.
Ground tolerances in the numeric contract instead of widening them until tests pass.

Prefer real readiness and visible behavior for UI and process integration tests.
[Playwright](https://playwright.dev/docs/actionability) waits for an element to accept input instead of sleeping.
Keep a bound ephemeral port owned while handing it to the consumer, because releasing a found free port creates a race.
Probe the condition the user needs, such as a ready endpoint, not only process existence.

Treat resource limits, cache eviction, disk exhaustion, file-descriptor limits, and subprocess output backpressure as external failure inputs.
Control their limits in tests and keep the primary cause.
When the contract includes platform behavior, use the project's documented hosted platform runners or pinned container.

## Separate host filtering from remote behavior

When a discovery request gets no reply, check the host's own packet filtering before concluding that the remote device ignored it.
A per-app firewall can drop one binary's inbound replies while another binary on the same host receives them.
Send the same unicast request from a platform-signed tool and from the binary under test.
When the device ignores a broadcast ping, connect to its documented TCP port instead.
Record each binary's firewall decision beside the evidence, and recheck it after a rebuild.
On macOS, read [firewall observations](macos-application-firewall.md) for the commands and a dated comparison.
