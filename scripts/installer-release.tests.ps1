$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$notesPath = Join-Path $root 'installer\release-notes.md'
$workflowPath = Join-Path $root '.github\workflows\windows.yml'
$notes = Get-Content -LiteralPath $notesPath -Raw
$workflow = Get-Content -LiteralPath $workflowPath -Raw

if ($notes -match 'единственный релиз проекта') { throw 'Installer release notes contain obsolete single-release wording.' }
foreach ($required in @('fixed release', 'compatibility', 'module releases', 'EvoHime-Setup.json')) {
    if ($notes -notmatch [regex]::Escape($required)) { throw "Installer release notes miss required text: $required" }
}

$notesArgument = "--notes-file 'installer/release-notes.md'"
if ($workflow -notmatch [regex]::Escape($notesArgument)) { throw 'Installer workflow does not create the release from canonical notes.' }
$releaseDelete = 'gh api --method DELETE "repos/$env:GITHUB_REPOSITORY/releases/$releaseId"'
$tagDelete = 'gh api --method DELETE "repos/$env:GITHUB_REPOSITORY/git/refs/tags/$env:RELEASE_TAG"'
$releaseCreate = 'gh release create $env:RELEASE_TAG'
foreach ($required in @($releaseDelete, $tagDelete, $releaseCreate)) {
    if ($workflow -notmatch [regex]::Escape($required)) { throw "Installer workflow misses replacement operation: $required" }
}
if ($workflow -match '(?m)^\s*gh release edit\b') {
    throw 'Installer workflow still edits the old release instead of replacing it.'
}
$releaseDeleteIndex = $workflow.IndexOf($releaseDelete, [StringComparison]::Ordinal)
$tagDeleteIndex = $workflow.IndexOf($tagDelete, [StringComparison]::Ordinal)
$releaseCreateIndex = $workflow.IndexOf($releaseCreate, [StringComparison]::Ordinal)
if ($releaseDeleteIndex -lt 0 -or $tagDeleteIndex -lt $releaseDeleteIndex -or $releaseCreateIndex -lt $tagDeleteIndex) {
    throw 'Installer workflow must delete the old release and tag before creating the new release.'
}
if ($workflow -match '(?i)описание\s+релиза\s+остаётся\s+без\s+изменений') {
    throw 'Installer workflow still documents stale release descriptions.'
}

Write-Host 'installer release notes/workflow gate: PASS'
