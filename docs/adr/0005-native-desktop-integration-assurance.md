# Native desktop integration assurance

Status: Accepted

## Contract

The OpenCode Context7 plugin is enabled only for a Linux desktop with the native 1Password executable and no nonempty `SSH_CONNECTION`, `SSH_CLIENT`, or `SSH_TTY`.
Absent and empty SSH variables represent local sessions.
The debsig keyring is regenerated when the downloaded signing key changes or the output is absent, remains unchanged on an identical rerun, and propagates converter failure.
The existing signing-key fingerprint validation precedes conversion.

## Assurance boundary

Production behavior is interpreted by chezmoi's Go templates and Ansible's Jinja expressions and command module, with executable lookup, downloads, filesystem state, and GPG outside Kani's executable Rust semantics.
Copying the Boolean decisions into Rust would prove a separate model rather than these production expressions.
Use native interpreter checks of the actual production files as the required alternative for this scoped boundary.
Existing production Rust decision proofs remain required through `check:proofs`.

`xtask/tests/desktop_integrations.rs` exercises all combinations of the three SSH indicators, desktop availability, and supported OS values, including explicit empty variables, through the pinned chezmoi executable.
On Linux, it parses the production download and conversion tasks and executes them with pinned Ansible and real GPG.
Only fixture URLs, input/output paths, privilege and ownership settings, and an observation register differ from production.
The public fixtures share a primary key but have different certified user identities, so source and converted bytes change while the primary fingerprint remains stable.
Tests check initial creation, unchanged reruns, source updates with existing output, missing output, and converter failure.
All writes stay in owned temporary directories without administrator access.

`check:rust` runs these tests as part of the existing required CI and local pre-push gate.
Missing tools or production tasks, interpreter failures, incorrect transitions, and malformed rendered JSON fail verification.
Ansible execution is Linux-only because these are Linux package-provisioning tasks; Windows and Mac exercise their native chezmoi paths.
These checks establish the stated interpreter and adapter behavior, not upstream authenticity, download freshness, cryptographic correctness, privileged deployment, or arbitrary concurrent system administration.
Revisit the assessment when interpreter semantics, SSH indicators, provisioning targets, command arguments, or the signing-key trust policy change.
