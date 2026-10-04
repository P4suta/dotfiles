# Enforce a Local CodeRabbit Review Budget

Status: Superseded for attempt limits, service reserve, and executor scope by [reserved review capacity](0007-reserved-review-capacity.md).

## Context

The Advanced subscription provides included CLI reviews with a rolling service allowance.
Coding agents can otherwise spend that allowance through concurrent work, retries, and repeated background reviews.
The owner requires fixed subscription spending and local use below the service ceiling.
Skill prose alone cannot serialize processes or persist attempts after an interrupted run.

## Decision

Keep the official CLI version and archive checksums in the dotfiles mise configuration, renamed to `coderabbit-vendor`.
Install the existing xtask crate's Rust guard as both `coderabbit` and `cr` so ordinary shell and `mise x` invocations share one boundary.
Limit attempts to two per rolling hour and eight per rolling 24 hours across every repository on one machine.
Reserve an attempt durably before service preflight and hold the ledger's operating-system lock through completion.
Keep failed and interrupted attempts, preserve history across installation, and refuse a missing ledger after initialization.
Publish the first-install marker only after the ledger is synchronized, locked, and validated; retry an incomplete installation while preserving any existing history.
On Unix, synchronize the state directory and its home-directory ancestry before publishing the marker, then synchronize the state directory again to preserve the marker entry.
Refuse unknown quota or billing state, a mismatched personal account, paid-credit requests, cloud coding, remote scopes, and the self-update command.
Verify the quota in the selected local repository and retain one included review as headroom.
Keep service-side paid add-ons inactive independently of the wrapper.

The [guard implementation](../../xtask/src/review_guard.rs) owns these rules.
The [contract tests](../../xtask/tests/review_guard.rs) exercise rolling limits, durable reservations, locking, refusal paths, and installation state.
The [runbook](../coderabbit/setup.md) owns installation and daily usage instructions.

## Consequences

Ordinary local CLI reviews cannot exceed the compiled attempt budget through concurrency, a failed preflight, or reinstalling the guard.
Conservative refusal can spend a local attempt without starting a service review.
Usage reporting is a service snapshot and cannot reserve capacity against other clients.
GitHub automatic reviews, IDE extensions, browser tasks, other machines, and deliberate raw-executable or state manipulation remain outside this local boundary.
This is an operational guard for cooperative tools, not an account-wide quota service or a sandbox against its owner.
Coordinating every client would require a service-enforced organization cap or a shared external gateway, which is outside this setup.
Changes to the compiled limits or repairs to initialized state require an explicit owner decision.
