[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string]$Repository,
    [string]$Tag = 'installer'
)

$ErrorActionPreference = 'Stop'
if (-not $env:GH_TOKEN) { throw 'GH_TOKEN is required.' }

gh release view $Tag --repo $Repository 2>$null | Out-Null
if ($LASTEXITCODE -ne 0) { throw "Не найден installer release $Tag." }

gh release edit $Tag --repo $Repository --latest
if ($LASTEXITCODE -ne 0) { throw "Не удалось отметить installer release $Tag как Latest." }

Write-Host "Marked $Tag as the repository Latest release."
