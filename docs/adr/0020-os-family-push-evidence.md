# Require a passing gate on every OS family before push

Status: Accepted.

## Context

Cross-platform failures surfaced first in hosted CI, although a Mac, a Linux, and a Windows host were available.
The multi-machine and ci-budget skills asked for local runs, but nothing enforced them.

## Decision

`dotfiles-xtask hosts` keeps one host per OS family reachable and records gate results as Git notes under `refs/notes/hosts`.

- `hosts sync` picks one tailnet host per OS family other than this machine's and writes `~/.local/state/hosts/ssh_config` and `known_hosts`.
  Both stay machine-local; `~/.ssh/config` includes the first.
  The user comes from `--user`, the managed file, or a hub remote.
  A host key is learned by the system SSH client over the tailnet address, because Windows `ssh-keyscan` cannot negotiate with current OpenSSH servers.
  A changed key stops the sync with its fingerprint until `--accept HOST`.
- `hosts doctor` checks tailnet presence, `domyjob doctor`, and the gate's program on the login `PATH`, and prints the next action per failure.
- `hosts check` requires a clean tree, stops at the first unready host, runs the gate (default `just check`) through each family's login shell, and appends `{host, os, tree, status, gate}` to the commit's note.
  A remote host gets a Git bundle of `HEAD` through `domyjob run --wait` and runs the gate in a clone of it, because a domyjob snapshot carries no `.git`.
  A job that did not finish records nothing.
- The required families are the runner labels of every job in the commit's own `.github/workflows`, with `matrix.*` expressions resolved; an unresolvable runner is an error.
- The global pre-push hook, this repository's Lefthook pre-push, and `pr-workflow ready` refuse a GitHub branch whose commit lacks, for a required family, a latest record for its tree that passed.
  Deletions, tags, and pushes to the owner's hubs are not gated.

`xtask/src/hosts_rules.rs` holds the decisions, and Kani proves them with rejecting counterexamples.

## Alternatives

Trusting hosted CI alone was rejected because it spends Actions minutes on failures the owner's hosts can find first.
Keying evidence to the tree alone was rejected because a note must name the commit it was checked out from; records for another tree are ignored instead.

## Consequences

Pushing a branch to GitHub needs `hosts check` for its commit first, and an amended commit needs a new check.
Notes stay local unless pushed explicitly.
