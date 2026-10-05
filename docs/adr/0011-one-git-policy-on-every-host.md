# Run one Git policy on every host

Status: accepted.

## Context

The Mac and Linux run Git through `dotguard`.
`~/.local/bin/git` refuses `--no-verify`, destructive commands without a per-invocation waiver, and other policy violations before the real Git runs, and the global hooks run the dotguard commit, push, and signing gates.
Windows resolved `git` straight to Git for Windows, installed its hooks through `init.templateDir`, and ran a separate `dotctl hook` for signing only.
On Windows, `git commit --no-verify` and destructive commands passed, repositories cloned before the template existed had no hooks, and the commit-message and staged-diff gates never ran.
`dotguard` failed to build on Windows only because its wrapper used `exec` and two Unix-only agents used Unix permissions and tools.

## Decision

Windows runs the same `dotguard` as the Mac and Linux.
The runtime step installs `dotguard.exe` and a copy named `git.exe` in `~/.local/bin`.
Under the name `git`, dotguard applies the argv policy itself, then runs Git for Windows by absolute path, waits for it, and exits with its status, since Windows has no `exec`.
The wrapper ignores console interrupts through a registered handler, not the inherited ignore flag, so Git still receives Ctrl+C and the shell returns only after Git exits.
The PowerShell profile places `~/.local/bin` ahead of the machine `PATH`, which holds Git for Windows.
Git on Windows uses the global `core.hooksPath`, whose hooks run `dotfiles-xtask hook` and call dotguard for every gate on every host.
The `dotctl hook` command and the template directory go away.
The Unix-only `reaper` and `herdr-agent` binaries build on Windows and refuse to run there.

The guard's tests run the wrapper under the name `git` with the host's real Git on the Mac, Linux, and Windows.
The required Windows check lints and tests the guard crate.
The profile contracts pin which hosts render each global hook.

## Alternatives

Hooks alone never refuse `--no-verify`, because Git skips the hooks that would refuse it, and no client hook runs after a push.
A `git.cmd` batch wrapper lost because batch argument handling mangles quoting and programs that resolve `git.exe` skip it.
Adding `~/.local/bin` to the user `PATH` lost because Windows places the user `PATH` after the machine `PATH`.

## Consequences

Interactive PowerShell sessions and the processes they start resolve the wrapper first, as login shells do on the Mac and Linux.
Programs that start from the machine `PATH` without the profile, such as a GUI editor or a `-NoProfile` session, still run Git for Windows directly.
The global hooks, the post-commit signing rollback, and the server-side signature ruleset still apply to them.
Replacing a `git.exe` that a running Git still holds moves the old copy aside, and a later apply removes it.
