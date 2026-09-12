[CmdletBinding()]
param(
    [string]$InstallDirectory
)

$ErrorActionPreference = 'Stop'

# This is an explicitly user-triggered bridge for installations whose updater
# predates the GitHub signed-redirect fix. It replaces only the Rust updater;
# it never runs the installer and never touches user data or application files.
$repository = 'rkfsociety/EvoHime'
$dataDirectory = Join-Path $env:LOCALAPPDATA 'EvoHime'
$headers = @{
    Accept = 'application/vnd.github+json'
    'X-GitHub-Api-Version' = '2022-11-28'
    'User-Agent' = 'EvoHime-updater-repair'
}

function Parse-Version([string]$value) {
    if ($value -notmatch '^module-updater-v(\d+\.\d+\.\d+)$') { return $null }
    return [Version]::Parse($Matches[1])
}

function Assert-GitHubReleaseAssetUrl([string]$value, [string]$tag, [string]$assetName) {
    $uri = [Uri]$value
    $expectedPath = "/$repository/releases/download/$tag/$assetName"
    if ($uri.Scheme -ne 'https' -or $uri.Host -ne 'github.com' -or
        $uri.UserInfo -or $uri.Query -or $uri.Fragment -or $uri.AbsolutePath -ne $expectedPath) {
        throw "Небезопасный URL release asset: $value"
    }
}

if ([string]::IsNullOrWhiteSpace($InstallDirectory)) {
    $candidates = @(
        (Join-Path $env:LOCALAPPDATA 'Programs\EvoHime'),
        (Join-Path $env:ProgramFiles 'EvoHime')
    )
    $InstallDirectory = @($candidates | Where-Object {
        Test-Path -LiteralPath (Join-Path $_ 'evohime-updater.exe') -PathType Leaf
    } | Select-Object -First 1)
}

if ([string]::IsNullOrWhiteSpace($InstallDirectory)) {
    throw 'Не найден EvoHime. Укажите каталог установки параметром -InstallDirectory.'
}
$InstallDirectory = (Resolve-Path -LiteralPath $InstallDirectory).Path
$target = Join-Path $InstallDirectory 'evohime-updater.exe'
if (-not (Test-Path -LiteralPath $target -PathType Leaf)) {
    throw "Updater не найден: $target"
}

$releases = @(Invoke-RestMethod -Uri "https://api.github.com/repos/$repository/releases?per_page=100" -Headers $headers)
$release = @($releases | Where-Object {
    -not $_.draft -and -not $_.prerelease -and $null -ne (Parse-Version ([string]$_.tag_name))
} | Sort-Object @{ Expression = { Parse-Version ([string]$_.tag_name) }; Descending = $true } | Select-Object -First 1)
if ($release.Count -eq 0) { throw 'Опубликованный release updater не найден.' }
$release = $release[0]
$tag = [string]$release.tag_name
$manifestAsset = @($release.assets | Where-Object { $_.name -eq 'updater.manifest.json' }) | Select-Object -First 1
$binaryAsset = @($release.assets | Where-Object { $_.name -eq 'evohime-updater.exe' }) | Select-Object -First 1
if ($null -eq $manifestAsset -or $null -eq $binaryAsset) {
    throw "В release $tag отсутствует updater или его manifest."
}
Assert-GitHubReleaseAssetUrl ([string]$manifestAsset.browser_download_url) $tag 'updater.manifest.json'
Assert-GitHubReleaseAssetUrl ([string]$binaryAsset.browser_download_url) $tag 'evohime-updater.exe'

$manifest = Invoke-RestMethod -Uri $manifestAsset.browser_download_url -Headers $headers -MaximumRedirection 5
$expectedVersion = [string]$manifest.version
$expectedArtifact = [string]$manifest.artifact
$expectedSize = [int64]$manifest.size
$expectedHash = ([string]$manifest.sha256).ToLowerInvariant()
if ($expectedVersion -ne $tag.Substring('module-updater-v'.Length) -or
    $expectedArtifact -ne 'evohime-updater.exe' -or $expectedSize -le 0 -or
    $expectedHash -notmatch '^[0-9a-f]{64}$') {
    throw "Некорректный updater manifest в release $tag."
}

$running = @(Get-CimInstance Win32_Process -Filter "Name = 'evohime-updater.exe'" -ErrorAction SilentlyContinue |
    Where-Object { $_.ExecutablePath -and ((Resolve-Path -LiteralPath $_.ExecutablePath -ErrorAction SilentlyContinue).Path -eq $target) })
if ($running.Count -gt 0) {
    throw 'Закройте окно EvoHime и повторите recovery: updater сейчас запущен.'
}

$stateDirectory = Join-Path $dataDirectory 'update-state'
$temporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) ("EvoHime-updater-repair-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $stateDirectory, $temporaryDirectory | Out-Null
$download = Join-Path $temporaryDirectory 'evohime-updater.exe'
try {
    Invoke-WebRequest -Uri $binaryAsset.browser_download_url -Headers $headers -MaximumRedirection 5 -OutFile $download
    $downloaded = Get-Item -LiteralPath $download
    if ($downloaded.Length -ne $expectedSize) {
        throw "Размер updater не совпал: получено $($downloaded.Length), ожидалось $expectedSize."
    }
    $hash = (Get-FileHash -LiteralPath $download -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($hash -ne $expectedHash) { throw 'SHA-256 updater не совпал с release manifest.' }
    $header = [IO.File]::ReadAllBytes($download)
    if ($header.Length -lt 2 -or $header[0] -ne 0x4d -or $header[1] -ne 0x5a) {
        throw 'Скачанный updater не является Windows executable.'
    }

    $backup = Join-Path $stateDirectory ("updater-recovery-previous-" + [DateTime]::UtcNow.ToString('yyyyMMddHHmmss') + '.exe')
    [IO.File]::Replace($download, $target, $backup, $true)
    Write-Host "Updater восстановлен: версия $expectedVersion"
    Write-Host "Резервная копия старого updater: $backup"
    Write-Host 'Теперь снова запустите ярлык EvoHime.'
}
finally {
    if (Test-Path -LiteralPath $temporaryDirectory) {
        Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force -ErrorAction SilentlyContinue
    }
}
