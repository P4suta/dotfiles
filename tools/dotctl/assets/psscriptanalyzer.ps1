# requires -Version 7 PSScriptAnalyzer runner, embedded in dotctl and written to a temp file when `dotctl lint ps1` runs.
# PowerShell is the only interface the analyzer has.
#
# One invocation checks every file, because importing the module costs well over a second and paying that per file turned a 6-second lint into a 37-second one.
# The manifest carries each file's rendered text next to the source path to name in the report; -ScriptDefinition keeps the analyzer off the disk, which is also where -Path spent its time.
#
# Exits 1 when anything is found, so the caller can stay in Rust.

[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Manifest,
    [Parameter(Mandatory)][string]$Settings
)

# A missing module or unreadable settings must fail the lint rather than report a clean run.
$ErrorActionPreference = 'Stop'
Import-Module PSScriptAnalyzer

$entries = Get-Content -Raw -LiteralPath $Manifest | ConvertFrom-Json
$exitCode = 0

foreach ($entry in $entries) {
    $findings = Invoke-ScriptAnalyzer -ScriptDefinition $entry.content -Settings $Settings
    if (-not $findings) { continue }

    Write-Output "::error file=$($entry.label):: PSScriptAnalyzer found $($findings.Count) issue(s)"
    $findings | Format-Table -AutoSize Severity, RuleName, Line, Message | Out-String -Width 200
    $exitCode = 1
}

exit $exitCode
