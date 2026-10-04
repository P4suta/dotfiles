# Public source with native profiles and Rust orchestration

Status: accepted for isolated local preparation.

## Decision

Use a fresh repository containing only the reviewed current tree, without importing the old Git databases.
Keep one chezmoi source with explicit Mac, Linux, Windows, and WSL profiles.
Preserve different native settings in profile templates and share the canonical agent skill catalog.
Keep author identities, public key selectors, private topology, service credentials, application authentication, and persistent tool state outside the source.
Doppler injects explicitly selected API secrets into a consuming child process.
1Password retains SSH and private signing keys.

Implement procedural behavior in Rust xtask, dotguard, and dotctl.
Retain shell configuration and small native launchers where the operating system or tool requires them.
Third-party installers and the Windows shortcut COM API remain external native boundaries.
They are called with explicit arguments by Rust rather than reimplemented as repository shell programs.

Live application requires an explicit configuration, the matching native profile, the native home destination, a fresh backup, and `--live`.
An exclusive state lock prevents overlapping applications using the same private state directory.
Managed-file failures restore prior file contents, symlinks, permissions, and the previous chezmoi state database.
The backup remains available for inspection.
Package installs, service operations, OS settings, and external installer effects require native verification and are outside managed-file rollback.
Owner-controlled filesystem concurrency and process identity races are external boundaries; these mechanisms do not claim an atomic system-wide transaction.

## Checked contracts

`xtask/src/profile_rules.rs` supplies the actual profile selection, application eligibility, source reconciliation, secret readiness, Context7 eligibility, WSL path translation, and forge-key admission decisions.
Its seven Kani harnesses check all finite decision inputs and require reachable acceptance and refusal outcomes.
`guard/src/reaper_rules.rs` checks the production reaper eligibility conjunction and uptime arithmetic.
Existing skill, PR workflow, and layered review-capacity contracts ([ADR 0008](0008-layered-review-capacity.md)) remain part of the required proof gate, which derives its expected harness total from the production inventories.
The gate rejects missing, duplicate, unsupported, vacuous, failed, and timed-out results and runs deliberately invalid counterexamples.

The arithmetic proof replaces only the unsigned quotient with an arbitrary `u64` under a nonzero-frequency assertion.
This sound overapproximation checks the caller for every possible production quotient; it does not prove an exact division formula.
A separate source-bound harness checks the actual implementation at zero uptime for every tick count and frequency, including the zero-frequency refusal.
The decomposition keeps the existing per-harness timeout and follows [Kani's stubbing interface](https://model-checking.github.io/kani/reference/experimental/stubbing.html).

## External boundaries

The profile gate renders all four profiles, checks critical target ownership, parses rendered JSON, TOML, Git, Nushell, PowerShell, and POSIX launchers, applies files twice in owned homes, and verifies their resulting state.
Fixture paths include spaces and an apostrophe.
Scripts and external assets are never executed by this gate.
Nushell parsing substitutes owned empty generator modules and includes the production rendered environment; actual generated integrations require native verification.
Fault injection checks rollback, generator output preservation, hook configuration parsing, CLI scope, and consumer exit behavior.
The review ledger owns an explicitly released operating-system lock, including every early-return path after acquisition.
A descriptor-duplication regression checks release while another descriptor remains open, matching [Rust's documented file-lock semantics](https://doc.rust-lang.org/std/fs/struct.File.html#method.unlock).
Linux Ansible/GPG fixtures exercise the production debsig conversion with original, updated, missing, and invalid key inputs.
Actual Mac, Linux, Windows, and WSL rollout remains a separate owner-authorized native stage.

Gitleaks and a private exact-value audit check the publication candidate and provenance remains private.
Scanner coverage and reviewed ownership establish publication evidence; they do not prove absence of every unknown secret.
Doppler's [documented child-process injection](https://docs.doppler.com/docs/cli) and [CLI implementation](https://github.com/DopplerHQ/cli/blob/master/pkg/cmd/run.go) support explicit selection and disabled fallback storage.
Real authentication, unavailable services, application-managed caches, and private signing agents need native verification at cutover.
No CodeRabbit service, allowance inquiry, publication, or native deployment is part of isolated preparation.
