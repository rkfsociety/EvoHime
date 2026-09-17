$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$workflow = Get-Content -LiteralPath (Join-Path $root '.github\workflows\bootstrap-installer.yml') -Raw
$publisher = Get-Content -LiteralPath (Join-Path $root 'scripts\publish-bootstrap-release.ps1') -Raw
$iss = Get-Content -LiteralPath (Join-Path $root 'installer\EvoHimeBootstrap.iss') -Raw

foreach ($required in @('module-updater-v$updaterVersion', 'build-bootstrap-source.ps1', 'EvoHimeBootstrap.iss', 'bootstrap-installer.tests.ps1')) {
    if ($workflow -notmatch [regex]::Escape($required)) { throw "Bootstrap workflow misses: $required" }
}
if ($workflow -notmatch 'github\.ref == .refs/heads/main') { throw 'Bootstrap publication is not restricted to main.' }
if ($publisher -notmatch "\$Tag = 'bootstrap'") { throw 'Bootstrap publisher uses an unexpected release tag.' }
if ($publisher -notmatch 'releaseExit' -or $publisher -notmatch 'releaseAbsent.*HTTP.*404') { throw 'Bootstrap publisher does not fail closed on release lookup errors.' }
if ($publisher -notmatch 'gh release upload \$Tag.*\$setup.*--clobber') { throw 'Bootstrap publisher misses setup upload.' }
if ($publisher -notmatch 'gh release upload \$Tag.*\$manifestPath.*--clobber') { throw 'Bootstrap publisher misses manifest upload.' }
if ($iss -notmatch 'Source: "\{#SourceDir\}\\\*"' -or $iss -notmatch 'updater\\EvoHimeUpdater\.exe') { throw 'Bootstrap Inno script misses required runtime files.' }
Write-Host 'bootstrap release contract tests passed.'
