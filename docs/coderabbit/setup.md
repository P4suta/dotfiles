# Review service setup

The shared `coderabbit-review` skill handles local reviews and existing pull request findings.
The shared `coderabbit` skill handles service configuration, Plan, Triage, security evidence, and Finishing Touches.
Both live in `~/.agents/skills` with relative Claude aliases, and Codex and OpenCode read the same canonical files.
The shared `dotfiles` skill applies the setup through each host's native profile and verifies the installed behavior.

## Installation

The dotfiles mise template pins the CodeRabbit command-line tool with per-platform checksums from the official release manifest.
Mise renames the upstream executable to `coderabbit-vendor`, and the Rust guard owns `~/.local/bin/coderabbit` and `cr`.
The chezmoi setup script builds and installs the guard through the repository's xtask.
For a scoped installation from this checkout, run `mise run install:review-guard`, then run `coderabbit auth login` on each machine.
Never synchronize `.coderabbit` authentication, history, or incremental checkpoints.
Change the version through dotfiles, never through the tool's self-updater.

## Local review limits

On the Mac, all repositories and processes share a limit of seven attempts per rolling hour and 168 per rolling 24 hours.
It reserves and flushes an attempt before querying the service allowance, keeps failed and interrupted attempts, and holds a file lock until the invocation ends.
A review requires a complete numeric allowance response for `P4suta` and inactive usage billing.
Consumption after the proposed review must stay within `floor(capacity * 0.9 * 0.8)`, so a limit of ten admits seven reviews and refuses the eighth.
The local ceiling stays at seven even when the service reports a higher limit, and a lower verified service limit tightens admission.
Linux and Windows refuse managed reviews and usage queries, so only one host spends the managed pool.
Usage queries, including internal preflight and failed probes, have their own durable ledger, a one-minute gap between queries, and the same hourly and daily limits.
The guard queries usage during preflight, so skip a separate usage query right before a review.
Missing or corrupt state, clock rollback, concurrent runs, unknown billing state, and unavailable quota refuse analysis with exit code 75.
Audit ledgers stay append-only and refuse further work past 1 MiB with an explicit maintenance diagnostic.
Read-only commands, authentication, and saved findings spend no review attempts.
The guard refuses paid-credit flags, inline API keys, remote review scopes, cloud coding, and the self-update command.
To review another repository, use a local checkout and `--dir`, so preflight queries the intended repository.

Inspect local capacity with `coderabbit --guard-status`, which never contacts the service.
An owner pause lives at `~/.local/state/coderabbit-guard/paused` and blocks every vendor call, including usage probes and authentication status, until the owner resumes.
Reinstalling the guard keeps the pause.
The review ledger, the `usage.log` probe ledger, and the initialization markers live under `~/.local/state/coderabbit-guard`, outside dotfiles synchronization.
Reinstallation keeps history and refuses to recreate a ledger deleted after initialization.
Never delete state, call the vendor executable directly, create another installation, or retry automatically to bypass a refusal.
State repair and limit changes need an explicit owner decision.

The guard controls the managed entry points on the installed machine, including `mise x -- coderabbit`.
GitHub automatic reviews, IDE extensions, browser tasks, unmanaged hosts, and direct calls to the raw executable fall outside it.
Keep paid service add-ons inactive.
See the [guard decision](../adr/0002-coderabbit-local-review-budget.md) for the enforcement boundary.

## Service configuration

Apply [organization defaults](organization.yaml) in Organization settings → YAML editor.
Apply [global overrides](global-overrides.yaml) in Organization settings → Global overrides.
The [original organization configuration](organization-before.yaml) records the settings before this setup, when global overrides held nothing.
Global overrides keep the shared automatic-review policy, two advisory checks, and the selected on-demand Finishing Touches in force when a repository's UI settings take precedence.

Automatic review covers ready pull requests and incremental pushes, skips drafts, and pauses after five reviewed commits.
Resume a paused review only when the final change stands ready and the user authorizes the GitHub comment.
The two advisory checks assess behavior verification evidence and public interface compatibility disclosure, and they run no tests.
Docstring coverage and generation stay off.
Autofix, test generation, and supported CI fixes run on demand.

## Daily workflow

Keep usage-based reviews, the paid security scan, the paid coding add-on, and automatic seat assignment off, even when a review hits its allowance.

For large work, refine a CodeRabbit Plan, select a phase, and copy its handoff prompt into the implementation workflow.
Finish the local change and its project checks before preparing a pull request.
Run one budget-qualified command-line review after local checks for an initial substantive pull request, and read the existing pull request review first when updating one.
The [pull request workflow](../../dot_agents/skills/pull-request/references/workflow.md) requires numeric capacity for operations that start a pull request review, and holds publication when that capacity stays unverified.
The official status inquiry reports only availability, which establishes no numeric reserve.
The same formula applies to verified pull request hourly allowance and adaptive activity thresholds.
Verify the applicable window and counts first.
A confirmed seven-day highest-band threshold of 49 then gives 35 review events per rolling 168 hours.
Waiting an hour permits reconsideration, not publication.
Wait for the final pushed head's review, inspect CI and unresolved findings, fix supported findings, and rerun affected checks before an authorized update.
After updating a pull request, inspect the automatic review of the new head before reporting completion.
Explain unsupported findings with code evidence, and report paused, skipped, failed, stale, or rate-limited reviews as pending.
For a requested local review, request a second run only when substantive fixes justify it and the guard permits it.
Batch related commits into one push, and wait for the active review before updating the pull request again.

Use a Triage saved view filtered to open pull requests that `P4suta` opened.
Inspect CI, unresolved feedback, current-head review state, and security risk together.
Treat a missing or rate-limited review as incomplete analysis even when GitHub reports a passing CodeRabbit check.
Read the architecture and impact reviews for changes to trust boundaries.

Steering, revisions, retries, and follow-up turns of Finishing Touches can spend billed add-on usage, so stop at any request for paid continuation.
Posting a command or choosing direct commit delivery needs the current user authorization, and this setup grants no lasting commit or push permission.

## Verification

Run `mise run check:rust` for the shared skill tree, aliases, and guard contracts.
Check both skills with the skill-creator quick validator, render and lint the mise configuration, and check the service YAML with the current official CodeRabbit schema.
Run `coderabbit --version`, `coderabbit auth status`, and `coderabbit doctor` to check the runtime and connection without starting a paid review or cloud task.
Run `coderabbit --guard-status` and `mise x -- coderabbit --guard-status`, and confirm that `coderabbit review --use-credits` exits with code 75 and leaves the ledger unchanged.
Confirm saved service settings by reloading the management page.
