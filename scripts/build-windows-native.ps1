[CmdletBinding()]
param(
    [string]$OutputPath = (Join-Path $PSScriptRoot '..\artifacts\native\windows-x64'),
    [ValidateSet('Debug', 'Release')]
    [string]$Configuration = 'Release',
    [string]$Version,
    [string]$Commit,
    [string]$Branch = 'main',
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'

# native-package.ps1 использует `Set-Content -Encoding utf8NoBOM`, которого нет
# в Windows PowerShell 5.1. Без явной проверки запуск падает на разборе скрипта,
# и настоящая причина в сообщении не видна.
if ($PSVersionTable.PSVersion.Major -lt 7) {
    throw 'Нужен PowerShell 7 или новее. Run: pwsh -File .\scripts\build-windows-native.ps1'
}

. (Join-Path $PSScriptRoot 'native-package.ps1')

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$electronRoot = Join-Path $repoRoot 'desktop\evohime-electron'
$outputCandidate = if ([System.IO.Path]::IsPathRooted($OutputPath)) { $OutputPath } else { Join-Path $repoRoot $OutputPath }
$resolvedOutput = [System.IO.Path]::GetFullPath($outputCandidate)
$manifest = New-NativePackageManifest -Architecture 'x64' -OsMinimum 'Windows 10 2004 / Windows 11'
$cargoProfile = if ($Configuration -eq 'Debug') { 'debug' } else { 'release' }
$cargoArguments = @('build', '--locked')
if ($Configuration -eq 'Release') { $cargoArguments += '--release' }
$cargoArguments += @('-p', 'evohime-core', '-p', 'evohime-supervisor', '-p', 'evohime-updater')
$electronBuilderArguments = @('electron-builder', '--dir', '--config', 'electron-builder.yml')
if ($Version) {
    if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "Некорректная версия Electron package: $Version" }
    $electronBuilderArguments += @('--config.extraMetadata.version', $Version)
}

if (-not $SkipBuild) {
    Push-Location $repoRoot
    try {
        Invoke-NativeCommand -Executable 'cargo' -Arguments $cargoArguments
        Push-Location $electronRoot
        try {
            Invoke-NativeCommand -Executable 'npm' -Arguments @('ci', '--ignore-scripts')
            Invoke-NativeCommand -Executable 'node' -Arguments @('scripts/postinstall-allowlist.mjs')
            Invoke-NativeCommand -Executable 'npm' -Arguments @('run', 'build')
            Invoke-NativeCommand -Executable 'npx' -Arguments $electronBuilderArguments
        }
        finally { Pop-Location }
    }
    finally { Pop-Location }
}

New-Item -ItemType Directory -Force -Path $resolvedOutput | Out-Null
$cargoTarget = Join-Path $repoRoot "target\$cargoProfile"
$requiredNative = @('evohime-core.exe', 'evohime-supervisor.exe', 'evohime-transaction.exe')
foreach ($component in $requiredNative) {
    $destination = Join-Path $resolvedOutput $component
    $source = if ($SkipBuild) { $destination } else { Join-Path $cargoTarget $component }
    if (-not (Test-Path -LiteralPath $source)) { throw "Native-компонент не найден: $source" }
    if ($source -ne $destination) { Copy-Item -LiteralPath $source -Destination $destination -Force }
}

$electronPayload = Join-Path $electronRoot 'release\win-unpacked'
$uiPackaged = Join-Path $resolvedOutput 'EvoHime.exe'
if (-not $SkipBuild) {
    if (-not (Test-Path -LiteralPath $electronPayload)) { throw "Electron package не найден: $electronPayload" }
    foreach ($item in Get-ChildItem -LiteralPath $electronPayload -Force) {
        $destination = Join-Path $resolvedOutput $item.Name
        # Copy-Item -Recurse вкладывает каталог внутрь уже существующего вместо
        # замены, поэтому повторная сборка оставляла прежний app.asar и создавала
        # resources\resources. Каталог сносится целиком перед копированием.
        if ($item.PSIsContainer -and (Test-Path -LiteralPath $destination)) {
            Remove-Item -LiteralPath $destination -Recurse -Force
        }
        Copy-Item -LiteralPath $item.FullName -Destination $destination -Force -Recurse
    }
}
if (-not (Test-Path -LiteralPath $uiPackaged)) { throw "Electron UI не найден: $uiPackaged" }

Write-NativePackageManifest -OutputPath (Join-Path $resolvedOutput 'evohime.manifest.json') -Manifest $manifest

# Маркер сборки: установленный клиент по нему знает свой коммит и понимает,
# отстал ли он от отслеживаемой ветки. Без маркера версия считается неизвестной
# и клиент пересобирает себя при первом запуске.
Write-NativeBuildMarker -OutputPath (Join-Path $resolvedOutput 'evohime.build.json') -RepositoryRoot $repoRoot -Commit $Commit -Branch $Branch
Write-Output "Electron native package: $resolvedOutput"
