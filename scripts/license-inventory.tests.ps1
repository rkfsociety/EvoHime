[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$manifestPath = Join-Path $repo 'docs\licenses\manifest.json'
$cargoLock = Join-Path $repo 'Cargo.lock'
$npmLock = Join-Path $repo 'desktop\evohime-electron\package-lock.json'
$nodeModules = Join-Path $repo 'desktop\evohime-electron\node_modules'

foreach ($path in @($manifestPath, $cargoLock, $npmLock)) {
    if (-not (Test-Path -LiteralPath $path)) { throw "License inventory input missing: $path" }
}

$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
$cargoHash = (Get-FileHash -LiteralPath $cargoLock -Algorithm SHA256).Hash
$npmHash = (Get-FileHash -LiteralPath $npmLock -Algorithm SHA256).Hash
if ($manifest.generated_from.cargo_lock_sha256 -ne $cargoHash) { throw 'Cargo.lock changed without license manifest refresh.' }
if ($manifest.generated_from.npm_lock_sha256 -ne $npmHash) { throw 'package-lock.json changed without license manifest refresh.' }

Push-Location $repo
try {
    $metadata = cargo metadata --locked --format-version 1 | ConvertFrom-Json
    foreach ($package in $metadata.packages) {
        if ($null -ne $package.source -and [string]::IsNullOrWhiteSpace([string]$package.license)) {
            throw "Registry crate has no license metadata: $($package.name)@$($package.version)"
        }
    }
} finally { Pop-Location }

$lock = Get-Content -LiteralPath $npmLock -Raw | ConvertFrom-Json -AsHashtable
if (-not (Test-Path -LiteralPath $nodeModules)) { throw 'Electron node_modules is required for npm license metadata verification.' }
$checked = 0
foreach ($packagePath in $lock['packages'].Keys) {
    if ($packagePath -notlike 'node_modules/*') { continue }
    $lockedPackage = $lock['packages'][$packagePath]
    $packageJson = Join-Path $nodeModules ($packagePath.Substring('node_modules/'.Length))
    $packageJson = Join-Path $packageJson 'package.json'
    if (-not (Test-Path -LiteralPath $packageJson)) {
        if ([string]::IsNullOrWhiteSpace([string]$lockedPackage['license'])) {
            throw "Uninstalled npm package has no locked license metadata: $packagePath"
        }
        $checked++
        continue
    }
    $package = Get-Content -LiteralPath $packageJson -Raw | ConvertFrom-Json
    if ([string]::IsNullOrWhiteSpace([string]$package.license) -and $null -eq $package.licenses) {
        throw "npm package has no license metadata: $($package.name)@$($package.version)"
    }
    $checked++
}
if ($checked -eq 0) { throw 'No npm packages were checked.' }
Write-Output "license inventory gate: PASS ($checked npm packages; Cargo registry metadata verified)"
