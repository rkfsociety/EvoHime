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
$editMatch = [regex]::Match($workflow, '(?ms)gh release view \$env:RELEASE_TAG.*?gh release upload')
if (-not $editMatch.Success -or $editMatch.Value -notmatch [regex]::Escape($notesArgument)) {
    throw 'Installer workflow does not refresh the existing release description.'
}
if ($workflow -match '(?i)описание\s+релиза\s+остаётся\s+без\s+изменений') {
    throw 'Installer workflow still documents stale release descriptions.'
}

Write-Host 'installer release notes/workflow gate: PASS'
