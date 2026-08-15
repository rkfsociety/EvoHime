[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string]$Prompt,
    [string]$Workspace = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path,
    [string]$Model,
    [switch]$NoBuild,
    [switch]$ApproveWrites
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$dataPath = Join-Path $root '.evohime-native\data'
$corePath = Join-Path $root 'target\release\evohime-core.exe'

if (Test-Path (Join-Path $root '.env')) {
    $allowedKeys = @(
        'MODEL_PROVIDER', 'MODEL_DEFAULT_ROUTE', 'MODEL_ROUTES_JSON',
        'LITEROUTER_API_KEY', 'LITEROUTER_BASE_URL', 'LITEROUTER_MODEL',
        'OPENAI_API_KEY', 'OPENAI_BASE_URL', 'OPENAI_MODEL'
    )
    foreach ($line in Get-Content (Join-Path $root '.env')) {
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

# WinUI сохраняет выбранный provider/model/key в DPAPI-файле текущего
# пользователя. Читаем его тем же Windows-механизмом, не показывая ключ.
$settingsPath = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'EvoHime\provider-settings.bin'
if (Test-Path -LiteralPath $settingsPath) {
    try {
        Add-Type -AssemblyName System.Security.Cryptography.ProtectedData
        $protectedData = [IO.File]::ReadAllBytes($settingsPath)
        $plainData = [Security.Cryptography.ProtectedData]::Unprotect(
            $protectedData,
            $null,
            [Security.Cryptography.DataProtectionScope]::CurrentUser)
        $settings = [Text.Encoding]::UTF8.GetString($plainData) | ConvertFrom-Json
        if ($settings.Provider) { $env:MODEL_PROVIDER = $settings.Provider }
        if ($settings.BaseUrl) { $env:LITEROUTER_BASE_URL = $settings.BaseUrl }
        if ($settings.Model) { $env:LITEROUTER_MODEL = $settings.Model }
        if ($settings.ApiKey) { $env:LITEROUTER_API_KEY = $settings.ApiKey }
    }
    catch {
        Write-Verbose "Не удалось прочитать provider-settings.bin: $($_.Exception.Message)"
    }
}
if ($Model) {
    $env:LITEROUTER_MODEL = $Model
}

New-Item -ItemType Directory -Force -Path $dataPath | Out-Null
$env:EVOHIME_DATA_DIR = $dataPath
$env:EVOHIME_TASK_TIMEOUT_SECONDS = '300'

Push-Location $root
try {
    if (-not $NoBuild) {
        cargo build --locked --release -p evohime-core
        if ($LASTEXITCODE -ne 0) {
            throw "Сборка evohime-core завершилась с кодом $LASTEXITCODE"
        }
    }
    if (-not (Test-Path -LiteralPath $corePath)) {
        throw "Core не найден: $corePath. Запустите без -NoBuild."
    }
    $coreArgs = @('--console', '--workspace', $Workspace, '--prompt', $Prompt)
    if ($ApproveWrites) {
        $coreArgs += '--approve-writes'
    }
    & $corePath @coreArgs
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
