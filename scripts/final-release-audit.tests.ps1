[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$removedPlans = @(
    'docs\plans\16-0-workflow-automation-and-simulation.md',
    'docs\plans\17-0-release-criteria-and-open-decisions.md',
    'docs\plans\17-4-release-audit-and-acceptance.md'
)
foreach ($path in $removedPlans) { if (Test-Path -LiteralPath (Join-Path $repo $path)) { throw "Completed plan must be removed: $path" } }
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
Write-Output 'final release audit: TECHNICAL GATES PASS; release status remains BLOCKED by documented open decisions'
