[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$PackagePath,
    [int]$StartupTimeoutSeconds = 10
)

$ErrorActionPreference = 'Stop'
$resolvedPackage = (Resolve-Path -LiteralPath $PackagePath).Path
$required = @(
    'EvoHime.exe',
    'resources\app.asar',
    'resources\evohime-agent.ico',
    'evohime-core.exe',
    'evohime-supervisor.exe',
    'evohime-transaction.exe',
    'evohime.manifest.json'
)
foreach ($component in $required) {
    if (-not (Test-Path -LiteralPath (Join-Path $resolvedPackage $component))) {
        throw "Electron package component is missing: $component"
    }
}

$dataPath = Join-Path $resolvedPackage ('.acceptance-data-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $dataPath | Out-Null
$previousDataDir = $env:EVOHIME_DATA_DIR
$env:EVOHIME_DATA_DIR = $dataPath
$shell = $null
function Get-PackageProcesses {
    Get-CimInstance Win32_Process | Where-Object {
        $_.ExecutablePath -and $_.ExecutablePath.StartsWith($resolvedPackage, [System.StringComparison]::OrdinalIgnoreCase)
    }
}
try {
    $shell = Start-Process -FilePath (Join-Path $resolvedPackage 'EvoHime.exe') `
        -WorkingDirectory $resolvedPackage -PassThru
    $deadline = (Get-Date).AddSeconds($StartupTimeoutSeconds)
    while (-not $shell.HasExited -and (Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 250
        $shell.Refresh()
    }
    if ($shell.HasExited) {
        throw "EvoHime.exe exited during package startup with code $($shell.ExitCode)."
    }
    Write-Output "electron package startup: PASS (pid $($shell.Id))"
}
finally {
    if ($shell -and -not $shell.HasExited) {
        Stop-Process -Id $shell.Id -Force -ErrorAction SilentlyContinue
    }
    Start-Sleep -Milliseconds 500
    for ($attempt = 0; $attempt -lt 4; $attempt++) {
        $packageProcesses = @(Get-PackageProcesses | Where-Object { $_.ProcessId -ne $PID })
        foreach ($process in $packageProcesses) {
            Stop-Process -Id $process.ProcessId -Force -ErrorAction SilentlyContinue
        }
        if ($packageProcesses.Count -eq 0) { break }
        Start-Sleep -Milliseconds 250
    }
    if ($null -eq $previousDataDir) {
        Remove-Item Env:EVOHIME_DATA_DIR -ErrorAction SilentlyContinue
    } else {
        $env:EVOHIME_DATA_DIR = $previousDataDir
    }
}

Write-Output 'electron package acceptance: PASS'
