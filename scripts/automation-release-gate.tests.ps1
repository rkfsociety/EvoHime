[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$automationFiles = @(
    (Join-Path $repo 'crates\evohime-core\src\automation.rs'),
    (Join-Path $repo 'crates\evohime-core\src\automation_runtime.rs'),
    (Join-Path $repo 'crates\evohime-core\src\automation_simulation.rs'),
    (Join-Path $repo 'crates\evohime-core\src\automation_acceptance.rs'),
    (Join-Path $repo 'crates\evohime-local-storage\src\automation_store.rs')
)
foreach ($path in $automationFiles) {
    if (-not (Test-Path -LiteralPath $path)) { throw "Automation gate input missing: $path" }
}

# Automation contracts must stay filesystem/network/process free. Provider
# calls are represented by the bounded operation contract and are dispatched
# by the existing Core gateway, not by this module.
foreach ($path in $automationFiles) {
    $raw = Get-Content -LiteralPath $path -Raw
    foreach ($forbidden in @('std::fs', 'reqwest::', 'std::process::Command', 'create_dir_all')) {
        if ($raw.Contains($forbidden)) { throw "Forbidden host capability $forbidden in $path" }
    }
}

Push-Location $repo
try {
    & cargo test --locked -p evohime-core automation_acceptance --lib
    if ($LASTEXITCODE -ne 0) { throw "Automation acceptance tests failed with exit code $LASTEXITCODE" }
    & cargo test --locked -p evohime-local-storage automation_store --lib
    if ($LASTEXITCODE -ne 0) { throw "Automation storage tests failed with exit code $LASTEXITCODE" }
    & cargo check --locked -p evohime-core -p evohime-local-storage
    if ($LASTEXITCODE -ne 0) { throw "Automation locked check failed with exit code $LASTEXITCODE" }
} finally { Pop-Location }
Write-Output 'automation runtime boundary gate: PASS'
