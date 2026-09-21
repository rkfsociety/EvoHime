[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$catalogPath = Join-Path $repo 'docs\plans\README.md'
$catalog = Get-Content -LiteralPath $catalogPath -Raw
$activeNumbers = @([regex]::Matches($catalog, '(?m)^\|\s*(\d+)\s*\|') | ForEach-Object { [int]$_.Groups[1].Value } | Sort-Object -Unique)
if ($activeNumbers.Count -eq 0) { throw 'No active implementation plans found in docs/plans/README.md' }
$activeSet = [System.Collections.Generic.HashSet[int]]::new()
foreach ($number in $activeNumbers) { [void]$activeSet.Add($number) }
$planFiles = @(Get-ChildItem -LiteralPath (Join-Path $repo 'docs\plans') -File -Filter '*.md' | Where-Object Name -ne 'README.md')
foreach ($file in $planFiles) {
    if ($file.BaseName -notmatch '^(?<number>\d+)-(?<stage>[0-4])-.+$') { throw "Invalid implementation plan filename: $($file.Name)" }
    if (-not $activeSet.Contains([int]$Matches.number)) { throw "Plan is not listed as active: $($file.Name)" }
}
foreach ($number in $activeNumbers) {
    foreach ($stage in 0..4) {
        $stageFilter = $number.ToString() + '-' + $stage.ToString() + '-*.md'
        $stageFiles = @(Get-ChildItem -LiteralPath (Join-Path $repo 'docs\plans') -File -Filter $stageFilter)
        if ($stageFiles.Count -ne 1) { throw "Active plan $number must have exactly one stage $stage file." }
    }
}
Push-Location $repo
try {
    & cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw 'cargo fmt gate failed' }
    & cargo test --locked -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc
    if ($LASTEXITCODE -ne 0) { throw 'Rust release audit tests failed' }
    & (Join-Path $repo 'scripts\automation-release-gate.tests.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'Automation boundary gate failed' }
    & (Join-Path $repo 'scripts\release-evidence.tests.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'Release evidence gate failed' }
    Push-Location (Join-Path $repo 'desktop\evohime-electron')
    try {
        & npm run check:protocol
        if ($LASTEXITCODE -ne 0) { throw 'Electron protocol gate failed' }
        & npm run typecheck
        if ($LASTEXITCODE -ne 0) { throw 'Electron typecheck failed' }
    } finally { Pop-Location }
} finally { Pop-Location }
Write-Output 'final release audit: TECHNICAL GATES PASS; release status GREEN (code signing is out of scope)'
