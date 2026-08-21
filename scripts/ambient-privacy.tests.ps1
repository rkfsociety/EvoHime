[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$audio = Join-Path $root 'crates\evohime-listener-audio\src'
$forbidden = @('std::fs', 'std::fs::', 'File::', 'OpenOptions', 'tempfile', 'create_dir', 'write_all')
# Поиск идёт встроенным Select-String, а не ripgrep: гейт обязан работать на
# любой машине и на раннере, где внешнего rg в образе нет.
$sources = @(Get-ChildItem -LiteralPath $audio -Recurse -File -Filter '*.rs')
if ($sources.Count -eq 0) { throw "Audio crate sources not found under '$audio'." }
foreach ($needle in $forbidden) {
    $hits = @($sources | Select-String -SimpleMatch -Pattern $needle)
    if ($hits.Count -gt 0) {
        $where = ($hits | ForEach-Object { "$($_.Path):$($_.LineNumber)" }) -join ', '
        throw "Audio crate contains forbidden filesystem API '$needle': $where"
    }
}
$cargo = Get-Command cargo -ErrorAction Stop
& $cargo.Source test --locked -p evohime-listener-audio
if ($LASTEXITCODE -ne 0) { throw 'Listener audio privacy tests failed.' }
$manifest = Join-Path $root 'scripts\native-package.ps1'
$manifestText = Get-Content -LiteralPath $manifest -Raw
if ($manifestText -notmatch "listener\s*=\s*'evohime-listener\.exe'") { throw 'Native package manifest does not allow-list the listener.' }
Write-Host 'ambient privacy gate: PASS'
