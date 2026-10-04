# CodeRabbit Setup

The shared `coderabbit-review` skill handles local reviews and existing PR findings.
The shared `coderabbit` skill handles service configuration, Plan, Triage, security evidence, and Finishing Touches.
Both live in `~/.agents/skills` with relative Claude aliases; Codex and OpenCode discover the same canonical files.
The shared `dotfiles` skill applies equivalent setup through each Mac, Linux, and Windows host's native profile and verifies its installed behavior.

CodeRabbit CLI is pinned in the dotfiles mise template with platform-specific checksums from the official release manifest.
Mise renames the upstream executable to `coderabbit-vendor`; the maintained Rust guard owns `~/.local/bin/coderabbit` and `cr`.
The chezmoi setup script builds and installs that guard through the repository's xtask after applying tool configuration.
For a scoped installation from this checkout, run `mise run install:review-guard`, then authenticate on each machine with `coderabbit auth login`.
Do not synchronize `.coderabbit` authentication, history, or incremental checkpoints.
Use the installed CLI's help rather than its self-updater to change a dotfiles-managed version.

## Local review limits

The guard permits seven attempts per rolling hour and 168 per rolling 24 hours, shared across all repositories and agent processes on the Mac.
It reserves and flushes an attempt before querying service allowance, retains failed and interrupted attempts, and holds an operating-system file lock until the invocation ends.
A review requires a complete numeric allowance response for `P4suta` and inactive usage billing.
Consumption after the proposed review must not exceed `floor(capacity * 0.9 * 0.8)`: 90% is the ceiling, with 20% of that ceiling retained as margin.
The calculation uses exact overflow-safe integers and rounds down once; a limit of ten admits seven reviews and refuses the eighth.
The local Advanced ceiling remains seven even if a service response reports a higher limit; a lower verified service limit tightens admission.
Managed reviews and usage queries are refused on Linux and Windows, so the three installed hosts cannot independently spend the same managed CLI pool.
Usage queries, including internal preflight and failed probes, have their own durable ledger, a one-minute minimum interval, and limits of seven per rolling hour and 168 per rolling day.
These are managed traffic ceilings, not an asserted CodeRabbit API rate limit.
The guard queries usage during preflight; avoid an extra usage query immediately before reviewing.
The CLI reported an included limit of ten reviews per rolling hour on October 3, 2026; the guard verifies the current response rather than relying on that observation forever.
Missing or corrupt state, clock rollback, concurrent runs, unknown billing state, and unavailable quota refuse analysis with exit code 75.
Audit ledgers remain append-only and refuse further work above 1 MiB with an explicit operator-maintenance diagnostic.
Higher usage reaches that boundary sooner; the guard never silently discards expired history or resets allowance to continue.
Read-only commands, authentication, and saved findings do not consume review attempts.
Paid-credit flags, inline API keys, remote review scopes, cloud coding, and the self-update command are refused.
Use a local checkout and `--dir` when selecting another repository so preflight queries the intended repository.

Inspect local capacity with `coderabbit --guard-status`.
An owner-imposed pause is recorded as `~/.local/state/coderabbit-guard/paused` and blocks vendor calls, including usage probes and authentication status, until explicitly resumed.
Local `--guard-status` still reports capacity and pause state without contacting the service, and reinstalling the guard preserves the pause.
The review ledger, separate `usage.log` probe ledger, and initialization markers live under `~/.local/state/coderabbit-guard` and are excluded from dotfiles synchronization.
Reinstallation preserves history and refuses to recreate a ledger deleted after initialization.
First installation publishes its initialization marker only after the ledger is synchronized, locked, and validated, so an interrupted setup can retry without discarding an existing history.
On Unix, the state directory and its home-directory ancestry are synchronized before marker creation, and the state directory is synchronized again afterward so the file entries are persisted as well as their contents.
Do not delete state, invoke the vendor executable directly, create another installation, or retry automatically to bypass refusal.
State repair and limit changes require an explicit owner decision.

This controls the managed CLI entry points on the installed machine, including ordinary `mise x -- coderabbit`.
GitHub automatic reviews, IDE extensions, browser tasks, unmanaged hosts, and deliberate invocation of the raw executable are outside this local boundary.
Service-side paid add-ons must remain inactive independently of the local limit.
See the [guard decision](../adr/0002-coderabbit-local-review-budget.md) for the enforcement boundary and rationale.

## Service configuration

