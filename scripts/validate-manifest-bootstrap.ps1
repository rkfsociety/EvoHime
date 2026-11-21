[CmdletBinding()]
param(
    [string]$ManifestPath,
    [string]$Repository = $(if ($env:GITHUB_REPOSITORY) { $env:GITHUB_REPOSITORY } else { 'rkfsociety/EvoHime' })
)

$ErrorActionPreference = 'Stop'

$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("evohime-bootstrap-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tempDir -Force | Out-Null

try {
    $resolvedManifestPath = $ManifestPath
    if ([string]::IsNullOrWhiteSpace($resolvedManifestPath)) {
        gh release download compatibility --repo $Repository --pattern 'evohime.compatible.json' --dir $tempDir --clobber
        if ($LASTEXITCODE -ne 0) {
            Write-Host "Не удалось скачать evohime.compatible.json из релиза compatibility ($Repository)."
            exit 1
        }
        $resolvedManifestPath = Join-Path $tempDir 'evohime.compatible.json'
    }

    if (-not (Test-Path -LiteralPath $resolvedManifestPath -PathType Leaf)) {
        Write-Host "Файл compatible manifest не найден: $resolvedManifestPath"
        exit 1
    }

    $manifest = Get-Content -LiteralPath $resolvedManifestPath -Raw -Encoding utf8 | ConvertFrom-Json
    if ($null -eq $manifest -or $null -eq $manifest.components) {
        Write-Host 'Некорректный compatible manifest: отсутствует массив components.'
        exit 1
    }
    $manifestIds = @($manifest.components | ForEach-Object { [string]$_.id })

    gh release download installer --repo $Repository --pattern 'EvoHime-Setup.json' --dir $tempDir --clobber
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Не удалось скачать EvoHime-Setup.json из релиза installer ($Repository)."
        exit 1
    }

    $setupMetaPath = Join-Path $tempDir 'EvoHime-Setup.json'
    $setupMeta = Get-Content -LiteralPath $setupMetaPath -Raw -Encoding utf8 | ConvertFrom-Json
    $installerCommit = [string]$setupMeta.commit
    if ([string]::IsNullOrWhiteSpace($installerCommit) -or $installerCommit -notmatch '^[0-9a-f]{40}$') {
        Write-Host "Некорректный commit в EvoHime-Setup.json: '$installerCommit'."
        exit 1
    }

    $encodedContent = gh api "repos/$Repository/contents/crates/evohime-update-agent/src/main.rs?ref=$installerCommit" --jq '.content'
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($encodedContent)) {
        Write-Host "Не удалось получить crates/evohime-update-agent/src/main.rs для коммита $installerCommit."
        exit 1
    }

    $cleanBase64 = ($encodedContent -replace '\s', '')
    $updaterSource = [System.Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($cleanBase64))

    $hasForwardCompat = $updaterSource -match '(?s)fn\s+validate_compatible_manifest\b.*?if\s+!MODULE_IDS\.contains\(&component\.id\.as_str\(\)\)\s*\{\s*continue;\s*\}'
    if ($hasForwardCompat) {
        Write-Host "Bootstrap check passed: updater из релиза installer ($installerCommit) поддерживает forward-compatibility."
        exit 0
    }

    $moduleListMatch = [regex]::Match($updaterSource, 'const\s+MODULE_IDS\s*:\s*&\[&str\]\s*=\s*&\[(?<items>[\s\S]*?)\];')
    if (-not $moduleListMatch.Success) {
        Write-Host "Не удалось извлечь MODULE_IDS из updater ($installerCommit)."
        exit 1
    }

    $previousModuleIds = @(
        [regex]::Matches($moduleListMatch.Groups['items'].Value, '"([a-z0-9-]+)"') |
            ForEach-Object { $_.Groups[1].Value }
    )
    if ($previousModuleIds.Count -eq 0) {
        Write-Host "Список MODULE_IDS в updater ($installerCommit) пуст."
        exit 1
    }

    $unknownModules = @($manifestIds | Where-Object { $previousModuleIds -notcontains $_ })
    if ($unknownModules.Count -gt 0 -or $manifestIds.Count -ne $previousModuleIds.Count) {
        Write-Host "Манифест содержит модули, которые не примет предыдущая версия updater'а. Требуется сначала обновить updater через installer release."
        if ($unknownModules.Count -gt 0) {
            Write-Host ("Неизвестные для updater ($installerCommit) модули: " + ($unknownModules -join ', '))
        }
        Write-Host ("Компонентов в манифесте: $($manifestIds.Count); в MODULE_IDS предыдущего updater'а: $($previousModuleIds.Count).")
        exit 1
    }

    Write-Host "Bootstrap check passed: все $($manifestIds.Count) модулей приняты бы updater'ом из релиза installer ($installerCommit)."
}
finally {
    if (Test-Path -LiteralPath $tempDir) {
        Remove-Item -LiteralPath $tempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}
