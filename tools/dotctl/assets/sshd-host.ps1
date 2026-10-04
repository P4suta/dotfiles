# The Windows-side effectors for `dotctl setup sshd`, embedded in dotctl and written to a temp file when it runs.
#
# Runs under Windows PowerShell 5.1, not pwsh 7, and that is not a style choice: DISM ships as a 5.1 module, and from an elevated pwsh 7 Get-WindowsCapability fails with "Class not registered" (0x80040154) rather than doing anything.
# Unelevated it gets as far as the privilege check, so the failure only appears once the UAC prompt has been answered.
# Everything else here - the service, the firewall, the connection profiles - exists in 5.1 too, so the whole helper lives there.
#
# These four APIs - DISM capability, the service control manager, the firewall, and the network connection profiles - have no locale-independent command-line form: `dism /Get-Capabilities` and `sc query` print localized prose, and `netsh advfirewall` addresses rules by display name, so it can neither see nor set the stable rule Name that Get-NetFirewallRule uses.
# Their PowerShell cmdlets return objects instead, so the boundary is JSON in and JSON out, with every decision left to the caller.
#
# -Query prints the current state.
# -Apply takes a JSON request describing the changes and prints what it did.
#
# stdout carries the JSON and nothing else.
# Progress goes to stderr, which the caller lets through to its own stderr, so a step that takes minutes - a capability install pulls hundreds of megabytes from Windows Update - says so while it happens instead of after.

[CmdletBinding(DefaultParameterSetName = 'Query')]
param(
    [Parameter(Mandatory, ParameterSetName = 'Query')][switch]$Query,
    [Parameter(Mandatory, ParameterSetName = 'Apply')][string]$Apply
)

$ErrorActionPreference = 'Stop'

function Write-Progress-Line([string]$message) {
    [Console]::Error.WriteLine("    $message")
}

function Get-State {
    $capability = Get-WindowsCapability -Online -Name 'OpenSSH.Server*' |
        Sort-Object -Property Name | Select-Object -Last 1
    $service = Get-Service -Name sshd -ErrorAction SilentlyContinue
    # When the service's listener started, so the caller can tell whether it predates the config on disk.
    # Per-session sshd processes start later and must not stand in for the listener.
    $startedAt = $null
    $servicePid = (Get-CimInstance -ClassName Win32_Service -Filter "Name='sshd'" -ErrorAction SilentlyContinue).ProcessId
    if ($servicePid) {
        $startedAt = (Get-Process -Id $servicePid -ErrorAction SilentlyContinue).StartTime
    }
    $rule = Get-NetFirewallRule -Name 'OpenSSH-Server-In-TCP' -ErrorAction SilentlyContinue

    [pscustomobject]@{
        capability = if ($capability) {
            @{ name = $capability.Name; state = "$($capability.State)" }
        } else { $null }
        service    = if ($service) {
            @{
                status    = "$($service.Status)"
                startType = "$($service.StartType)"
                startedAt = if ($startedAt) { $startedAt.ToUniversalTime().ToString('o') } else { $null }
            }
        } else { $null }
        firewall   = if ($rule) {
            $filter = $rule | Get-NetFirewallPortFilter
            @{
                displayName = $rule.DisplayName
                enabled     = "$($rule.Enabled)"
                profiles    = "$($rule.Profile)"
                localPort   = "$($filter.LocalPort)"
            }
        } else { $null }
        networks   = @(Get-NetConnectionProfile | ForEach-Object { "$($_.NetworkCategory)" } |
                Sort-Object -Unique)
        addresses  = @(Get-NetIPAddress -AddressFamily IPv4 |
                Where-Object { $_.IPAddress -notlike '127.*' -and $_.IPAddress -notlike '169.254.*' } |
                ForEach-Object { $_.IPAddress })
    }
}

function Invoke-Apply {
    param([Parameter(Mandatory)]$Request)

    $done = [System.Collections.Generic.List[string]]::new()
    $restartNeeded = $false

    if ($Request.installCapability) {
        Write-Progress-Line "installing $($Request.installCapability) - this pulls the payload from Windows Update and can take several minutes"
        $result = Add-WindowsCapability -Online -Name $Request.installCapability
        $restartNeeded = [bool]$result.RestartNeeded
        $done.Add("installed $($Request.installCapability)")
    }

    if ($Request.serviceAutomatic) {
        Write-Progress-Line 'setting sshd to start automatically'
        Set-Service -Name sshd -StartupType Automatic
        $done.Add('sshd start type: Automatic')
    }
    if ($Request.startService) {
        Write-Progress-Line 'starting sshd'
        Start-Service -Name sshd
        $done.Add('sshd started')
    }
    if ($Request.restartService) {
        Restart-Service -Name sshd
        $done.Add('sshd restarted')
    }

    if ($Request.firewall) {
        Write-Progress-Line 'applying the firewall rule'
        $wanted = $Request.firewall
        if (Get-NetFirewallRule -Name $wanted.name -ErrorAction SilentlyContinue) {
            Set-NetFirewallRule -Name $wanted.name -Enabled True `
                -Profile $wanted.profile -LocalPort $wanted.port | Out-Null
            $done.Add("firewall rule updated: $($wanted.name)")
        } else {
            # -Name is the stable identifier and -DisplayName the label; netsh can only address the latter, which is why this is a cmdlet.
            New-NetFirewallRule -Name $wanted.name -DisplayName $wanted.displayName `
                -Direction Inbound -Protocol TCP -Action Allow `
                -LocalPort $wanted.port -Profile $wanted.profile -Enabled True | Out-Null
            $done.Add("firewall rule created: $($wanted.name)")
        }
    }

    [pscustomobject]@{ restartNeeded = $restartNeeded; done = @($done) }
}

$result = if ($Query) {
    Get-State
} else {
    Invoke-Apply -Request (Get-Content -Raw -LiteralPath $Apply | ConvertFrom-Json)
}

$result | ConvertTo-Json -Depth 5 -Compress
