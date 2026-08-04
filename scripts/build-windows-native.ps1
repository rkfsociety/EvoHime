[CmdletBinding()]
param(
    [string]$OutputPath = (Join-Path $PSScriptRoot '..\artifacts\native\windows-x64'),
    [ValidateSet('Debug', 'Release')]
    [string]$Configuration = 'Release',
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$Version = '0.0.0001',
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'native-package.ps1')

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$outputCandidate = if ([System.IO.Path]::IsPathRooted($OutputPath)) {
    $OutputPath
} else {
    Join-Path $repoRoot $OutputPath
}
$resolvedOutput = [System.IO.Path]::GetFullPath($outputCandidate)
$manifest = New-NativePackageManifest -Architecture 'x64' -OsMinimum 'Windows 11 22H2'
$assemblyVersion = "$Version.0"

if (-not $SkipBuild) {
    $dotnet = Get-DotNetExecutable
    Push-Location $repoRoot
    try {
        Invoke-NativeCommand -Executable 'cargo' -Arguments @(
            'build', '--locked', '--release', '-p', 'evohime-core', '-p', 'evohime-supervisor', '-p', 'evohime-updater'
        )
        Invoke-NativeCommand -Executable $dotnet -Arguments @(
            'publish', 'desktop\EvoHime.Desktop\EvoHime.Desktop.csproj',
            '-c', $Configuration, '-r', 'win-x64', '--self-contained', 'false',
            '-p:Platform=x64', '-p:WindowsPackageType=None', '-p:EnableMsixTooling=false',
            "-p:Version=$Version", "-p:InformationalVersion=$Version",
            "-p:AssemblyVersion=$assemblyVersion", "-p:FileVersion=$assemblyVersion",
            '-o', (Join-Path $resolvedOutput 'ui')
        )
    }
    finally {
        Pop-Location
    }
}

New-Item -ItemType Directory -Force -Path $resolvedOutput | Out-Null
$cargoTarget = Join-Path $repoRoot 'target\release'
$required = @(
    'evohime-core.exe',
    'evohime-supervisor.exe',
    'evohime-transaction.exe'
)

foreach ($component in $required) {
    $destination = Join-Path $resolvedOutput $component
    $source = if (Test-Path -LiteralPath $destination) {
        $destination
    } else {
        Join-Path $cargoTarget $component
    }
    if (-not (Test-Path -LiteralPath $source)) {
        throw "Native-компонент не найден: $source"
    }
    if ($source -ne $destination) {
        Copy-Item -LiteralPath $source -Destination $destination -Force
    }
}

$uiStaged = Join-Path $resolvedOutput 'ui\EvoHime.Desktop.exe'
$uiPackaged = Join-Path $resolvedOutput 'EvoHime.exe'
if (Test-Path -LiteralPath $uiStaged) {
    Copy-Item -LiteralPath $uiStaged -Destination $uiPackaged -Force
}
if (-not (Test-Path -LiteralPath $uiPackaged)) {
    throw "Native-компонент не найден: $uiPackaged"
}
if (Test-Path -LiteralPath (Join-Path $resolvedOutput 'ui')) {
    Remove-Item -LiteralPath (Join-Path $resolvedOutput 'ui') -Recurse -Force
}
Write-NativePackageManifest -OutputPath (Join-Path $resolvedOutput 'evohime.manifest.json') -Manifest $manifest
Write-Output "Native package: $resolvedOutput"
