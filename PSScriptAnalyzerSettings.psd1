# PSScriptAnalyzer settings, passed via -Settings by `dotctl lint ps1`.
# Every severity stays enabled; only rules that are false positives in this repo are excluded.

@{
    Severity     = @('Information', 'Warning', 'Error')
    ExcludeRules = @(
        # mise, zoxide, and starship all document `<tool> init pwsh | Out-String | Invoke-Expression` as their init snippet, so Invoke-Expression is unavoidable.
        'PSAvoidUsingInvokeExpression',

        # A BOM is only needed for Windows PowerShell 5.1 compatibility.
        # This repo is pwsh 7+ only, where BOM-less UTF-8 is read correctly and a BOM would only dirty Git diffs.
        'PSUseBOMForUnicodeEncodedFile',

        # Upstream data-flow tracking is weak: variables used inside else blocks or in `if ((Test-Path $x))` get flagged as unused.
        # Adding dummy references costs more than the rule is worth.
        'PSUseDeclaredVarsMoreThanAssignments',

        # Flags `Write-Output 'message'` and demands -InputObject, which is needless verbosity for the single-argument calls that dominate here.
        'PSAvoidUsingPositionalParameters'
    )
}
