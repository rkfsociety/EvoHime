[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
if (-not $env:GH_TOKEN) { throw 'GH_TOKEN is required.' }
if (-not $env:GITHUB_REPOSITORY) { throw 'GITHUB_REPOSITORY is required.' }

$moduleIds = @(
    'shell-host', 'ui-bundle', 'core', 'supervisor', 'cli', 'analysis-worker',
    'listener', 'listener-runtime', 'transaction', 'verifier', 'updater'
)
$releasePages = @(gh api --paginate --slurp "repos/$env:GITHUB_REPOSITORY/releases?per_page=100" | ConvertFrom-Json)
if ($LASTEXITCODE -ne 0) { throw 'Не удалось получить список модульных релизов.' }
$releases = @($releasePages | ForEach-Object { $_ })

function Parse-Version([string]$value) {
    if ($value -notmatch '^\d+\.\d+\.\d+$') { return $null }
    return [Version]::Parse($value)
}

function Get-ModuleRelease([string]$module) {
    $prefix = "module-$module-v"
    $candidates = @($releases | Where-Object {
        $_.tag_name -is [string] -and $_.tag_name.StartsWith($prefix) -and $null -ne (Parse-Version $_.tag_name.Substring($prefix.Length))
    } | Sort-Object @{ Expression = { Parse-Version $_.tag_name.Substring($prefix.Length) }; Descending = $true })
    if ($candidates.Count -eq 0) { throw "Не найден опубликованный релиз модуля: $module" }
    return $candidates[0]
}

function Get-Asset($release, [string]$name) {
    $asset = @($release.assets | Where-Object { $_.name -eq $name }) | Select-Object -First 1
    if ($null -eq $asset) { throw "В релизе $($release.tag_name) отсутствует asset $name." }
    return $asset
}

function Read-AssetJson($asset) {
    $headers = @{
        Authorization = "Bearer $env:GH_TOKEN"
        Accept = 'application/octet-stream'
        'X-GitHub-Api-Version' = '2022-11-28'
        'User-Agent' = 'EvoHime-compatible-manifest'
    }
    return Invoke-RestMethod -Uri $asset.url -Headers $headers
}

$components = foreach ($module in $moduleIds) {
    $release = Get-ModuleRelease $module
    $manifestAssetName = if ($module -eq 'listener-runtime') { 'listener-runtime.json' } else { "$module.manifest.json" }
    $manifestAsset = Get-Asset $release $manifestAssetName
    $moduleManifest = Read-AssetJson $manifestAsset
    if ($moduleManifest.module -ne $module -or $null -eq (Parse-Version ([string]$moduleManifest.version))) {
        throw "Некорректный manifest модуля $module в релизе $($release.tag_name)."
    }
    if ($module -eq 'listener-runtime') {
        [ordered]@{
            id = $module
            version = [string]$moduleManifest.version
            release_tag = $release.tag_name
            manifest_asset = $manifestAssetName
            artifact = $null
            size = 0
            sha256 = ''
            dependencies = @()
            restart = 'listener'
            protocol = 'listener-runtime-v1'
            summary = 'Библиотеки распознавания речи и модели для listener.'
            changes = @('Обновлён проверенный комплект библиотек и моделей.')
        }
    } else {
        $artifactAsset = Get-Asset $release ([string]$moduleManifest.artifact)
        [ordered]@{
            id = $module
            version = [string]$moduleManifest.version
            release_tag = $release.tag_name
            manifest_asset = $manifestAssetName
            artifact = [string]$moduleManifest.artifact
            size = [int64]$moduleManifest.size
            sha256 = ([string]$moduleManifest.sha256).ToLowerInvariant()
            dependencies = @($moduleManifest.dependencies)
            restart = [string]($moduleManifest.restart ?? 'module')
            protocol = 'desktop-ipc-v1'
            summary = [string]($moduleManifest.summary ?? "Модуль $module.")
            changes = @($moduleManifest.changes)
        }
    }
}

$updaterVersion = (Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\release-versions\updater.txt') -Raw).Trim()
if ($updaterVersion -notmatch '^\d+\.\d+\.\d+$') { throw "Некорректная минимальная версия updater: $updaterVersion" }
$manifest = [ordered]@{
    schema = 'evohime.compatible-set.v1'
    product = 'EvoHime'
    os = 'windows'
    architecture = 'x64'
    generated_from = if ($env:GITHUB_SHA) { $env:GITHUB_SHA } else { 'manual' }
    updater = [ordered]@{ minimum_version = $updaterVersion; update_first = $true }
    components = @($components)
}
$output = Join-Path $env:RUNNER_TEMP 'evohime.compatible.json'
$manifest | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $output -Encoding utf8NoBOM

$tag = 'compatibility'
$notes = Join-Path $env:RUNNER_TEMP 'compatible-manifest-notes.md'
@(
    '# Совместимый комплект EvoHime'
    ''
    'Этот fixed release содержит только манифест. Он ссылается на конкретные'
    'module releases, версии, зависимости и SHA-256; installer пересобирать не нужно.'
) | Set-Content -LiteralPath $notes -Encoding utf8NoBOM
gh release view $tag --repo $env:GITHUB_REPOSITORY 2>$null | Out-Null
if ($LASTEXITCODE -ne 0) {
    gh release create $tag --repo $env:GITHUB_REPOSITORY --title 'EvoHime compatible set' --notes-file $notes
    if ($LASTEXITCODE -ne 0) { throw 'Не удалось создать release совместимого комплекта.' }
} else {
    gh release edit $tag --repo $env:GITHUB_REPOSITORY --title 'EvoHime compatible set' --notes-file $notes
    if ($LASTEXITCODE -ne 0) { throw 'Не удалось обновить release совместимого комплекта.' }
}
gh release upload $tag --repo $env:GITHUB_REPOSITORY $output --clobber
if ($LASTEXITCODE -ne 0) { throw 'Не удалось опубликовать манифест совместимого комплекта.' }
Write-Host "Опубликован совместимый комплект: $tag"
