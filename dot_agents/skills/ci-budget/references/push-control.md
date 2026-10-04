# Machine-local Push Control

The personal Git hook uses the presence of `~/.config/git/push-paused` to refuse pushes before repository checks.
Inspect the effective `core.hooksPath` and its `pre-push` hook before assuming this guard is installed.
Native profiles may use a global hooks directory or a template-installed hook that delegates to another tool.
Use `dotfiles` to add equivalent behavior to a missing profile while preserving that host's signing and project gates.

The marker affects Git pushes from that machine wherever the hook is used, including non-GitHub remotes.
It is not an account-wide Actions budget, a GitHub merge lock, or a control over another machine, web UI, API, scheduled workflow, or already-running job.
Keep the marker machine-local and exclude it from dotfiles deployment.
Never silently clear it during setup or verification.

For Bash or Zsh on Mac and Linux:

```sh
test -e "${HOME}/.config/git/push-paused"
mkdir -p "${HOME}/.config/git"
touch "${HOME}/.config/git/push-paused"
```

The first command checks state, and the remaining commands pause pushes only when requested.
Resume only after an explicit owner instruction with `rm -- "${HOME}/.config/git/push-paused"`.

For native PowerShell on Windows, resolve the Git wrapper's home directory from the current environment:

```powershell
$taskPushHome = $env:HOME
if (-not $taskPushHome) {
    if ($env:HOMEDRIVE -and $env:HOMEPATH) {
        $taskPushHome = $env:HOMEDRIVE + $env:HOMEPATH
    } else {
        $taskPushHome = $env:USERPROFILE
    }
}
if (-not $taskPushHome) { throw "Git home directory is unavailable" }
$taskPushHold = Join-Path $taskPushHome ".config/git/push-paused"
Test-Path -LiteralPath $taskPushHold
New-Item -ItemType Directory -Force -Path (Split-Path $taskPushHold) | Out-Null
New-Item -ItemType File -Force -Path $taskPushHold | Out-Null
```

Only the last two commands pause pushes.
After explicit resumption, use `Remove-Item -LiteralPath $taskPushHold`.
Keep fetch and local validation available; clearing the marker does not skip the ordinary signing, hook, CI, or PR review gates.
