# requires -Version 7 PSGallery installer, embedded in dotctl and written to a temp file by `dotctl setup tools`.
# PowerShellGet is the only interface these modules have, and one process covers the whole list because the module probe is what costs the time.
#
# A module that fails to install is a warning, not a failure: the tool set still works without PSFzf, and an unreachable gallery must not fail an otherwise good apply.

[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Modules
)

foreach ($module in $Modules -split ',') {
    $module = $module.Trim()
    if (-not $module) { continue }
    if (Get-Module -ListAvailable -Name $module) { continue }

    Write-Output ">>> Install-Module $module"
    try {
        Install-Module -Name $module -Scope CurrentUser -Force -AcceptLicense -ErrorAction Stop
    } catch {
        Write-Warning "Install-Module $module failed: $($_.Exception.Message)"
    }
}
