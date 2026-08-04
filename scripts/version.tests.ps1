$ErrorActionPreference = 'Stop'

$installer = Get-Content -Raw (Join-Path $PSScriptRoot '..\installer\EvoHime.iss')
if ($installer -notmatch '#define AppVersion "0\.0\.000030"') {
    throw 'The current Windows client version must be 0.0.000030.'
}

$project = Get-Content -Raw (Join-Path $PSScriptRoot '..\desktop\EvoHime.Desktop\EvoHime.Desktop.csproj')
if ($project -notmatch '<Version>0\.0\.000030</Version>') {
    throw 'The WinUI client project must use version 0.0.000030.'
}

Write-Output 'version smoke: PASS'
