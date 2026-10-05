# Native desktop integration assurance

Status: accepted.

## Contract

Chezmoi enables the OpenCode Context7 plugin only on a Linux desktop with the native 1Password executable and no nonempty `SSH_CONNECTION`, `SSH_CLIENT`, or `SSH_TTY`.
An absent or empty SSH variable means a local session.
Provisioning checks the downloaded signing-key fingerprint, then rebuilds the debsig keyring when that key changes or the output goes missing.
An identical rerun leaves the keyring unchanged, and a converter failure fails the task.

## Assurance boundary

Chezmoi's Go templates and Ansible's Jinja expressions and command module interpret this behavior.
Executable lookup, downloads, filesystem state, and GnuPG lie outside Kani's Rust semantics.
Copying the Boolean decisions into Rust would prove a separate model, so native interpreter checks of the production files replace a proof here.
The production Rust decision proofs stay required through `check:proofs`.

`xtask/tests/desktop_integrations.rs` runs every combination of the three SSH variables, desktop availability, and supported OS values, including explicit empty variables, through the pinned chezmoi executable.
On Linux, it parses the production download and conversion tasks and runs them with pinned Ansible and real GnuPG.
Only fixture URLs, input and output paths, privilege and ownership settings, and an observation register differ from production.
The public fixtures share a primary key with different certified user identities, so source and converted bytes change while the fingerprint stays stable.
The tests cover creation, unchanged reruns, source updates with existing output, missing output, and converter failure, all inside owned temporary directories without administrator access.

`check:rust` runs these tests in required CI and the local pre-push gate.
Missing tools or production tasks, interpreter failures, wrong transitions, and malformed rendered JSON fail the check.
Ansible runs only on Linux, the target of these package-provisioning tasks, and Windows and Mac run their native chezmoi paths.
These checks cover interpreter and adapter behavior, not upstream authenticity, download freshness, cryptographic correctness, privileged deployment, or concurrent system administration.
Revisit them when interpreter semantics, SSH variables, provisioning targets, command arguments, or the signing-key trust policy change.
