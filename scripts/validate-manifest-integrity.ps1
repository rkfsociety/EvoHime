[CmdletBinding()]
param(
    [string]$ManifestPath,
    [string]$ReleaseVersionsDir = (Join-Path $PSScriptRoot '..\release-versions'),
    [string]$Repository = $(if ($env:GITHUB_REPOSITORY) { $env:GITHUB_REPOSITORY } else { 'rkfsociety/EvoHime' })
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $ReleaseVersionsDir -PathType Container)) {
    Write-Host "Каталог release-versions не найден: $ReleaseVersionsDir"
    exit 1
}

$versionFiles = @(
    Get-ChildItem -LiteralPath $ReleaseVersionsDir -Filter '*.txt' -File |
        Where-Object { $_.Name -ne 'installer.txt' } |
        Sort-Object Name
)

if ($versionFiles.Count -eq 0) {
    Write-Host "В каталоге $ReleaseVersionsDir не найдено файлов модулей."
    exit 1
}

$downloadedTempDir = $null
try {
    $resolvedManifestPath = $ManifestPath
    if ([string]::IsNullOrWhiteSpace($resolvedManifestPath)) {
        $downloadedTempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("evohime-manifest-" + [System.Guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $downloadedTempDir -Force | Out-Null
        gh release download compatibility --repo $Repository --pattern 'evohime.compatible.json' --dir $downloadedTempDir --clobber
        if ($LASTEXITCODE -ne 0) {
            Write-Host "Не удалось скачать evohime.compatible.json из релиза compatibility ($Repository)."
            exit 1
        }
        $resolvedManifestPath = Join-Path $downloadedTempDir 'evohime.compatible.json'
    }

    if (-not (Test-Path -LiteralPath $resolvedManifestPath -PathType Leaf)) {
        Write-Host "Файл compatible manifest не найден: $resolvedManifestPath"
        exit 1
    }

    $manifest = Get-Content -LiteralPath $resolvedManifestPath -Raw -Encoding utf8 | ConvertFrom-Json
    if ($null -eq $manifest -or $null -eq $manifest.components) {
        Write-Host "Некорректный compatible manifest: отсутствует массив components."
        exit 1
    }

    $componentIds = @($manifest.components | ForEach-Object { [string]$_.id })
    $missing = [System.Collections.Generic.List[string]]::new()

    foreach ($file in $versionFiles) {
        $moduleName = [System.IO.Path]::GetFileNameWithoutExtension($file.Name)
        if ($componentIds -notcontains $moduleName) {
            Write-Host "Модуль $moduleName есть в release-versions, но отсутствует в compatible manifest!"
            [void]$missing.Add($moduleName)
        }
    }

    if ($missing.Count -gt 0) {
        exit 1
    }

    Write-Host "Manifest integrity check passed ($($versionFiles.Count) modules verified)."
}
finally {
    if ($downloadedTempDir -and (Test-Path -LiteralPath $downloadedTempDir)) {
        Remove-Item -LiteralPath $downloadedTempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}
