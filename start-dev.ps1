[CmdletBinding()]
param(
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path $PSScriptRoot).Path
$packagePath = Join-Path $root '.evohime-native\windows-x64'
$buildScript = Join-Path $root 'scripts\build-windows-native.ps1'
$dataPath = Join-Path $root '.evohime-native\data'

if (-not $SkipBuild) {
    & (Join-Path $PSHOME 'pwsh.exe') -NoProfile -File $buildScript `
        -OutputPath $packagePath -Configuration Debug
    if ($LASTEXITCODE -ne 0) {
        throw "Native-сборка завершилась с кодом $LASTEXITCODE"
    }
}

$supervisorPath = Join-Path $packagePath 'evohime-supervisor.exe'
$uiPath = Join-Path $packagePath 'EvoHime.Desktop.exe'
foreach ($path in @($supervisorPath, $uiPath)) {
    if (-not (Test-Path -LiteralPath $path)) {
        throw "Native-компонент не найден: $path"
    }
}
New-Item -ItemType Directory -Force -Path $dataPath | Out-Null

$previousCoreExe = $env:EVOHIME_CORE_EXE
$previousDataDir = $env:EVOHIME_DATA_DIR
$env:EVOHIME_CORE_EXE = Join-Path $packagePath 'evohime-core.exe'
$env:EVOHIME_DATA_DIR = $dataPath
$supervisor = $null
$ui = $null

try {
    $supervisor = Start-Process -FilePath $supervisorPath -WorkingDirectory $packagePath -WindowStyle Hidden -PassThru
    $ui = Start-Process -FilePath $uiPath -WorkingDirectory $packagePath -PassThru
    $ui.WaitForExit()
}
finally {
    if ($ui -and -not $ui.HasExited) {
        $ui.Kill()
    }
    if ($supervisor -and -not $supervisor.HasExited) {
        $supervisor.Kill()
        $supervisor.WaitForExit()
    }
    if ($null -eq $previousCoreExe) {
        Remove-Item Env:EVOHIME_CORE_EXE -ErrorAction SilentlyContinue
    } else {
        $env:EVOHIME_CORE_EXE = $previousCoreExe
    }
    if ($null -eq $previousDataDir) {
        Remove-Item Env:EVOHIME_DATA_DIR -ErrorAction SilentlyContinue
    } else {
        $env:EVOHIME_DATA_DIR = $previousDataDir
    }
}
