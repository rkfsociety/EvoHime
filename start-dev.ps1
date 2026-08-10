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
    # Запускаем сборщик в текущем PowerShell-процессе. Вложенный pwsh ломает
    # запуск из некоторых оболочек Windows и не даёт полезной диагностики.
    & $buildScript -OutputPath $packagePath -Configuration Debug
    if ($LASTEXITCODE -ne 0) {
        throw "Native-сборка завершилась с кодом $LASTEXITCODE"
    }
}

$uiPath = Join-Path $packagePath 'EvoHime.exe'
$corePath = Join-Path $packagePath 'evohime-core.exe'
$supervisorPath = Join-Path $packagePath 'evohime-supervisor.exe'
foreach ($path in @($uiPath, $corePath, $supervisorPath)) {
    if (-not (Test-Path -LiteralPath $path)) {
        throw "Native-компонент не найден: $path"
    }
}
New-Item -ItemType Directory -Force -Path $dataPath | Out-Null

$previousCoreExe = $env:EVOHIME_CORE_EXE
$previousDataDir = $env:EVOHIME_DATA_DIR
$env:EVOHIME_CORE_EXE = $corePath
$env:EVOHIME_DATA_DIR = $dataPath
$ui = $null

try {
    # EvoHime.exe сам запускает supervisor, а supervisor — Core. Это сохраняет
    # один владелец жизненного цикла и не создаёт второй supervisor из launcher.
    $ui = Start-Process -FilePath $uiPath -WorkingDirectory $packagePath -PassThru
    $ui.WaitForExit()
}
finally {
    if ($ui -and -not $ui.HasExited) {
        $ui.Kill()
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
