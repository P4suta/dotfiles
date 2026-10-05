# Machine-local push control

The personal Git hook refuses pushes while `~/.config/git/push-paused` exists.
Inspect the effective `core.hooksPath` and its `pre-push` hook before assuming the guard works.
Use `dotfiles` to add the hook to a profile that lacks it.

Keep the marker machine-local and out of dotfiles deployment.
Never clear it during setup or verification.

For Bash or Zsh on Mac and Linux:

```sh
test -e "${HOME}/.config/git/push-paused"
mkdir -p "${HOME}/.config/git"
touch "${HOME}/.config/git/push-paused"
```

The first command checks the state, and the other two pause pushes.
Resume only on explicit owner instruction with `rm -- "${HOME}/.config/git/push-paused"`.

For PowerShell on Windows, resolve the Git wrapper's home directory from the environment:

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
After explicit resumption, run `Remove-Item -LiteralPath $taskPushHold`.
