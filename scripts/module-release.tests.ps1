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
if ($text -notmatch 'module-\$Module-v\$Version') { throw 'Versioned module release tag missing.' }
if ($text -notmatch 'gh release delete') { throw 'Old module release cleanup missing.' }
if ($text -match "NotesFile = 'installer/release-notes\.md'") { throw 'Module release still inherits installer notes by default.' }
if ($text -notmatch 'Версия модуля') { throw 'Module release version text is missing.' }
if ($text -match '# \$Module \$Version') { throw 'Module release body still duplicates the release title.' }
Remove-Item -LiteralPath $root -Recurse -Force
Write-Output 'module release smoke: PASS'
