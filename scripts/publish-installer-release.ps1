[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string]$SetupPath,
    [Parameter(Mandatory)] [ValidatePattern('^\d+\.\d+\.\d+$')] [string]$Version,
    [Parameter(Mandatory)] [string]$Commit,
    [string]$Tag = 'installer'
)

$ErrorActionPreference = 'Stop'
if (-not $env:GH_TOKEN) { throw 'GH_TOKEN is required.' }
$setup = (Resolve-Path $SetupPath).Path
if (-not (Test-Path -LiteralPath $setup -PathType Leaf)) { throw "Installer setup missing: $setup" }
$manifestPath = Join-Path (Split-Path -Parent $setup) 'EvoHime-Setup.json'
$manifest = [ordered]@{
    version = 2
    product = 'EvoHime'
    kind = 'web-installer'
    installerVersion = $Version
    asset = 'EvoHime-Setup.exe'
    commit = $Commit
    branch = 'main'
    size = (Get-Item -LiteralPath $setup).Length
    sha256 = (Get-FileHash -LiteralPath $setup -Algorithm SHA256).Hash.ToLowerInvariant()
    nextStep = 'download-compatible-modules'
}
$manifest | ConvertTo-Json | Set-Content -LiteralPath $manifestPath -Encoding utf8NoBOM
$repo = if ($env:GITHUB_REPOSITORY) { $env:GITHUB_REPOSITORY } else { (gh repo view --json nameWithOwner --jq .nameWithOwner) }
$notes = Join-Path $env:TEMP 'evohime-installer-release-notes.md'
Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\installer\release-notes.md') | Set-Content -LiteralPath $notes -Encoding utf8NoBOM

$releaseLookup = @(gh api "repos/$repo/releases/tags/$Tag" 2>&1)
$releaseExit = $LASTEXITCODE
$releaseText = $releaseLookup -join "`n"
$releaseAbsent = $releaseExit -ne 0 -and $releaseText -match '(?i)(HTTP/?[0-9.]*\s*404|HTTP 404|\(HTTP 404\)|404 Not Found)'
if ($releaseExit -ne 0 -and -not $releaseAbsent) {
    throw "Не удалось проверить installer release: $releaseText"
}
if (-not $releaseAbsent) {
    try {
        $release = $releaseText | ConvertFrom-Json
    } catch {
        throw "Не удалось разобрать installer release: $releaseText"
    }
    $releaseId = [string]$release.id
    if ($releaseId -notmatch '^\d+$') { throw "Installer release не содержит корректный id: $releaseText" }
    gh api --method DELETE "repos/$repo/releases/$releaseId"
    if ($LASTEXITCODE -ne 0) { throw "Не удалось удалить старый installer release $Tag." }
}
gh release create $Tag --repo $repo --target $Commit --title "EvoHime installer $Version" --notes-file $notes
if ($LASTEXITCODE -ne 0) { throw "Не удалось создать installer release $Tag." }
gh release upload $Tag --repo $repo $setup --clobber
if ($LASTEXITCODE -ne 0) { throw 'Не удалось опубликовать web installer.' }
gh release upload $Tag --repo $repo $manifestPath --clobber
if ($LASTEXITCODE -ne 0) { throw 'Не удалось опубликовать installer manifest.' }
Write-Host "Published web installer $Version to $Tag ($($manifest.sha256))"
