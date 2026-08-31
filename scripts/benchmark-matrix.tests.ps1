[CmdletBinding()]
param(
    [string]$Suite = (Join-Path $PSScriptRoot '..\tests\evals\benchmarks\core.json'),
    [int]$Attempts = 3
)
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $repo
try {
    $lines = & cargo run --quiet -p evohime-eval -- benchmark --suite $Suite --attempts $Attempts --mode deterministic 2>&1
    if ($LASTEXITCODE -ne 0) { throw "benchmark matrix failed with exit code $LASTEXITCODE" }
    $report = ($lines | Select-Object -Last 1) | ConvertFrom-Json
    if ($report.redaction_status -ne 'redacted') { throw 'benchmark report is not redacted' }
    if ($report.metrics.Count -eq 0) { throw 'benchmark report has no metrics' }
    Write-Output "benchmark matrix: PASS ($($report.metrics.Count) combinations; $Attempts attempts)"
} finally { Pop-Location }
