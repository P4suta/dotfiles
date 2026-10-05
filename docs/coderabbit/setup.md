# CodeRabbit setup

The shared `coderabbit-review` skill handles local reviews and existing PR findings.
The shared `coderabbit` skill handles service configuration, Plan, Triage, security evidence, and Finishing Touches.
Both live in `~/.agents/skills` with relative Claude aliases.
The shared `dotfiles` skill applies the setup through each host's native profile.

## Installation

The dotfiles mise template pins the CodeRabbit CLI with per-platform checksums from the official release manifest.
Mise renames the upstream executable to `coderabbit-vendor`, and the Rust guard owns `~/.local/bin/coderabbit` and `cr`.
The chezmoi setup script builds and installs the guard through the repository's xtask.
For a scoped installation from this checkout, run `mise run install:review-guard`, then run `coderabbit auth login` on each machine.
Never synchronize `.coderabbit` authentication, history, or incremental checkpoints.
Change the version through dotfiles, never through the CLI's self-updater.

## Local review limits

On the Mac, all repositories and processes share a limit of seven attempts per rolling hour and 168 per rolling 24 hours.
The guard reserves and flushes an attempt before querying the service allowance, keeps failed and interrupted attempts, and holds a file lock until the invocation ends.
A review requires a complete numeric allowance response for `P4suta` and inactive usage billing.
Consumption after the proposed review must stay within `floor(capacity * 0.9 * 0.8)`, so a limit of ten admits seven reviews and refuses the eighth.
The local ceiling stays at seven even when the service reports a higher limit, and a lower verified service limit tightens admission.
Linux and Windows refuse managed reviews and usage queries.
Usage queries, including internal preflight and failed probes, have their own durable ledger, a one-minute gap between queries, and the same hourly and daily limits.
The guard queries usage during preflight, so skip a separate usage query right before a review.
Missing or corrupt state, clock rollback, concurrent runs, unknown billing state, and unavailable quota refuse analysis with exit code 75.
Audit ledgers stay append-only and refuse further work past 1 MiB with a maintenance diagnostic.
Read-only commands, authentication, and saved findings spend no review attempts.
The guard refuses paid-credit flags, inline API keys, remote review scopes, cloud coding, and the self-update command.
To review another repository, use a local checkout and `--dir`.

Inspect local capacity with `coderabbit --guard-status`, or `coderabbit --guard-status --json` for its machine-readable form.
`--guard-status` never contacts the service.
An owner pause lives as a marker in `~/.local/state/coderabbit-guard`: `paused-pr` pauses PR reviews, `paused-cli` pauses CLI use, and `paused` pauses both.
A CLI pause blocks every vendor call, including usage probes and authentication status, and a PR pause makes `pr-workflow` refuse operations that would request a PR review.
Reinstalling the guard keeps every marker.
The review ledger, the `usage.log` probe ledger, and the initialization markers live under `~/.local/state/coderabbit-guard` and stay out of dotfiles synchronization.
Reinstallation keeps history and refuses to recreate a ledger deleted after initialization.
First installation synchronizes, locks, and validates the ledger before it publishes the initialization marker, so an interrupted setup can retry without discarding history.
Never delete state, call the vendor executable directly, create another installation, or retry automatically to bypass a refusal.
State repair and limit changes need an explicit owner decision.

The guard controls the managed entry points on the installed machine, including `mise x -- coderabbit`.
Keep paid service add-ons inactive.
See the [guard decision](../adr/0002-coderabbit-local-review-budget.md) for the enforcement boundary.

## Service configuration

Apply [organization defaults](organization.yaml) in Organization settings → YAML editor.
Apply [global overrides](global-overrides.yaml) in Organization settings → Global overrides.
The [original organization configuration](organization-before.yaml) records the settings before this setup, when global overrides held nothing.

Automatic review covers ready PRs and incremental pushes, skips drafts, and pauses after five reviewed commits.
Resume a paused review only when the final change stands ready and the user authorizes the GitHub comment.
The two advisory checks assess behavior verification evidence and public interface compatibility disclosure, and they run no tests.
Docstring coverage and generation stay off.
Autofix, test generation, and supported CI fixes run on demand.

## Daily workflow

Keep usage-based reviews, CodeRabbit Security Scan, CodeRabbit Agent, and automatic seat assignment off, even when a review hits its allowance.

For large work, refine a CodeRabbit Plan, select a phase, and copy Agent Handoff into the implementation workflow.
Finish the local change and its project checks before preparing a PR.
Run one budget-qualified CLI review after local checks for an initial substantive PR, and read the existing PR review first when updating one.
The [PR workflow](../../dot_agents/skills/pull-request/references/workflow.md) requires numeric capacity for operations that start a PR review, and holds publication when that capacity stays unverified.
The same formula applies to verified PR hourly allowance and adaptive activity thresholds.
Verify the applicable window and counts first.
A confirmed seven-day highest-band threshold of 49 then gives 35 review events per rolling 168 hours.
Waiting an hour permits reconsideration, not publication.
Wait for the final pushed head's review, inspect CI and unresolved findings, fix supported findings, and rerun affected checks before an authorized update.
Explain unsupported findings with code evidence, and report paused, skipped, failed, stale, or rate-limited reviews as pending.
For a requested local review, request a second run only when substantive fixes justify it and the guard permits it.
Batch related commits into one push, and wait for the active review before updating the PR again.

Use a Triage saved view filtered to open PRs that `P4suta` opened.
Inspect CI, unresolved feedback, current-head review state, and security risk together.
Treat a missing or rate-limited review as incomplete analysis even when GitHub reports a passing CodeRabbit check.

Steering, revisions, retries, and follow-up turns of Finishing Touches can spend billed Agent usage, so stop at any request for paid continuation.
Posting a command or choosing direct commit delivery needs the current user authorization.

## Verification

Run `mise run check:rust` for the shared skill tree, aliases, and guard contracts.
Run `coderabbit --version`, `coderabbit auth status`, and `coderabbit doctor` to check the runtime and connection without starting a paid review or cloud task.
Run `coderabbit --guard-status` and `mise x -- coderabbit --guard-status`, and confirm that `coderabbit review --use-credits` exits with code 75 and leaves the ledger unchanged.
