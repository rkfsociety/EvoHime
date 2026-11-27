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
    'updater' = @()
}
$moduleRestart = @{
    'shell-host' = 'shell'; 'ui-bundle' = 'shell'; 'core' = 'core';
    'supervisor' = 'supervisor'; 'cli' = 'none'; 'analysis-worker' = 'core';
    'listener' = 'listener'; 'transaction' = 'transaction'; 'verifier' = 'none';
    'listener-runtime' = 'listener'
    'updater' = 'updater'
}
$dependencies = [System.Collections.Generic.List[string]]::new()
if ($moduleDependencies.ContainsKey($Module)) {
    foreach ($dependency in @($moduleDependencies[$Module])) {
        if ($null -ne $dependency) { [void]$dependencies.Add([string]$dependency) }
    }
}
$restart = if ($moduleRestart.ContainsKey($Module)) { $moduleRestart[$Module] } else { 'module' }
$modulePaths = @{
    'shell-host' = @('desktop/evohime-electron/src/main', 'desktop/evohime-electron/src/preload')
    'ui-bundle' = @('desktop/evohime-electron/src/renderer', 'desktop/evohime-electron/src/shared')
    'core' = @('crates/evohime-core', 'crates/context-budget', 'crates/desktop-ipc', 'crates/evohime-cli-contract', 'crates/evohime-receipts', 'crates/model-gateway', 'crates/permissions', 'crates/tool-runtime', 'crates/evohime-local-storage')
    'supervisor' = @('crates/evohime-supervisor', 'crates/evohime-process')
    'cli' = @('crates/evohime-cli', 'crates/evohime-cli-protocol', 'crates/evohime-cli-contract', 'crates/desktop-ipc')
    'analysis-worker' = @('crates/evohime-supervisor/src/analysis_worker.rs')
    'listener' = @('crates/evohime-listener', 'crates/evohime-listener-ipc')
    'listener-runtime' = @('scripts/build-listener-runtime.ps1', 'crates/evohime-listener/src/tools_dir.rs')
    'core-assets' = @('assets/core-assets')
    'evals' = @('crates/evohime-evals-service')
    'media-voice' = @('crates/evohime-media-voice')
    'research' = @('crates/evohime-research')
    'remote-a2a' = @('crates/evohime-remote')
    'ambient' = @('crates/evohime-ambient')
    'computer-use' = @('crates/evohime-computer-use')
    'workflow-engine' = @('crates/evohime-workflow')
    'rag-memory' = @('crates/evohime-rag-memory')
    'tools-workspace' = @('crates/evohime-tools-workspace')
    'tools-exec' = @('crates/evohime-tools-exec')
    'model-adapters' = @('crates/evohime-model-adapters')
    'transaction' = @('crates/evohime-updater')
    'verifier' = @('crates/evohime-receipts')
    'updater' = @(
        'crates/evohime-update-agent',
        'crates/evohime-updater',
        'desktop/evohime-electron/src/main/updater.ts',
        'desktop/evohime-electron/src/main/updater-window.ts',
        'desktop/evohime-electron/src/main/update/module-update-service.ts',
        'desktop/evohime-electron/src/main/update',
        'desktop/evohime-electron/src/preload/updater.ts',
        'desktop/evohime-electron/src/renderer/updater.html',
        'desktop/evohime-electron/src/renderer/src/UpdaterApp.tsx',
        'desktop/evohime-electron/src/renderer/src/UpdaterSurface.css',
        'desktop/evohime-electron/src/renderer/src/updater-main.tsx',
        'desktop/evohime-electron/src/shared/updater.ts',
        'desktop/evohime-electron/src/shared/update.ts',
        'desktop/evohime-electron/electron-builder-updater.yml',
        'desktop/evohime-electron/electron.vite.config.ts',
        'desktop/evohime-electron/scripts/build-electron.mjs'
    )
}
. (Join-Path $PSScriptRoot 'module-release-notes.ps1')
$selectedPaths = if ($modulePaths.ContainsKey($Module)) { @($modulePaths[$Module]) } else { @() }
[string[]]$changes = Resolve-ModuleReleaseChanges -Module $Module -Version $Version -ModulePaths $selectedPaths
$summaryText = if ($Summary) { $Summary } else {
    switch ($Module) {
        'shell-host' { 'Оболочка Electron: окно приложения, IPC-адаптеры, preload и запуск пользовательского интерфейса.'; break }
        'ui-bundle' { 'Собранный renderer-интерфейс приложения с проверкой IPC-контракта и типизации.'; break }
        'core' { 'Основной Rust runtime агента: инструменты, провайдеры, локальное состояние и обработка задач.'; break }
        'supervisor' { 'Windows supervisor: жизненный цикл Core, mutex, Job Object, восстановление и журналы.'; break }
        'cli' { 'Официальный консольный клиент для диагностики, запуска задач и чтения статуса Core.'; break }
        'analysis-worker' { 'Фоновый worker анализа данных и выполнения связанных аналитических операций.'; break }
        'listener' { 'Исполняемый listener для захвата аудио и обмена с Core по защищённому IPC.'; break }
        'listener-runtime' { 'Библиотеки распознавания речи и модели, используемые модулем listener.'; break }
        'transaction' { 'Worker транзакционного обновления: безопасная замена файлов, backup и rollback.'; break }
        'verifier' { 'Проверяющий worker целостности и контрактов поставляемых файлов.'; break }
        'updater' { 'Самостоятельный модуль обновления: Rust worker с встроенной транзакцией и Electron-интерфейс EvoHimeUpdater.'; break }
        'hive' { 'Модуль Улья: координация ролевых агентов, карта архитектуры и наблюдение за задачами.'; break }
        'core-assets' { 'Внешний пакет декларативных данных: системные промпты, рецепты и профили без пересборки ядра.'; break }
        'evals' { 'Модуль оценки моделей: бенчмарки, симуляции инструментов и локальная калибровка.'; break }
        'media-voice' { 'Модуль картинок и голоса: генерация изображений, зрение и голосовой синтез.'; break }
        'research' { 'Модуль поиска и браузера: веб-поиск, загрузка страниц и инспекция репозиториев.'; break }
        'remote-a2a' { 'Модуль удалённых агентов: подключение внешних A2A-агентов и удалённых каналов.'; break }
        'ambient' { 'Модуль фоновой проактивности: временные сигналы и автономные эксперименты.'; break }
        'computer-use' { 'Модуль управления экраном и IDE: GUI-автоматизация и мост к редактору.'; break }
        'workflow-engine' { 'Модуль сценариев: выполнение workflow, расписания, триггеры и фоновые задания.'; break }
        'rag-memory' { 'Модуль поиска по проекту и памяти: индексация кода и извлечение фактов.'; break }
        'tools-workspace' { 'Модуль инструментов файлов и Git: чтение, патчи, поиск и работа с репозиторием.'; break }
        'tools-exec' { 'Модуль выполнения команд: запуск оболочки, скриптов, процессов и сборки.'; break }
        'model-adapters' { 'Модуль адаптеров моделей: парсинг вызовов инструментов и нормализация ответов.'; break }
        default { "Компонент `$Module` поставлен как самостоятельный модуль EvoHime."; break }
    }
}
$manifest = [ordered]@{
    schema = 'evohime.module-release.v1'
    module = $Module
    version = $Version
    artifact = $artifactName
    size = $size
    sha256 = $hash
    release_tag = $tag
    dependencies = $dependencies.ToArray()
    restart = $restart
    summary = $summaryText
    changes = @($changes)
}
$manifest | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $manifestPath -Encoding utf8NoBOM

