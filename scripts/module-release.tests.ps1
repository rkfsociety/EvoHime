$ErrorActionPreference = 'Stop'
$root = Join-Path $env:TEMP 'evohime-module-release-smoke'
Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $root | Out-Null
$artifact = Join-Path $root 'core.exe'
Set-Content -LiteralPath $artifact -Value 'core fixture' -Encoding utf8NoBOM
$text = Get-Content -LiteralPath 'scripts/module-release.ps1' -Raw
if ($text -notmatch "schema = 'evohime\.module-release\.v1'") { throw 'Module manifest schema missing.' }
if ($text -notmatch 'gh release upload \$tag') { throw 'Module release upload missing.' }
if ($text -notmatch 'sha256') { throw 'Module hash missing.' }
Remove-Item -LiteralPath $root -Recurse -Force
Write-Output 'module release smoke: PASS'