Apply [organization defaults](organization.yaml) in Organization settings → YAML editor.
Apply [global overrides](global-overrides.yaml) in Organization settings → Global overrides.
The [original organization configuration](organization-before.yaml) preserves the settings observed before this setup; global overrides were empty.
Global overrides ensure the shared automatic-review policy, two advisory checks, and selected on-demand Finishing Touches are available even when a repository's UI settings take precedence over organization defaults.
Repository-specific instructions and merge protections remain governed by their existing configuration.

Use English for outward-facing reviews and artifacts.
Automatic review covers ready PRs and incremental pushes, excludes drafts, and pauses after five reviewed commits to bound repeated review work.
Resume a paused review deliberately when the final change is ready and the user authorizes the GitHub comment.
The two advisory checks assess behavior verification evidence and public interface compatibility disclosure.
They inspect source and PR evidence; they do not execute tests or replace deterministic CI.
Docstring coverage and generation are disabled to match concise-source preferences.
Autofix, test generation, and supported CI fixes remain available on demand.

## Account and daily workflow

The account observed on October 3, 2026 has Advanced, one assigned seat, annual billing of $864, and renewal on October 3, 2027.
Usage-based reviews, CodeRabbit Security Scan, and CodeRabbit Agent are inactive; automatic seat assignment is off.
Leave these paid add-ons inactive under the fixed-fee policy, including when a review hits its allowance.

For substantial work, refine a CodeRabbit Plan, select a useful phase, and copy Agent Handoff into the existing implementation workflow.
Complete the local change and relevant project checks before preparing a purposeful PR.
Automatic PR review remains the final completion path; run one budget-qualified CLI review after local checks for an initial substantive PR and use the existing PR review first when updating a PR.
The [PR workflow](../../dot_agents/skills/pull-request/references/workflow.md) requires independent numeric capacity for operations that start a PR review and holds publication when that capacity cannot be verified.
The official status inquiry observed on October 4, 2026 reported only that reviews were available, so that response does not establish a numeric reserve.
The same layered formula applies to verified PR hourly allowance and adaptive activity thresholds.
A confirmed seven-day highest-band threshold of 49 gives a budget of 35 review events per rolling 168 hours, subject to verification of the actual applicable window and counts.
Waiting an hour permits reconsideration, not automatic publication or a claim that capacity has refilled.
The shared agent policy routes existing PR work and authorized PR updates to `coderabbit-review` in Codex, Claude Code, and OpenCode.
Wait for the final pushed head's review, inspect CI and unresolved findings, fix supported actionable findings, and rerun affected checks before an authorized update.
After updating a PR, inspect the automatic review of the new head before reporting completion.
Explain unsupported findings with concrete code evidence, and report paused, skipped, failed, stale, or rate-limited reviews as pending.
For an explicitly requested local review, request a second run only when substantive fixes justify it and the guard permits it.
Batch related commits into a meaningful push and wait for the active review before updating the PR again.

Use a Triage saved view with an explicit `P4suta` author filter for the user's open PRs.
Inspect CI, unresolved feedback, current-head review state, and security risk together.
Read missing or rate-limited review status as incomplete analysis even if GitHub reports a passing CodeRabbit check.
Read Architecture Review and Blast Radius for changes to trust boundaries; repository-wide Security Scan is separately billed.

The initial eligible Finishing Touches turn can be free, but steering, revision, retries, and follow-up turns can consume billed Agent usage.
With the add-on inactive, stop at requests for paid continuation.
Posting a command or choosing direct commit delivery must follow the current user authorization; this setup does not grant permanent commit or push permission.

## Verification

Run `mise run check:rust` for the shared skill tree, aliases, and Rust guard contracts.
Validate both new skills with the skill-creator quick validator, render and lint the mise TOML, and validate the service YAML against the current official CodeRabbit schema.
Use `coderabbit --version`, `coderabbit auth status`, and `coderabbit doctor` to verify the installed runtime and connection without starting a paid review or cloud task.
Verify both `coderabbit --guard-status` and `mise x -- coderabbit --guard-status`, and confirm that `coderabbit review --use-credits` exits with code 75 without altering the ledger.
Confirm saved service settings by reloading the management page.

The guard, pinned vendor runtime, shared workflow skills, and client instructions were installed and verified on the owner's Mac, Linux, and Windows on October 3, 2026.
Native `check:rust` passed on all three, and both guarded aliases and `mise x -- coderabbit --guard-status` reported zero attempts on every machine.
CLI authentication is complete on the Mac; Linux and Windows still require their own `coderabbit auth login` before service analysis.
