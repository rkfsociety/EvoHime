$ErrorActionPreference = 'Stop'

# Релизы больше не публикуются автоматически: клиент обновляется из исходников
# по коммитам, а установщик остаётся артефактом прогона. Скрипт очистки
# сохранён как ручной инструмент, и проверяется именно он — чтобы случайный
# запуск не снёс ничего, кроме релизов с версионным тегом.
$cleanup = Get-Content -Raw (Join-Path $PSScriptRoot 'cleanup-github-releases.ps1')

foreach ($required in @(
    'KeepCount',
    '--cleanup-tag',
    'git/refs/tags/'
)) {
    if ($cleanup -notmatch [regex]::Escape($required)) {
        throw "Retention configuration is missing: $required"
    }
}

if ($cleanup -notmatch "\^v\\d\+\\.\\d\+\\.\\d\+\$") {
    throw 'Cleanup must be restricted to semantic version tags.'
}

$workflow = Get-Content -Raw (Join-Path $PSScriptRoot '..\.github\workflows\windows.yml')
if ($workflow -match [regex]::Escape('cleanup-github-releases.ps1')) {
    throw 'Release cleanup must stay a manual tool, not a scheduled workflow job.'
}

Write-Output 'github-retention smoke: PASS'