$generatedNotesPath = Join-Path $env:RUNNER_TEMP "$Module-release-notes.md"
$runUrl = if ($env:GITHUB_SERVER_URL -and $env:GITHUB_REPOSITORY -and $env:GITHUB_RUN_ID) {
    "$($env:GITHUB_SERVER_URL)/$($env:GITHUB_REPOSITORY)/actions/runs/$($env:GITHUB_RUN_ID)"
} else { $null }
$notes = [System.Collections.Generic.List[string]]::new()
$notes.Add(('Версия модуля `{0}` опубликована после успешных проверок и сборки.' -f $Version))
$notes.Add('')
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

function Invoke-GhWithRetry {
    param(
        [Parameter(Mandatory)] [scriptblock]$Command,
        [Parameter(Mandatory)] [string]$FailureMessage,
        [int]$MaxAttempts = 5,
        [int]$InitialDelaySeconds = 5,
        [switch]$AllowNotFound
    )

    for ($attempt = 1; $attempt -le $MaxAttempts; $attempt++) {
        $global:LASTEXITCODE = 0
        $output = @(& $Command 2>&1)
        $exitCode = $LASTEXITCODE
        if ($exitCode -eq 0) {
            return $output
        }
        $details = ($output | ForEach-Object { "$_" }) -join "`n"
        if ($AllowNotFound -and $details -match '(?i)(release not found|HTTP/?[0-9.]*\s*404|404 Not Found)') {
            $global:LASTEXITCODE = 1
            return $null
        }
        if ($attempt -lt $MaxAttempts) {
            $delay = $InitialDelaySeconds * $attempt
            Write-Warning "$FailureMessage (attempt $attempt/$MaxAttempts, exit $exitCode): $details. Retrying in ${delay}s..."
            Start-Sleep -Seconds $delay
        } else {
            throw "$FailureMessage (after $MaxAttempts attempts, exit $exitCode): $details"
        }
    }
}

