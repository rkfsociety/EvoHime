param(
    [Parameter(Mandatory)] [ValidatePattern('^[a-z0-9][a-z0-9-]*$')] [string]$Module,
    [Parameter(Mandatory)] [ValidatePattern('^\d+\.\d+\.\d+$')] [string]$Version,
    [Parameter(Mandatory)] [string]$Artifact,
    [string]$Summary
)

$ErrorActionPreference = 'Stop'
if (-not $env:GH_TOKEN) { throw 'GH_TOKEN is required.' }
if (-not (Test-Path -LiteralPath $Artifact -PathType Leaf)) { throw "Module artifact missing: $Artifact" }

$repo = if ($env:GITHUB_REPOSITORY) { $env:GITHUB_REPOSITORY } else { (gh repo view --json nameWithOwner --jq .nameWithOwner) }
$tag = "module-$Module-v$Version"
$artifactPath = (Resolve-Path -LiteralPath $Artifact).Path
$artifactName = Split-Path -Leaf $artifactPath
$hash = (Get-FileHash -LiteralPath $artifactPath -Algorithm SHA256).Hash.ToLowerInvariant()
$size = [int64](Get-Item -LiteralPath $artifactPath).Length
$manifestPath = Join-Path $env:RUNNER_TEMP "$Module.manifest.json"
$moduleDependencies = @{
    'shell-host' = @('ui-bundle', 'core', 'supervisor', 'transaction', 'verifier')
    'ui-bundle' = @()
    'core' = @('supervisor')
    'supervisor' = @('transaction', 'verifier')
    'cli' = @('core')
    'analysis-worker' = @('core')
    'listener' = @('core', 'listener-runtime')
    'transaction' = @()
    'verifier' = @()
    'listener-runtime' = @()
}
$moduleRestart = @{
    'shell-host' = 'shell'; 'ui-bundle' = 'shell'; 'core' = 'core';
    'supervisor' = 'supervisor'; 'cli' = 'none'; 'analysis-worker' = 'core';
    'listener' = 'listener'; 'transaction' = 'transaction'; 'verifier' = 'none';
    'listener-runtime' = 'listener'
}
$dependencies = if ($moduleDependencies.ContainsKey($Module)) { $moduleDependencies[$Module] } else { @() }
$restart = if ($moduleRestart.ContainsKey($Module)) { $moduleRestart[$Module] } else { 'module' }
$manifest = [ordered]@{
    schema = 'evohime.module-release.v1'
    module = $Module
    version = $Version
    artifact = $artifactName
    size = $size
    sha256 = $hash
    release_tag = $tag
    dependencies = $dependencies
    restart = $restart
}
$manifest | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $manifestPath -Encoding utf8NoBOM

