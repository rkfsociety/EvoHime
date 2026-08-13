[CmdletBinding()]
param(
    [string]$OutputPath = (Join-Path $PSScriptRoot '..\artifacts\native\windows-x64'),
    [ValidateSet('Debug', 'Release')]
    [string]$Configuration = 'Release',
    [string]$Version,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'native-package.ps1')

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$desktopProjectPath = Join-Path $repoRoot 'desktop\EvoHime.Desktop\EvoHime.Desktop.csproj'
[xml]$desktopProject = Get-Content -LiteralPath $desktopProjectPath -Raw
$projectVersion = [string]($desktopProject.Project.PropertyGroup |
    Where-Object { $_.Version } |
    Select-Object -First 1).Version
if ([string]::IsNullOrWhiteSpace($projectVersion)) {
    throw "Версия не найдена в проекте WinUI: $desktopProjectPath"
}
if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = $projectVersion
}
if ($Version -notmatch '^\d+\.\d+\.\d+$') {
    throw "Некорректная версия native-пакета: $Version"
}

$outputCandidate = if ([System.IO.Path]::IsPathRooted($OutputPath)) {
    $OutputPath
} else {
    Join-Path $repoRoot $OutputPath
}
$resolvedOutput = [System.IO.Path]::GetFullPath($outputCandidate)
$manifest = New-NativePackageManifest -Architecture 'x64' -OsMinimum 'Windows 10 2004 / Windows 11'
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
            '-c', $Configuration, '-r', 'win-x64', '--self-contained', 'true',
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
    $source = if ($SkipBuild) {
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

$uiStaged = Join-Path $resolvedOutput 'ui'
$uiPackaged = Join-Path $resolvedOutput 'EvoHime.exe'
$appPriName = 'EvoHime.Desktop.pri'
if (Test-Path -LiteralPath $uiStaged) {
    foreach ($item in Get-ChildItem -LiteralPath $uiStaged -Force) {
        $name = if ($item.Name -eq 'EvoHime.Desktop.exe') {
            'EvoHime.exe'
        } elseif ($item.Name -in @('EvoHime.Desktop.deps.json', 'EvoHime.Desktop.runtimeconfig.json')) {
            'EvoHime' + $item.Name.Substring('EvoHime.Desktop'.Length)
        } else {
            $item.Name
        }
        $destination = Join-Path $resolvedOutput $name
        Copy-Item -LiteralPath $item.FullName -Destination $destination -Force -Recurse
    }
}
if (-not (Test-Path -LiteralPath (Join-Path $resolvedOutput $appPriName)) -and
    -not (Test-Path -LiteralPath (Join-Path $uiStaged $appPriName))) {
    $appPri = Get-ChildItem -LiteralPath (Join-Path $repoRoot 'desktop\EvoHime.Desktop\bin') -Filter $appPriName -File -Recurse |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if ($appPri -eq $null) {
        throw "Ресурсный PRI-файл WinUI не найден: $appPriName"
    }
    Copy-Item -LiteralPath $appPri.FullName -Destination (Join-Path $uiStaged $appPriName) -Force
    Copy-Item -LiteralPath $appPri.FullName -Destination (Join-Path $resolvedOutput $appPriName) -Force
}
if (-not (Test-Path -LiteralPath $uiPackaged)) {
    throw "Native-компонент не найден: $uiPackaged"
}
if (-not (Test-Path -LiteralPath (Join-Path $resolvedOutput $appPriName))) {
    throw "Native-компонент не найден: $(Join-Path $resolvedOutput $appPriName)"
}
if (Test-Path -LiteralPath (Join-Path $resolvedOutput 'ui')) {
    Remove-Item -LiteralPath (Join-Path $resolvedOutput 'ui') -Recurse -Force
}
Write-NativePackageManifest -OutputPath (Join-Path $resolvedOutput 'evohime.manifest.json') -Manifest $manifest
Write-Output "Native package: $resolvedOutput"