$releaseExists = $true
$null = Invoke-GhWithRetry -FailureMessage "Failed to check release $tag" -AllowNotFound -Command {
    gh release view $tag --repo $repo
}
if ($LASTEXITCODE -ne 0) {
    $releaseExists = $false
}

if (-not $releaseExists) {
    Invoke-GhWithRetry -FailureMessage "Failed to create release $tag" -Command {
        $out = @(gh release create $tag --repo $repo --title $Module --notes-file $generatedNotesPath --latest=false 2>&1)
        if ($LASTEXITCODE -ne 0 -and (($out -join "`n") -match '(?i)already exists')) {
            gh release edit $tag --repo $repo --title $Module --notes-file $generatedNotesPath --latest=false
            return
        }
        $out
    } | Out-Null
} else {
    Invoke-GhWithRetry -FailureMessage "Failed to update release $tag" -Command {
        gh release edit $tag --repo $repo --title $Module --notes-file $generatedNotesPath --latest=false
    } | Out-Null
}

Invoke-GhWithRetry -FailureMessage "Failed to publish $artifactName." -Command {
    gh release upload $tag --repo $repo $artifactPath --clobber
} | Out-Null

Invoke-GhWithRetry -FailureMessage "Failed to publish module manifest." -Command {
    gh release upload $tag --repo $repo $manifestPath --clobber
} | Out-Null

# Удаляем старые версии только после успешной публикации нового релиза.
$releaseRows = @(Invoke-GhWithRetry -FailureMessage "Failed to list releases for cleanup" -Command {
    gh api --paginate "repos/$repo/releases?per_page=100" --jq '.[] | [.tag_name, (.id|tostring)] | @tsv'
})
foreach ($row in $releaseRows) {
    $parts = $row -split "`t", 2
    $oldTag = $parts[0]
    $oldId = $parts[1]
    if ($oldTag -notlike "module-$Module-v*" -and $oldTag -ne "module-$Module") { continue }
    if ($oldTag -eq $tag) { continue }
    Invoke-GhWithRetry -FailureMessage "Failed to remove old release $oldTag." -Command {
        gh api --method DELETE "repos/$repo/releases/$oldId"
    } | Out-Null
    gh api --method DELETE "repos/$repo/git/refs/tags/$oldTag" 2>$null
    Write-Host "Removed old release $oldTag"
}
./scripts/mark-installer-release-latest.ps1 -Repository $repo
Write-Host "Published $Module $Version to $tag ($hash)"
