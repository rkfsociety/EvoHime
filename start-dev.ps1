[CmdletBinding()]
param(
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'

# Сборка опирается на возможности PowerShell 7. В 5.1 запуск обрывается
# ошибкой разбора вложенного скрипта, поэтому версию проверяем сразу.
if ($PSVersionTable.PSVersion.Major -lt 7) {
    # Команда продублирована латиницей: консоль Windows часто выводит кириллицу
    # мусором, и подсказка о запуске должна читаться в любой кодовой странице.
    throw 'Нужен PowerShell 7 или новее. Run: pwsh -File .\start-dev.ps1'
}

$root = (Resolve-Path $PSScriptRoot).Path
$packagePath = Join-Path $root '.evohime-native\windows-x64'
$buildScript = Join-Path $root 'scripts\build-windows-native.ps1'
$dataPath = Join-Path $root '.evohime-native\data'

function Import-ModelEnvironment {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        return
    }

    $allowedKeys = @(
        'MODEL_PROVIDER',
        'MODEL_DEFAULT_ROUTE',
        'MODEL_ROUTES_JSON',
        'LITEROUTER_API_KEY',
        'LITEROUTER_BASE_URL',
        'LITEROUTER_MODEL',
        'OPENAI_API_KEY',
        'OPENAI_BASE_URL',
        'OPENAI_MODEL'
    )
    foreach ($line in Get-Content -LiteralPath $Path) {
        if ($line -match '^\s*([A-Za-z0-9_]+)\s*=\s*(.*)\s*$' -and $allowedKeys -contains $Matches[1]) {
            $value = $Matches[2].Trim()
            if (($value.StartsWith('"') -and $value.EndsWith('"')) -or
                ($value.StartsWith("'") -and $value.EndsWith("'"))) {
                $value = $value.Substring(1, $value.Length - 2)
            }
            [Environment]::SetEnvironmentVariable($Matches[1], $value, 'Process')
        }
    }
}

# Локальные ключи и выбранная модель передаются только дочерним native-процессам;
# сам .env не попадает в репозиторий и не копируется в пакет.
Import-ModelEnvironment (Join-Path $root '.env')

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
$previousUpdateEnabled = $env:EVOHIME_UPDATE_ENABLED
$env:EVOHIME_CORE_EXE = $corePath
$env:EVOHIME_DATA_DIR = $dataPath
$env:EVOHIME_UPDATE_ENABLED = '0'
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
    if ($null -eq $previousUpdateEnabled) {
        Remove-Item Env:EVOHIME_UPDATE_ENABLED -ErrorAction SilentlyContinue
    } else {
        $env:EVOHIME_UPDATE_ENABLED = $previousUpdateEnabled
    }
}
