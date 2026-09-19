$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$workflow = Get-Content -LiteralPath (Join-Path $root '.github\workflows\installer.yml') -Raw
$publisher = Get-Content -LiteralPath (Join-Path $root 'scripts\publish-installer-release.ps1') -Raw
$iss = Get-Content -LiteralPath (Join-Path $root 'installer\EvoHime.iss') -Raw

foreach ($required in @('module-updater-v$updaterVersion', 'build-installer-source.ps1', 'EvoHime.iss', 'installer-content.tests.ps1')) {
    if ($workflow -notmatch [regex]::Escape($required)) { throw "Installer workflow misses: $required" }
}
if ($workflow -notmatch 'github\.ref == .refs/heads/main') { throw 'Installer publication is not restricted to main.' }
if ($publisher -notmatch "\$Tag = 'installer'") { throw 'Web installer publisher uses an unexpected release tag.' }
if ($publisher -notmatch 'releaseExit' -or $publisher -notmatch 'releaseAbsent.*HTTP.*404') { throw 'Installer publisher does not fail closed on release lookup errors.' }
foreach ($required in @('gh api --method DELETE "repos/$repo/releases/$releaseId"', 'gh release create $Tag')) {
    if ($publisher -notmatch [regex]::Escape($required)) { throw "Installer publisher misses release recreation operation: $required" }
}
if ($publisher -match 'gh release edit') { throw 'Installer publisher still edits the old release instead of recreating it.' }
if ($publisher -match 'git/refs/tags') { throw 'Installer publisher must preserve the permanent installer tag.' }
if ($publisher -notmatch 'gh release upload \$Tag.*\$setup.*--clobber') { throw 'Installer publisher misses setup upload.' }
if ($publisher -notmatch 'gh release upload \$Tag.*\$manifestPath.*--clobber') { throw 'Installer publisher misses manifest upload.' }
if ($iss -notmatch 'Source: "\{#SourceDir\}\\\*"' -or $iss -notmatch 'updater\\EvoHimeUpdater\.exe') { throw 'Installer Inno script misses required runtime files.' }
Write-Host 'web installer release contract tests passed.'
