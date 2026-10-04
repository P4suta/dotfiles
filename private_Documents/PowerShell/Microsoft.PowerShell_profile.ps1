$herdrBin = "$env:LOCALAPPDATA\Programs\Herdr\bin"
if ($env:Path -notlike "*$herdrBin*") {
    $env:Path = "$herdrBin;$env:Path"
}

if (-not (Get-Module PSReadLine)) {
    $env:Path = "$env:Path;$env:LOCALAPPDATA\mise\shims"
    return
}

foreach ($init in 'starship', 'mise', 'zoxide') {
    $script = Join-Path $HOME ".cache\pwsh\$init.ps1"
    if (Test-Path -LiteralPath $script) {
        . $script
    }
}

Set-PSReadLineOption -EditMode Emacs
if (-not [Console]::IsOutputRedirected) {
    Set-PSReadLineOption -PredictionSource History -PredictionViewStyle ListView
}
Set-PSReadLineKeyHandler -Key UpArrow -Function HistorySearchBackward
Set-PSReadLineKeyHandler -Key DownArrow -Function HistorySearchForward
Set-PSReadLineKeyHandler -Chord Ctrl+r -ScriptBlock {
    Import-Module PSFzf
    Set-PsFzfOption -PSReadlineChordReverseHistory Ctrl+r
    Invoke-FzfPsReadlineHandlerHistory
}

if (Get-Command Enable-TransientPrompt -ErrorAction Ignore) {
    function global:Invoke-Starship-TransientFunction {
        &starship module character
    }
    Enable-TransientPrompt
}
