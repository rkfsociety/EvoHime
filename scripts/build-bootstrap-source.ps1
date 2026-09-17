[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string]$UpdaterArchivePath,
    [Parameter(Mandatory)] [string]$TransactionPath,
    [Parameter(Mandatory)] [string]$OutputPath,
    [Parameter(Mandatory)] [ValidatePattern('^\d+\.\d+\.\d+$')] [string]$UpdaterVersion,
    [Parameter(Mandatory)] [ValidatePattern('^\d+\.\d+\.\d+$')] [string]$TransactionVersion,
    [string]$Commit,
    [string]$Branch = 'main'
)

$ErrorActionPreference = 'Stop'
if ($PSVersionTable.PSVersion.Major -lt 7) {
    throw 'Нужен PowerShell 7 или новее. Run: pwsh -File .\scripts\build-bootstrap-source.ps1'
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$output = [System.IO.Path]::GetFullPath($OutputPath)
if ($output -eq $repoRoot) { throw 'Bootstrap source нельзя собрать в корень репозитория.' }
$archive = [System.IO.Path]::GetFullPath($UpdaterArchivePath)
$transaction = [System.IO.Path]::GetFullPath($TransactionPath)
foreach ($path in @($archive, $transaction)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Bootstrap input missing: $path" }
}

if (Test-Path -LiteralPath $output) { Remove-Item -LiteralPath $output -Recurse -Force }
New-Item -ItemType Directory -Force -Path $output | Out-Null
$extract = Join-Path ([System.IO.Path]::GetTempPath()) "evohime-updater-$([guid]::NewGuid().ToString('N'))"
try {
    Expand-Archive -LiteralPath $archive -DestinationPath $extract -Force
    $worker = Join-Path $extract 'evohime-updater.exe'
    $updaterUi = Join-Path $extract 'updater'
    if (-not (Test-Path -LiteralPath $worker -PathType Leaf)) { throw 'Updater archive misses evohime-updater.exe.' }
    foreach ($required in @('EvoHimeUpdater.exe', 'resources\app.asar')) {
        if (-not (Test-Path -LiteralPath (Join-Path $updaterUi $required) -PathType Leaf)) {
            throw "Updater archive misses updater\$required."
        }
    }
    Copy-Item -LiteralPath $worker -Destination (Join-Path $output 'evohime-updater.exe') -Force
    Copy-Item -LiteralPath $updaterUi -Destination (Join-Path $output 'updater') -Recurse -Force
    Copy-Item -LiteralPath $transaction -Destination (Join-Path $output 'evohime-transaction.exe') -Force

    $marker = [ordered]@{
        schema = 'evohime.bootstrap.v1'
        product = 'EvoHime'
        updaterVersion = $UpdaterVersion
        transactionVersion = $TransactionVersion
        commit = if ($Commit) { $Commit } else { (& git -C $repoRoot rev-parse HEAD).Trim() }
        branch = $Branch
        requiredFiles = @('evohime-updater.exe', 'evohime-transaction.exe', 'updater\EvoHimeUpdater.exe', 'updater\resources\app.asar')
        nextStep = 'download-compatible-modules'
    }
    $marker | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $output 'evohime.bootstrap.json') -Encoding utf8NoBOM
    Write-Output "EvoHime bootstrap source: $output"
}
finally {
    if (Test-Path -LiteralPath $extract) { Remove-Item -LiteralPath $extract -Recurse -Force }
}