$generatedNotesPath = Join-Path $env:RUNNER_TEMP "$Module-release-notes.md"
$runUrl = if ($env:GITHUB_SERVER_URL -and $env:GITHUB_REPOSITORY -and $env:GITHUB_RUN_ID) {
    "$($env:GITHUB_SERVER_URL)/$($env:GITHUB_REPOSITORY)/actions/runs/$($env:GITHUB_RUN_ID)"
} else { $null }
$modulePaths = @{
    'shell-host' = @('desktop/evohime-electron/src/main', 'desktop/evohime-electron/src/preload')
    'ui-bundle' = @('desktop/evohime-electron/src/renderer', 'desktop/evohime-electron/src/shared')
    'core' = @('crates/evohime-core', 'crates/evohime-model-gateway', 'crates/evohime-permissions', 'crates/tool-runtime', 'crates/evohime-local-storage')
    'supervisor' = @('crates/evohime-supervisor', 'crates/evohime-process')
    'cli' = @('crates/evohime-cli')
    'analysis-worker' = @('crates/evohime-supervisor/src/analysis_worker.rs')
    'listener' = @('crates/evohime-listener', 'crates/evohime-listener-ipc')
    'transaction' = @('crates/evohime-updater')
    'verifier' = @('crates/evohime-receipts')
}
$changes = if ($modulePaths.ContainsKey($Module)) { @(git log -5 --pretty=format:'- %s' -- $modulePaths[$Module] 2>$null) } else { @() }
if ($changes.Count -eq 0) { $changes = @('- Обновлён состав поставки модуля и его проверенный бинарный артефакт.') }
$notes = [System.Collections.Generic.List[string]]::new()
$notes.Add(('Версия модуля `{0}` опубликована после успешных проверок и сборки.' -f $Version))
$notes.Add('')
$summaryText = if ($Summary) { $Summary } else {
    switch ($Module) {
        'shell-host' { 'Оболочка Electron: окно приложения, IPC-адаптеры, preload и запуск пользовательского интерфейса.'; break }
        'ui-bundle' { 'Собранный renderer-интерфейс приложения с проверкой IPC-контракта и типизации.'; break }
        'core' { 'Основной Rust runtime агента: инструменты, провайдеры, локальное состояние и обработка задач.'; break }
        'supervisor' { 'Windows supervisor: жизненный цикл Core, mutex, Job Object, восстановление и журналы.'; break }
        'cli' { 'Официальный консольный клиент для диагностики, запуска задач и чтения статуса Core.'; break }
        'analysis-worker' { 'Фоновый worker анализа данных и выполнения связанных аналитических операций.'; break }
        'listener' { 'Исполняемый listener для захвата аудио и обмена с Core по защищённому IPC.'; break }
        'transaction' { 'Worker транзакционного обновления: безопасная замена файлов, backup и rollback.'; break }
        'verifier' { 'Проверяющий worker целостности и контрактов поставляемых файлов.'; break }
        default { "Компонент `$Module` поставлен как самостоятельный модуль EvoHime."; break }
    }
}
$notes.Add('## Назначение')
$notes.Add($summaryText)
$notes.Add('')
$notes.Add('## Изменения модуля')
$notes.Add('Обновление заменяет только данный модуль; остальные модули не изменяются.')
$notes.Add('')
$notes.Add('## Артефакт и проверки')
$notes.Add(('- Версия: `{0}`' -f $Version))
$notes.Add(('- SHA-256: `{0}`' -f $hash))
$notes.Add("- Размер: $size байт")
$notes.Add('- Тесты, lint и release-сборка успешно завершены до публикации.')
$notes.Add('')
$notes.Add('## Что изменилось')
$changes | ForEach-Object { $notes.Add($_) }
if ($runUrl) { $notes.Add('') ; $notes.Add("Публикация выполнена отдельным workflow после успешных тестов и сборки: [$runUrl]($runUrl)") }
$notes | Set-Content -LiteralPath $generatedNotesPath -Encoding utf8NoBOM

gh release view $tag --repo $repo 2>$null | Out-Null
if ($LASTEXITCODE -ne 0) {
    gh release create $tag --repo $repo --title "$Module $Version" --notes-file $generatedNotesPath
} else {
    gh release edit $tag --repo $repo --title "$Module $Version" --notes-file $generatedNotesPath
}
gh release upload $tag --repo $repo $artifactPath --clobber
if ($LASTEXITCODE -ne 0) { throw "Failed to publish $artifactName." }
gh release upload $tag --repo $repo $manifestPath --clobber
if ($LASTEXITCODE -ne 0) { throw "Failed to publish module manifest." }

# Удаляем старые версии только после успешной публикации нового релиза.
$releaseRows = @(gh api --paginate "repos/$repo/releases?per_page=100" --jq '.[] | [.tag_name, (.id|tostring)] | @tsv')
foreach ($row in $releaseRows) {
    $parts = $row -split "`t", 2
    $oldTag = $parts[0]
    $oldId = $parts[1]
    if ($oldTag -notlike "module-$Module-v*" -and $oldTag -ne "module-$Module") { continue }
    if ($oldTag -eq $tag) { continue }
    gh api --method DELETE "repos/$repo/releases/$oldId"
    if ($LASTEXITCODE -ne 0) { throw "Failed to remove old release $oldTag." }
    gh api --method DELETE "repos/$repo/git/refs/tags/$oldTag" 2>$null
    Write-Host "Removed old release $oldTag"
}
Write-Host "Published $Module $Version to $tag ($hash)"
