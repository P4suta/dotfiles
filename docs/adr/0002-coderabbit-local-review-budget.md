# Enforce a local CodeRabbit review budget

Status: superseded for attempt limits, service reserve, and executor scope by [reserved review capacity](0007-reserved-review-capacity.md).

## Context

The Advanced subscription includes command-line reviews under a rolling service allowance.
Concurrent agents, retries, and background reviews can spend that allowance.
The owner pays a fixed subscription and keeps local use below the service ceiling.
Skill prose neither serializes processes nor records attempts across an interrupted run.

## Decision

Pin the official command-line tool version and archive checksums in the dotfiles mise configuration, renamed to `coderabbit-vendor`.
Install the xtask Rust guard as both `coderabbit` and `cr`, so shell and `mise x` invocations share one boundary.
Limit attempts to two per rolling hour and eight per rolling 24 hours across every repository on one machine.
Reserve an attempt durably before service preflight, and hold the ledger's operating-system lock until the review ends.
Keep failed and interrupted attempts, keep history across installation, and refuse a missing ledger after initialization.
Publish the first-install marker only after the guard has synchronized, locked, and checked the ledger.
Retry an incomplete installation without discarding existing history.
On Unix, synchronize the state directory and its home-directory ancestry before publishing the marker, then synchronize the state directory again to persist the marker entry.
Refuse unknown quota or billing state, a mismatched personal account, paid-credit requests, cloud coding, remote scopes, and the self-update command.
Check the quota in the selected local repository, and keep one included review as headroom.
Keep paid service add-ons inactive outside the wrapper.

The [guard implementation](../../xtask/src/review_guard.rs) owns these rules.
The [contract tests](../../xtask/tests/review_guard.rs) cover rolling limits, durable reservations, locking, refusal paths, and installation state.
The [runbook](../coderabbit/setup.md) covers installation and daily use.

## Consequences

No concurrency, failed preflight, or reinstall pushes local reviews past the compiled attempt budget.
A conservative refusal can spend a local attempt without starting a service review.
A usage report shows a service snapshot and reserves no capacity from other clients.
GitHub automatic reviews, IDE extensions, browser tasks, other machines, and deliberate tampering with the raw executable or state fall outside this boundary.
The guard protects cooperative tools, not an account-wide quota, and offers no sandbox from its owner.
Coordinating every client needs a service-enforced organization cap or a shared gateway, outside this setup.
Changing the compiled limits or repairing initialized state takes an explicit owner decision.
