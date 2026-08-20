[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$audio = Join-Path $root 'crates\evohime-listener-audio\src'
$forbidden = @('std::fs', 'std::fs::', 'File::', 'OpenOptions', 'tempfile', 'create_dir', 'write_all')
foreach ($needle in $forbidden) {
    $hits = rg --fixed-strings --line-number $needle $audio 2>$null
    if ($LASTEXITCODE -eq 0 -and $hits) { throw "Audio crate contains forbidden filesystem API '$needle': $hits" }
}
$cargo = Get-Command cargo -ErrorAction Stop
& $cargo.Source test --locked -p evohime-listener-audio
if ($LASTEXITCODE -ne 0) { throw 'Listener audio privacy tests failed.' }
$manifest = Join-Path $root 'scripts\native-package.ps1'
$manifestText = Get-Content -LiteralPath $manifest -Raw
if ($manifestText -notmatch "listener\s*=\s*'evohime-listener\.exe'") { throw 'Native package manifest does not allow-list the listener.' }
Write-Host 'ambient privacy gate: PASS'
