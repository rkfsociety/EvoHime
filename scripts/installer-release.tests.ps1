$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$workflow = Get-Content -LiteralPath (Join-Path $root '.github\workflows\installer.yml') -Raw
$publisher = Get-Content -LiteralPath (Join-Path $root 'scripts\publish-installer-release.ps1') -Raw
$latestMarker = Get-Content -LiteralPath (Join-Path $root 'scripts\mark-installer-release-latest.ps1') -Raw
$modulePublisher = Get-Content -LiteralPath (Join-Path $root 'scripts\module-release.ps1') -Raw
$compatiblePublisher = Get-Content -LiteralPath (Join-Path $root 'scripts\publish-compatible-manifest.ps1') -Raw
$listenerWorkflow = Get-Content -LiteralPath (Join-Path $root '.github\workflows\listener.yml') -Raw
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
if ($publisher -notmatch 'gh release create \$Tag.*--title \$Tag') { throw 'Installer release title must contain only the module name.' }
if ($publisher -match 'EvoHime installer \$Version') { throw 'Installer release title still contains product/version text.' }
if ($publisher -notmatch 'gh release upload \$Tag.*\$setup.*--clobber') { throw 'Installer publisher misses setup upload.' }
if ($publisher -notmatch 'gh release upload \$Tag.*\$manifestPath.*--clobber') { throw 'Installer publisher misses manifest upload.' }
if ($publisher -notmatch 'gh release create \$Tag.*--latest') { throw 'Installer publisher does not mark its release as Latest.' }
if ($latestMarker -notmatch 'gh release edit \$Tag.*--latest') { throw 'Installer latest marker is missing.' }
if ($modulePublisher -notmatch 'gh release create \$tag .*--latest=false' -or $modulePublisher -notmatch 'gh release edit \$tag .*--latest=false') { throw 'Module publisher may replace installer as Latest.' }
if ($compatiblePublisher -notmatch 'gh release create \$tag .*--latest=false' -or $compatiblePublisher -notmatch 'gh release edit \$tag .*--latest=false') { throw 'Compatible manifest publisher may replace installer as Latest.' }
if ($listenerWorkflow -notmatch 'gh release create \$releaseTag .*--latest=false' -or $listenerWorkflow -notmatch 'gh release edit \$releaseTag .*--latest=false') { throw 'Listener publisher may replace installer as Latest.' }
if ($publisher -notmatch 'mark-installer-release-latest\.ps1' -or $modulePublisher -notmatch 'mark-installer-release-latest\.ps1' -or $compatiblePublisher -notmatch 'mark-installer-release-latest\.ps1' -or $listenerWorkflow -notmatch 'mark-installer-release-latest\.ps1') { throw 'All release publishers must restore installer as Latest.' }
if ($iss -notmatch 'Source: "\{#SourceDir\}\\\*"' -or $iss -notmatch 'updater\\EvoHimeUpdater\.exe') { throw 'Installer Inno script misses required runtime files.' }
Write-Host 'web installer release contract tests passed.'
