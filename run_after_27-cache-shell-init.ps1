$ErrorActionPreference = 'Stop'

& "$env:USERPROFILE/.local/bin/dotctl.exe" setup shell
exit $LASTEXITCODE
