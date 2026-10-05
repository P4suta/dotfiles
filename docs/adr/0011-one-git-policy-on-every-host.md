# Run one Git policy on every host

Status: Accepted.

## Context

The Mac and Linux run Git through `dotguard`: `~/.local/bin/git` refuses `--no-verify`, destructive commands without a per-invocation waiver, and other policy violations before the real Git runs, and the global hooks run the dotguard commit, push, and signing gates.
Windows resolved `git` to Git for Windows directly, installed its hooks through `init.templateDir`, and ran a separate `dotctl hook` implementation for signing only.
`git commit --no-verify` and destructive commands therefore passed on Windows, repositories cloned before the template existed had no hooks, and the commit-message and staged-diff gates did not run there.
`dotguard` could not build on Windows only because its wrapper used `exec` and two Unix-only agents used Unix permissions and tools.

## Decision

Windows runs the same `dotguard` as the Mac and Linux.
The runtime step installs `dotguard.exe` and a copy named `git.exe` in `~/.local/bin`; invoked under the name `git`, dotguard applies the argv policy itself and runs Git for Windows by absolute path, waiting for it and exiting with its status, because Windows has no `exec`.
The wrapper ignores console interrupts through a registered handler rather than the inherited ignore flag, so Git still receives Ctrl+C and the shell returns only after Git exits.
The PowerShell profile places `~/.local/bin` ahead of the machine PATH, where Git for Windows is installed.
Git on Windows uses the global `core.hooksPath`, whose hooks run `dotfiles-xtask hook` like the other hosts, and `dotfiles-xtask hook` calls dotguard for every gate on every host; the separate `dotctl hook` command and the template directory are removed.
The Unix-only `reaper` and `herdr-agent` binaries build on Windows and refuse to run there.

The guard's tests run the wrapper under the name `git` against the host's real Git on the Mac, Linux, and Windows, and the required Windows check now lints and tests the guard crate.
The profile contracts pin which hosts render each global hook.

## Alternatives

Hooks alone cannot refuse `--no-verify`, because Git skips the hooks that would refuse it, and no client hook runs after a push.
A `git.cmd` batch wrapper was rejected because batch argument handling mangles quoting and programs that resolve `git.exe` skip it.
Adding `~/.local/bin` to the user PATH was rejected as the precedence mechanism, because Windows places the user PATH after the machine PATH.

## Consequences

Interactive PowerShell sessions and the agents they start resolve the wrapper first, as login shells do on the Mac and Linux.
Programs that start from the machine PATH without the profile, such as a GUI editor or a session started with `-NoProfile`, still run Git for Windows directly; the global hooks, the post-commit signing rollback, and the server-side signature ruleset still apply to them.
Replacing a `git.exe` that a running Git still holds moves the old copy aside, and a later application removes it.
