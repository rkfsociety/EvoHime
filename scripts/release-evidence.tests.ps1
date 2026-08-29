[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$required = @(
    (Join-Path $repo 'docs\release-evidence.md'),
    (Join-Path $repo 'docs\licenses\README.md'),
    (Join-Path $repo 'docs\decision-register.md')
)
foreach ($path in $required) { if (-not (Test-Path -LiteralPath $path)) { throw "Release evidence input missing: $path" } }
foreach ($path in $required) {
    $raw = Get-Content -LiteralPath $path -Raw
    foreach ($marker in @('password=', 'api_key', 'bearer ', 'private_key', '@gmail.com')) {
        if ($raw.ToLowerInvariant().Contains($marker)) { throw "Secret/PII marker $marker in $path" }
    }
}
Push-Location $repo
try {
    & cargo test --locked -p evohime-local-storage backup::tests --lib
    if ($LASTEXITCODE -ne 0) { throw "Backup/restore evidence tests failed with exit code $LASTEXITCODE" }
    & cargo test --locked -p evohime-local-storage automation_store::tests::archive_restore_is_atomic_checksum_verified_and_retention_bounded --lib
    if ($LASTEXITCODE -ne 0) { throw "Automation archive/restore evidence tests failed with exit code $LASTEXITCODE" }
    & cargo test --locked -p evohime-core automation_acceptance --lib
    if ($LASTEXITCODE -ne 0) { throw "Automation evidence tests failed with exit code $LASTEXITCODE" }
    & cargo test --locked -p evohime-core analysis_kernel --lib
    if ($LASTEXITCODE -ne 0) { throw "Analysis kernel Core evidence tests failed with exit code $LASTEXITCODE" }
    & cargo test --locked -p evohime-supervisor --bin evohime-analysis-worker --bin evohime-supervisor
    if ($LASTEXITCODE -ne 0) { throw "Analysis kernel worker/supervisor evidence tests failed with exit code $LASTEXITCODE" }
    & (Join-Path $repo 'scripts\license-inventory.tests.ps1')
    if ($LASTEXITCODE -ne 0) { throw "License inventory gate failed with exit code $LASTEXITCODE" }
} finally { Pop-Location }
Write-Output 'release evidence gate: PASS'
