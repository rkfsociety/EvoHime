$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$script = Get-Content -LiteralPath (Join-Path $root 'scripts\repair-updater.ps1') -Raw

foreach ($required in @(
    'api.github.com/repos/$repository/releases?per_page=100',
    'updater.manifest.json',
    'Get-FileHash -LiteralPath $download -Algorithm SHA256',
    '[IO.File]::Replace($download, $target, $backup',
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
Write-Host 'updater recovery smoke: PASS'
