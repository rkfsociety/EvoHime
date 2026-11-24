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
if ($text -notmatch 'gh api --method DELETE') { throw 'Old module release cleanup missing.' }
if ($text -match "NotesFile = 'installer/release-notes\.md'") { throw 'Module release still inherits installer notes by default.' }
if ($text -notmatch 'Версия модуля') { throw 'Module release version text is missing.' }
if ($text -match '# \$Module \$Version') { throw 'Module release body still duplicates the release title.' }
if ($text -notmatch 'gh release create \$tag .*--title \$Module') { throw 'Module release title must contain only the module name.' }
if ($text -notmatch 'gh release edit \$tag .*--title \$Module') { throw 'Module release edit title must contain only the module name.' }
if ($text -notmatch 'module-release-notes\.ps1' -or $text -notmatch 'Resolve-ModuleReleaseChanges') { throw 'Module release does not use Resolve-ModuleReleaseChanges.' }

. (Join-Path $PSScriptRoot 'module-release-notes.ps1')

$notesFile = Join-Path $root 'core.md'
@'
## 0.0.1540

- Лишняя секция с похожим префиксом версии.

## 0.0.154

<!-- комментарий -->
- Улучшена скорость запуска фоновых задач.
* Исправлено сохранение настроек провайдера.
Уточнено сообщение об ошибке подключения.

## 0.0.153

- Старая версия, не должна попасть в 0.0.154.
'@ | Set-Content -LiteralPath $notesFile -Encoding utf8NoBOM

$resolved = Resolve-ModuleReleaseChanges -Module 'core' -Version '0.0.154' -NotesPath $notesFile
$expected = @(
    '- Улучшена скорость запуска фоновых задач.',
    '- Исправлено сохранение настроек провайдера.',
    '- Уточнено сообщение об ошибке подключения.'
)
if (($resolved -join "`n") -ne ($expected -join "`n")) {
    throw "Unexpected release notes parsed: $($resolved -join ' | ')"
}

$gitRepo = Join-Path $root 'repo'
New-Item -ItemType Directory -Force -Path $gitRepo | Out-Null
Push-Location $gitRepo
try {
    git init -q
    git config user.email 'test@example.com'
    git config user.name 'Test'
    New-Item -ItemType Directory -Force -Path 'crates/evohime-core' | Out-Null
    Set-Content -LiteralPath 'crates/evohime-core/lib.rs' -Value 'v1' -Encoding utf8NoBOM
    git add .
    git commit -q -m 'feat(core): старый коммит до тега'
    git tag 'module-core-v0.0.153'

    Set-Content -LiteralPath 'crates/evohime-core/lib.rs' -Value 'v2' -Encoding utf8NoBOM
    git add .
    git commit -q -m 'chore(release): поднять версию core'

    Set-Content -LiteralPath 'crates/evohime-core/lib.rs' -Value 'v3' -Encoding utf8NoBOM
    git add .
    git commit -q -m 'fix(core): исправлена обработка таймаута'

    $fallbackChanges = Resolve-ModuleReleaseChanges -Module 'core' -Version '0.0.154' -NotesPath (Join-Path $root 'missing.md') -ModulePaths @('crates/evohime-core') -RepoRoot $gitRepo
    if ($fallbackChanges.Count -ne 1 -or $fallbackChanges[0] -ne '- fix(core): исправлена обработка таймаута') {
        throw "Unexpected git fallback changes: $($fallbackChanges -join ' | ')"
    }

    git tag 'module-core-v0.0.154'
    Set-Content -LiteralPath 'crates/evohime-core/lib.rs' -Value 'v4' -Encoding utf8NoBOM
    git add .
    git commit -q -m 'chore(release): только служебный коммит'

    $emptyFallback = Resolve-ModuleReleaseChanges -Module 'core' -Version '0.0.155' -NotesPath (Join-Path $root 'missing.md') -ModulePaths @('crates/evohime-core') -RepoRoot $gitRepo
    if ($emptyFallback.Count -ne 1 -or $emptyFallback[0] -ne '- Обновлён состав поставки модуля и его проверенный бинарный артефакт.') {
        throw "Unexpected default fallback change: $($emptyFallback -join ' | ')"
    }
} finally {
    Pop-Location
}

Remove-Item -LiteralPath $root -Recurse -Force
Write-Output 'module release smoke: PASS'
