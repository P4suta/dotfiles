# Enforce a local CodeRabbit review budget

Status: superseded for attempt limits, service reserve, and executor scope by [reserved review capacity](0007-reserved-review-capacity.md).

## Context

The Advanced subscription includes CLI reviews under a rolling service allowance.
Concurrent agents, retries, and background reviews can spend it, and skill prose can't serialize processes or record attempts across an interrupted run.

## Decision

Pin the official CLI version and archive checksums in the dotfiles mise configuration, renamed to `coderabbit-vendor`.
Install the xtask Rust guard as both `coderabbit` and `cr`, so shell and `mise x` invocations share one boundary.
Limit attempts to two per rolling hour and eight per rolling 24 hours across every repository on one machine.
Reserve an attempt durably before service preflight, and hold the ledger's operating-system lock until the review ends.
Keep failed and interrupted attempts, keep history across installation, and refuse a missing ledger after initialization.
Publish the first-install marker only after the guard has synchronized, locked, and checked the ledger.
Retry an incomplete installation without discarding existing history.
On Unix, synchronize the state directory and its home-directory ancestry before publishing the marker, then synchronize the state directory again.
Refuse unknown quota or billing state, a mismatched personal account, paid-credit requests, cloud coding, remote scopes, and the self-update command.
Check the quota in the selected local repository, and keep one included review as headroom.
Keep paid service add-ons inactive outside the wrapper.

The [guard implementation](../../xtask/src/review_guard.rs) owns these rules, the [contract tests](../../xtask/tests/review_guard.rs) cover them, and the [runbook](../coderabbit/setup.md) covers use.

## Consequences

A conservative refusal can spend a local attempt without starting a service review.
GitHub automatic reviews, IDE extensions, browser tasks, other machines, and tampering with the raw executable or state fall outside this boundary.
Changing the compiled limits or repairing initialized state takes an explicit owner decision.
