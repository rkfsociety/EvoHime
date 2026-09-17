[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string]$SourcePath,
    [string]$SetupPath
)

$ErrorActionPreference = 'Stop'
$source = (Resolve-Path $SourcePath).Path
$allowed = @('evohime-updater.exe', 'evohime.bootstrap.json', 'updater')
$topLevel = @(Get-ChildItem -LiteralPath $source -Force | ForEach-Object { $_.Name })
foreach ($item in $topLevel) {
    if ($item -notin $allowed) { throw "Bootstrap source contains unexpected top-level item: $item" }
}
foreach ($required in @('evohime-updater.exe', 'evohime.bootstrap.json', 'updater\EvoHimeUpdater.exe', 'updater\resources\app.asar')) {
    if (-not (Test-Path -LiteralPath (Join-Path $source $required) -PathType Leaf)) { throw "Bootstrap source misses: $required" }
}
foreach ($forbidden in @('EvoHime.exe', 'resources\app.asar', 'evohime-core.exe', 'evohime-supervisor.exe', 'evohime-listener.exe', 'evohime.manifest.json', 'evohime.components.json')) {
    if (Test-Path -LiteralPath (Join-Path $source $forbidden)) { throw "Bootstrap source contains full-install component: $forbidden" }
}
$marker = Get-Content -LiteralPath (Join-Path $source 'evohime.bootstrap.json') -Raw | ConvertFrom-Json
if ($marker.schema -ne 'evohime.bootstrap.v1') { throw 'Bootstrap marker schema is invalid.' }
if (@($marker.requiredFiles).Count -ne 3) { throw 'Bootstrap marker requiredFiles is incomplete.' }
if ($SetupPath) {
    $setup = (Resolve-Path $SetupPath).Path
    if (-not (Test-Path -LiteralPath $setup -PathType Leaf)) { throw "Bootstrap setup missing: $setup" }
    if ((Get-Item -LiteralPath $setup).Length -le 0) { throw 'Bootstrap setup is empty.' }
}
Write-Host 'bootstrap installer smoke tests passed.'
