$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$script = Get-Content -LiteralPath (Join-Path $root 'scripts\repair-updater.ps1') -Raw
$agent = Get-Content -LiteralPath (Join-Path $root 'crates\evohime-update-agent\src\main.rs') -Raw
$agentLib = Get-Content -LiteralPath (Join-Path $root 'crates\evohime-update-agent\src\lib.rs') -Raw

foreach ($required in @(
    'api.github.com/repos/$repository/releases?per_page=100',
    'updater.manifest.json',
    'Get-FileHash -LiteralPath $download -Algorithm SHA256',
    '[IO.File]::Replace($expandedWorker, $target, $backup',
    'Expand-Archive -LiteralPath $download',
    "expectedArtifact -ne 'updater.zip'",
    'Join-Path $expandedUi',
    'Get-CimInstance Win32_Process',
    'MaximumRedirection 5'
)) {
    if ($script -notmatch [regex]::Escape($required)) { throw "Recovery script missing: $required" }
}
if ($script -match 'Stop-Process|Remove-Item -LiteralPath \$target') {
    throw 'Recovery script must not kill processes or delete the installed updater before replacement.'
}
if ($script -notmatch "expectedHash -notmatch '\^\[0-9a-f\]\{64\}\$'") {
    throw 'Recovery script does not validate the manifest SHA-256 shape.'
}
foreach ($required in @('--self-test', 'launch_preflight', 'updater-fallback.exe', 'manual-recovery')) {
    if ($agent -notmatch [regex]::Escape($required)) { throw "Updater agent missing recovery contract: $required" }
}
if ($agent -notmatch 'evohime_tx::apply_component_set_staged') {
    throw 'Updater agent does not own the embedded transaction engine.'
}
if ($agent -match 'install_dir\.join\("evohime-transaction\.exe"\)') {
    throw 'Updater agent still depends on the installed transaction executable.'
}
foreach ($required in @('RECOVERY_SCHEMA', 'write_recovery_journal', 'validate_pe_artifact', 'MAX_RECOVERY_BYTES')) {
    if ($agentLib -notmatch [regex]::Escape($required)) { throw "Updater recovery library missing: $required" }
}
Write-Host 'updater recovery smoke: PASS'
