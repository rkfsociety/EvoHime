[CmdletBinding()]
param(
    [switch]$GenerateOnly,
    [switch]$PublishOnly
)

$ErrorActionPreference = 'Stop'
if ($GenerateOnly -and $PublishOnly) {
    throw 'Параметры -GenerateOnly и -PublishOnly нельзя использовать одновременно.'
}
if (-not $env:GH_TOKEN) { throw 'GH_TOKEN is required.' }
if (-not $env:GITHUB_REPOSITORY) { throw 'GITHUB_REPOSITORY is required.' }

. (Join-Path $PSScriptRoot 'gh-retry.ps1')

$output = Join-Path $env:RUNNER_TEMP 'evohime.compatible.json'

if (-not $PublishOnly) {
$modulesManifestPath = Join-Path $PSScriptRoot '..\modules.manifest.json'
if (-not (Test-Path -LiteralPath $modulesManifestPath -PathType Leaf)) {
    throw "Не найден modules.manifest.json: $modulesManifestPath"
}
$modulesManifest = Get-Content -LiteralPath $modulesManifestPath -Raw -Encoding utf8 | ConvertFrom-Json
$moduleIds = @($modulesManifest.modules | Where-Object { $_.id -ne 'installer' } | ForEach-Object { [string]$_.id })
if ($moduleIds.Count -eq 0) {
    throw 'В modules.manifest.json не найдено ни одного модуля.'
}
$releasePages = Invoke-WithRetry -FailureMessage 'Не удалось получить список модульных релизов.' -Command {
    $rawPages = Invoke-GhRaw -Command {
        gh api --paginate --slurp "repos/$env:GITHUB_REPOSITORY/releases?per_page=100"
    }
    $jsonText = ($rawPages | ForEach-Object { "$_" }) -join "`n"
    if ([string]::IsNullOrWhiteSpace($jsonText)) {
        throw 'Пустой ответ GitHub API при получении списка релизов.'
    }
    return ,@($jsonText | ConvertFrom-Json)
}
$releases = @($releasePages | ForEach-Object { $_ })

function Parse-Version([string]$value) {
    if ($value -notmatch '^\d+\.\d+\.\d+$') { return $null }
    return [Version]::Parse($value)
}

function Get-ModuleRelease([string]$module) {
    $prefix = "module-$module-v"
    $candidates = @($releases | Where-Object {
        $_.tag_name -is [string] -and
        -not $_.draft -and
        -not $_.prerelease -and
        $_.tag_name.StartsWith($prefix) -and
        $null -ne (Parse-Version $_.tag_name.Substring($prefix.Length))
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
    return Invoke-WithRetry -FailureMessage "Не удалось загрузить asset $($asset.name)." -Command {
        Invoke-RestMethod -Uri $asset.url -Headers $headers
    }
}

function Normalize-StringArray($value, [string]$field, [string]$module) {
    $result = [System.Collections.Generic.List[string]]::new()
    # Не даём PowerShell развернуть пустой результат функции в $null.
    # Коллекция возвращается одним pipeline-объектом, а в JSON превращается
    # в настоящий [] даже при отсутствии исходного поля.
    if ($null -eq $value) { return ,$result }
    foreach ($item in @($value)) {
        if ($null -eq $item) {
            throw "Некорректное поле $field в manifest модуля ${module}: null-элемент недопустим."
        }
        if ($item -isnot [string]) {
            throw "Некорректное поле $field в manifest модуля ${module}: ожидался массив строк."
        }
        [void]$result.Add([string]$item)
    }
    return ,$result
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
            dependencies = [string[]]@()
            restart = 'listener'
            protocol = 'listener-runtime-v1'
            summary = 'Библиотеки распознавания речи и модели для listener.'
            changes = [string[]]@('Обновлён проверенный комплект библиотек и моделей.')
        }
    } else {
        $dependencies = (Normalize-StringArray $moduleManifest.dependencies 'dependencies' $module).ToArray()
        $changes = (Normalize-StringArray $moduleManifest.changes 'changes' $module).ToArray()
        $artifactAsset = Get-Asset $release ([string]$moduleManifest.artifact)
        [ordered]@{
            id = $module
            version = [string]$moduleManifest.version
            release_tag = $release.tag_name
            manifest_asset = $manifestAssetName
            artifact = [string]$moduleManifest.artifact
            size = [int64]$moduleManifest.size
            sha256 = ([string]$moduleManifest.sha256).ToLowerInvariant()
            dependencies = $dependencies
            restart = [string]($moduleManifest.restart ?? 'module')
            protocol = 'desktop-ipc-v1'
            summary = [string]($moduleManifest.summary ?? "Модуль $module.")
            changes = $changes
        }
    }
}

$updaterVersion = (Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\release-versions\updater.txt') -Raw).Trim()
if ($updaterVersion -notmatch '^\d+\.\d+\.\d+$') { throw "Некорректная минимальная версия updater: $updaterVersion" }
$updaterModules = @($components | Where-Object { $_.id -eq 'updater' })
$requiredUpdaterVersion = Parse-Version $updaterVersion
$publishedUpdaterVersion = if ($updaterModules.Count -eq 1) { Parse-Version ([string]$updaterModules[0].version) } else { $null }
if ($null -eq $publishedUpdaterVersion -or $publishedUpdaterVersion -lt $requiredUpdaterVersion) {
    $publishedVersion = if ($updaterModules.Count -eq 1) { [string]$updaterModules[0].version } else { 'отсутствует' }
    throw "Compatible manifest не опубликован: версия updater $publishedVersion ниже обязательной $updaterVersion."
}
$manifest = [ordered]@{
    schema = 'evohime.compatible-set.v1'
    product = 'EvoHime'
    os = 'windows'
    architecture = 'x64'
    generated_from = if ($env:GITHUB_SHA) { $env:GITHUB_SHA } else { 'manual' }
    updater = [ordered]@{ minimum_version = $updaterVersion; update_first = $true }
    components = @($components)
}
foreach ($component in @($manifest.components)) {
    foreach ($field in @('dependencies', 'changes')) {
        $value = $component[$field]
        if ($null -eq $value -or $value -isnot [array]) {
            throw "Сформированный compatible manifest содержит не-массив $field для модуля $($component.id)."
        }
        foreach ($item in @($value)) {
            if ($null -eq $item -or $item -isnot [string]) {
                throw "Сформированный compatible manifest содержит некорректный элемент $field для модуля $($component.id)."
            }
        }
    }
}
$manifest | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $output -Encoding utf8NoBOM
Write-Host "Сформирован совместимый комплект: $output"
}

if (-not $GenerateOnly) {
if (-not (Test-Path -LiteralPath $output -PathType Leaf)) {
    throw "Не найден сформированный manifest для публикации: $output"
}

$tag = 'compatibility'
$notes = Join-Path $env:RUNNER_TEMP 'compatible-manifest-notes.md'
@(
    '# Совместимый комплект EvoHime'
    ''
    'Этот fixed release содержит только манифест. Он ссылается на конкретные'
    'module releases, версии, зависимости и SHA-256; installer пересобирать не нужно.'
) | Set-Content -LiteralPath $notes -Encoding utf8NoBOM
$releaseExists = $true
$null = Invoke-GhWithRetry -FailureMessage "Не удалось проверить release $tag." -AllowNotFound -Command {
    gh release view $tag --repo $env:GITHUB_REPOSITORY
}
if ($LASTEXITCODE -ne 0) {
    $releaseExists = $false
}
if (-not $releaseExists) {
    Invoke-GhWithRetry -FailureMessage 'Не удалось создать release совместимого комплекта.' -Command {
        gh release create $tag --repo $env:GITHUB_REPOSITORY --title $tag --notes-file $notes --latest=false
    } | Out-Null
} else {
    Invoke-GhWithRetry -FailureMessage 'Не удалось обновить release совместимого комплекта.' -Command {
        gh release edit $tag --repo $env:GITHUB_REPOSITORY --title $tag --notes-file $notes --latest=false
    } | Out-Null
}
Invoke-GhWithRetry -FailureMessage 'Не удалось опубликовать манифест совместимого комплекта.' -Command {
    gh release upload $tag --repo $env:GITHUB_REPOSITORY $output --clobber
} | Out-Null
./scripts/mark-installer-release-latest.ps1 -Repository $env:GITHUB_REPOSITORY
Write-Host "Опубликован совместимый комплект: $tag"
}
