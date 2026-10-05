# Public source with native profiles and Rust orchestration

Status: accepted for isolated local preparation.

## Decision

Start a fresh repository from the reviewed current tree, without the old Git databases.
Keep one chezmoi source with explicit Mac, Linux, Windows, and WSL profiles.
Keep native settings in profile templates, and share the canonical skill catalog.
Keep commit identities, public key selectors, private topology, service credentials, app authentication, and persistent tool state outside the source.
Doppler injects explicitly selected API secrets into one child process.
1Password holds SSH and private signing keys.

Write procedural behavior in Rust: xtask, dotguard, and dotctl.
Keep shell configuration and small native launchers only where the operating system or a tool needs them.
Rust calls third-party installers and the Windows shortcut interface through COM with explicit arguments.

A live apply needs an explicit configuration, the matching native profile, the native home destination, a fresh backup, and `--live`.
An exclusive state lock stops two concurrent applies from sharing one private state directory.
A managed-file failure restores prior file contents, symlinks, permissions, and the previous chezmoi state database, and keeps the backup for inspection.
Package installs, service operations, OS settings, and external installer effects need native verification and fall outside managed-file rollback.

## Checked contracts

`xtask/src/profile_rules.rs` holds the production profile selection, apply eligibility, source reconciliation, secret readiness, Context7 eligibility, WSL path translation, and forge-key admission decisions.
Its seven Kani harnesses check all finite decision inputs and require reachable acceptance and refusal outcomes.
`guard/src/reaper_rules.rs` checks the production reaper eligibility conjunction and uptime arithmetic.
`xtask/src/shell_write_rules.rs` checks that the Claude Code Bash hook never admits a detected inline-interpreter or file-redirect write and always names a next action.
The skill, PR workflow, and [layered review-capacity](0008-layered-review-capacity.md) contracts stay in the required proof gate, which derives its expected harness total from the production inventories.
The gate refuses missing, duplicate, unsupported, vacuous, failed, and timed-out results, and runs invalid counterexamples as controls.

The arithmetic proof replaces only the unsigned quotient with an arbitrary `u64` under a nonzero-frequency assertion.
A separate source-bound harness checks the production code at zero uptime for every tick count and frequency, including the zero-frequency refusal.
The decomposition follows [Kani's stubbing interface](https://model-checking.github.io/kani/reference/experimental/stubbing.html).

## External boundaries

The profile gate renders all four profiles, checks critical target ownership, and parses rendered JSON, TOML, Git, Nushell, PowerShell, and POSIX launchers.
It applies files twice in owned homes and checks the resulting state, and never runs scripts or external assets.
Nushell parsing substitutes owned empty generator modules, and generated integrations need native verification.
The review ledger releases its operating-system lock explicitly on every return path after acquisition.
A descriptor-duplication regression checks release while another descriptor stays open, matching [Rust's documented file-lock semantics](https://doc.rust-lang.org/std/fs/struct.File.html#method.unlock).
Linux Ansible and GPG fixtures run the production debsig conversion with original, updated, missing, and invalid keys.
Rollout to the Mac, Linux, Windows, and WSL hosts remains a separate native stage that the owner authorizes.

Gitleaks and a private exact-value audit check the publication candidate, and provenance stays private.
Scanner coverage proves no absence of unknown secrets.
Doppler's [documented child-process injection](https://docs.doppler.com/docs/cli) and [CLI implementation](https://github.com/DopplerHQ/cli/blob/master/pkg/cmd/run.go) support explicit selection with fallback storage turned off.
Real authentication, unavailable services, app-managed caches, and private signing agents need native verification at cutover.
Isolated preparation involves no CodeRabbit service, allowance inquiry, publication, or native deployment.
