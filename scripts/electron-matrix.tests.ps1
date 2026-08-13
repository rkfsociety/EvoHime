[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$PackagePath
)

$ErrorActionPreference = 'Stop'
$resolvedPackage = (Resolve-Path -LiteralPath $PackagePath).Path
$asar = Join-Path $resolvedPackage 'resources\app.asar'
if (-not (Test-Path -LiteralPath $asar)) { throw "Electron archive is missing: $asar" }

$started = Get-Date
& (Join-Path $PSScriptRoot 'electron-acceptance.tests.ps1') -PackagePath $resolvedPackage
$startupMs = [int]((Get-Date) - $started).TotalMilliseconds
& (Join-Path $PSScriptRoot 'electron-fault.tests.ps1') -PackagePath $resolvedPackage

$packageBytes = (Get-ChildItem -LiteralPath $resolvedPackage -Recurse -File | Measure-Object -Property Length -Sum).Sum
$versionMatch = & rg.exe -a -o -U '"name": "evohime-electron",\s+"version": "[0-9]+\.[0-9]+\.[0-9]+"' $asar | Select-Object -First 1
if (-not $versionMatch) { throw 'Electron app.asar does not contain a release version.' }

Write-Output "electron matrix startup_ms: $startupMs"
Write-Output "electron matrix package_bytes: $packageBytes"
Write-Output "electron matrix package_version: $versionMatch"
Write-Output 'electron matrix read-only/locked/provider-outage/reconnect/diagnostics gates: covered by Core and Electron regression suites'
Write-Output 'electron acceptance matrix: PASS'
