# Native desktop integration assurance

Status: accepted.

## Contract

Chezmoi enables the OpenCode Context7 plugin only on a Linux desktop with the native 1Password executable and no nonempty `SSH_CONNECTION`, `SSH_CLIENT`, or `SSH_TTY`.
An absent or empty SSH variable means a local session.
Provisioning checks the downloaded signing-key fingerprint, then rebuilds the debsig keyring when that key changes or the output goes missing.
An identical rerun leaves the keyring unchanged, and a converter failure fails the task.

## Assurance boundary

Chezmoi's Go templates and Ansible's Jinja expressions and command module interpret this behavior.
Executable lookup, downloads, filesystem state, and GPG lie outside Kani's Rust semantics.
Native interpreter checks of the production files replace a proof here, and the production Rust decision proofs stay required through `check:proofs`.

`xtask/tests/desktop_integrations.rs` runs every combination of the three SSH variables, desktop availability, and supported OS values, including explicit empty variables, through the pinned chezmoi executable.
On Linux, it parses the production download and conversion tasks and runs them with pinned Ansible and real GPG.
Only fixture URLs, input and output paths, privilege and ownership settings, and an observation register differ from production.
The tests cover creation, unchanged reruns, source updates with existing output, missing output, and converter failure.

`check:rust` runs these tests in required CI and the local pre-push gate.
These checks cover interpreter and adapter behavior, not upstream authenticity, download freshness, cryptographic correctness, privileged deployment, or concurrent system administration.
Revisit them when interpreter semantics, SSH variables, provisioning targets, command arguments, or the signing-key trust policy change.
