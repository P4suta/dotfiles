# requires -Version 7 Runs one command elevated, embedded in dotctl and written to a temp file when a setup step needs an admin token.
#
# Why PowerShell rather than a ShellExecute call from Rust: `Start-Process -Verb RunAs -Wait -PassThru` is the mechanism already proven on this machine, and a UAC prompt cannot be answered from a test.
#
# -Verb RunAs and -RedirectStandardOutput belong to different parameter sets,
# so the elevated child cannot have its streams captured from outside.
# It redirects its own into the log instead, and `exit $LASTEXITCODE` carries the real exit code back out through `pwsh -Command`, which otherwise reports a bare 1.
#
# The window is hidden: a console that appears, scrolls something nobody can read and vanishes is worse than no window at all.
# The caller tails the log while this runs, so the output shows up where the command was typed.

[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Manifest
)

$ErrorActionPreference = 'Stop'

$spec = Get-Content -Raw -LiteralPath $Manifest | ConvertFrom-Json

function Quote([string]$value) {
    "'" + ($value -replace "'", "''") + "'"
}

$arguments = ($spec.arguments | ForEach-Object { Quote $_ }) -join ' '
$inner = "& $(Quote $spec.exe) $arguments *> $(Quote $spec.log); exit `$LASTEXITCODE"

$elevated = Start-Process -FilePath (Get-Process -Id $PID).Path -Verb RunAs -Wait -PassThru `
    -WindowStyle Hidden `
    -ArgumentList @('-NoLogo', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-Command', $inner)

exit $elevated.ExitCode
