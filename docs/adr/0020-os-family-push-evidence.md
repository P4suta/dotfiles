# Require a passing gate on every operating system family before push

The owner accepted this decision.

## Context

Cross-platform failures surfaced first in hosted CI, although the owner had a Mac, a Linux, and a Windows host.
The multi-machine and ci-budget skills asked for local runs, but nothing enforced them.

## Decision

`dotfiles-xtask hosts` keeps one host per OS family reachable and records gate results as Git notes under `refs/notes/hosts`.

- `hosts sync` picks one tailnet host per OS family other than this machine's and writes `~/.local/state/hosts/ssh_config` and `known_hosts`.
  Both stay machine-local.
  `~/.ssh/config` includes the first.
  The user comes from `--user`, the managed file, or a hub remote.
  The system SSH client learns a host key over the tailnet address, because Windows `ssh-keyscan` fails to negotiate with current OpenSSH servers.
  A changed key stops the sync with its fingerprint until `--accept HOST`.
- `hosts doctor` checks tailnet presence, `domyjob doctor`, and the gate's program on the login `PATH`, and prints the next action per failure.
- `hosts check` requires a clean tree, stops at the first unready host, runs the gate through each family's login shell, and appends `{host, os, tree, status, gate}` to the commit's note.
  A remote host gets a Git bundle of `HEAD` through `domyjob run --wait` and runs the gate in a clone of it, because a domyjob snapshot carries no `.git`.
  A job that stopped early records nothing.
  The gate defaults to `just check`.
- The required families come from the runner labels of every job in the commit's own `.github/workflows`, with `matrix.*` expressions resolved.
  An unresolvable runner causes an error.
- The global pre-push hook, this repository's Lefthook pre-push, and `pr-workflow ready` refuse a GitHub branch in one case.
The commit lacks a passing record for its tree on some required family.
  The gate skips deletions, tags, and pushes to the owner's hubs.

`xtask/src/hosts_rules.rs` holds the decisions, and Kani proves them with rejecting counterexamples.

## Alternatives

The owner rejected trusting hosted CI alone because it spends Actions minutes on failures the owner's hosts can find first.
The owner rejected keying evidence to the tree alone because a note must name the commit that the host checked out.
Records for another tree get ignored instead.

## Consequences

Pushing a branch to GitHub needs `hosts check` for its commit first, and an amended commit needs a new check.
Notes stay local unless pushed explicitly.
