---
status: accepted
date: 2026-10-21
---
# Sign through the password manager agent

## Status

Accepted.

## Context

Git's Unix SSH signing inherited `SSH_AUTH_SOCK` from the session.
Herdr's agent deliberately holds only the push key, so signing failed whenever that agent served the session.
Signature verification needs public keys and must work while the password manager is stopped.

## Decision

On the Mac and Linux, Git names the installed `dotfiles-ssh-keygen` executable as `gpg.ssh.program`.
The Runtime setup installs it on every Unix profile.
It replaces itself with `/usr/bin/ssh-keygen`, preserving every argument, stream, and exit status while setting `SSH_AUTH_SOCK` to the password manager socket.
Typed [chezmoi data](../../home/.chezmoidata/signing.toml) supplies each OS default.
A host may override `data.ssh_signing.socket` in its chezmoi configuration with an absolute path or a path starting with `~/`.
Chezmoi renders that selection to `~/.config/dotfiles/signing.toml` on the Mac and Linux.
Signing refuses a missing socket or a non-socket path and directs the user to start and unlock 1Password.
Verification still delegates without requiring a running agent.
Windows keeps Windows OpenSSH, and WSL keeps its existing Windows OpenSSH bridge.

Using the session agent would reintroduce the failure and weaken the separation between push and signing keys.
A shell command in `gpg.ssh.program` cannot supply the executable path Git requires.
A separate Rust binary fits the existing Cargo installation without adding a shell launcher or dispatch to unrelated runtime commands.

## Consequences

Signing no longer depends on which agent a terminal or Herdr selects.
The installed helper and rendered socket configuration become prerequisites for Unix signing.
The [pure rules](../../crates/rules/src/signing_rules.rs) prove the refusal boundary, and [OpenSSH integration tests](../../crates/dotfiles/tests/ssh_signing.rs) exercise signing and Git verification with separate agents.
OpenSSH and the socket provider remain trusted external boundaries.
