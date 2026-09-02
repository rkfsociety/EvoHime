$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$out = Join-Path ([IO.Path]::GetTempPath()) 'evohime-runtime-stall-findings.json'
& pwsh -NoProfile -File (Join-Path $PSScriptRoot 'runtime-stall-guard.ps1') -Root $root -Out $out
if ($LASTEXITCODE -ne 0) { throw 'Runtime Stall Guard did not produce a report.' }
$report = Get-Content -LiteralPath $out -Raw | ConvertFrom-Json
if ($report.schema_version -ne 1) { throw 'Unexpected report schema.' }
if ($report.findings.Count -gt 4096) { throw 'Finding bound exceeded.' }
if ($report.findings | Where-Object { $_.file -match '^[A-Za-z]:|^/' }) { throw 'Report leaked absolute paths.' }
Remove-Item -LiteralPath $out -Force
Write-Output 'Runtime Stall Guard PASS'
