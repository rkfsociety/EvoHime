[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$PackagePath,
    [int]$TimeoutSeconds = 30
)

$ErrorActionPreference = 'Stop'
$resolvedPackage = (Resolve-Path -LiteralPath $PackagePath).Path
$shellPath = Join-Path $resolvedPackage 'EvoHime.exe'
$corePath = Join-Path $resolvedPackage 'evohime-core.exe'
$supervisorPath = Join-Path $resolvedPackage 'evohime-supervisor.exe'
foreach ($path in @($shellPath, $corePath, $supervisorPath)) {
    if (-not (Test-Path -LiteralPath $path)) { throw "Fault smoke component is missing: $path" }
}

function Get-PackageProcesses([string]$executablePath) {
    Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $executablePath }
}

function Wait-For([scriptblock]$condition, [int]$seconds, [string]$failure) {
    $deadline = (Get-Date).AddSeconds($seconds)
    while ((Get-Date) -lt $deadline) {
        if (& $condition) { return }
        Start-Sleep -Milliseconds 250
    }
    throw $failure
}

$dataPath = Join-Path $resolvedPackage ('.fault-data-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $dataPath | Out-Null
$previousDataDir = $env:EVOHIME_DATA_DIR
$previousUpdateEnabled = $env:EVOHIME_UPDATE_ENABLED
$env:EVOHIME_DATA_DIR = $dataPath
# This smoke test exercises Core/supervisor recovery, not the source updater.
# Keep the lifecycle assertion independent from network latency and update-gate
# work that runs before the supervisor is started.
$env:EVOHIME_UPDATE_ENABLED = '0'
$shell = $null
try {
    $shell = Start-Process -FilePath $shellPath -WorkingDirectory $resolvedPackage -PassThru
    Wait-For { $null -ne (Get-PackageProcesses $corePath) } $TimeoutSeconds 'Core did not start.'
    $firstCore = (Get-PackageProcesses $corePath | Select-Object -First 1).ProcessId

    Stop-Process -Id $firstCore -Force
    Wait-For {
        $current = Get-PackageProcesses $corePath
        $null -ne $current -and ($current | Where-Object { $_.ProcessId -ne $firstCore })
    } $TimeoutSeconds 'Supervisor did not restart Core after a forced Core exit.'
    Write-Output 'fault smoke Core restart: PASS'

    $supervisor = Get-PackageProcesses $supervisorPath | Select-Object -First 1
    if ($null -eq $supervisor) { throw 'Supervisor did not remain alive after Core restart.' }
    Stop-Process -Id $supervisor.ProcessId -Force
    Wait-For { $null -eq (Get-PackageProcesses $corePath) } $TimeoutSeconds 'Core remained alive after supervisor termination.'
    Write-Output 'fault smoke supervisor ownership: PASS'
}
finally {
    if ($shell -and -not $shell.HasExited) { Stop-Process -Id $shell.Id -Force -ErrorAction SilentlyContinue }
    for ($attempt = 0; $attempt -lt 4; $attempt++) {
        $remaining = @(Get-PackageProcesses $shellPath) + @(Get-PackageProcesses $corePath) + @(Get-PackageProcesses $supervisorPath)
        foreach ($process in $remaining) {
            Stop-Process -Id $process.ProcessId -Force -ErrorAction SilentlyContinue
        }
        if ($remaining.Count -eq 0) { break }
        Start-Sleep -Milliseconds 250
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

Write-Output 'electron fault acceptance: PASS'
