# Agent client behavior

Observed behavior of the managed agent clients that no setting in this repository changes.
Each entry records what the client does and how to work with it.

## Claude Code permission evaluation in auto mode

With `defaultMode: "auto"`, the auto mode classifier runs before the `deny`, `ask`, and `allow` rules that the [permissions documentation](https://code.claude.com/docs/en/permissions.md) describes.
The classifier refuses a command it judges destructive as "Denied by auto mode classifier" before an `ask` rule can prompt for it.
To verify that an `ask` rule prompts, use a command the classifier judges harmless.
Human confirmation for destructive commands needs `defaultMode: "default"`, because in auto mode the classifier stops them, not `ask`.

## The Bash tool on Windows

Claude Code's Bash tool on Windows runs Git Bash, separate from the PowerShell tool.
The client refuses a PowerShell cmdlet such as `Remove-Item` or `Copy-Item` in the Bash tool as a way around the PowerShell deny rules.
Use POSIX commands and standalone executables such as `git` and `chezmoi` in the Bash tool, with paths written as `/c/...` or under `"$HOME"`.

## Tool search path for agent shells on Windows

The Bash and PowerShell tools on Windows start without the mise shims on `PATH`, because no shell profile activates mise for them.
Git hooks still find their tools.
The global hooks run `dotfiles-xtask`, which starts each hook tool with the mise shims prepended, and this repository's `lefthook.yml` runs `mise x --`.
A command that an agent runs directly needs the shims prepended to `PATH` or `mise x --` in front of it.

## Claude Code host terminal on Windows

Claude Code runs in Windows Terminal, and answers about its key handling assume Windows Terminal.
