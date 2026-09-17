[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string]$SetupPath,
    [Parameter(Mandatory)] [ValidatePattern('^\d+\.\d+\.\d+$')] [string]$Version,
    [Parameter(Mandatory)] [string]$Commit,
    [string]$Tag = 'bootstrap'
)

$ErrorActionPreference = 'Stop'
if (-not $env:GH_TOKEN) { throw 'GH_TOKEN is required.' }
$setup = (Resolve-Path $SetupPath).Path
if (-not (Test-Path -LiteralPath $setup -PathType Leaf)) { throw "Bootstrap setup missing: $setup" }
$manifestPath = Join-Path (Split-Path -Parent $setup) 'EvoHime-Setup.json'
$manifest = [ordered]@{
    version = 2
    product = 'EvoHime'
    kind = 'bootstrap'
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
$notes = Join-Path $env:TEMP 'evohime-bootstrap-release-notes.md'
Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\installer\bootstrap-release-notes.md') | Set-Content -LiteralPath $notes -Encoding utf8NoBOM

$releaseLookup = @(gh api "repos/$repo/releases/tags/$Tag" 2>&1)
$releaseExit = $LASTEXITCODE
$releaseText = $releaseLookup -join "`n"
$releaseAbsent = $releaseExit -ne 0 -and $releaseText -match '(?i)(HTTP/?[0-9.]*\s*404|HTTP 404|\(HTTP 404\)|404 Not Found)'
if ($releaseExit -ne 0 -and -not $releaseAbsent) {
    throw "Не удалось проверить bootstrap release: $releaseText"
}
if ($releaseAbsent) {
    gh release create $Tag --repo $repo --target $Commit --title "EvoHime bootstrap $Version" --notes-file $notes
} else {
    gh release edit $Tag --repo $repo --target $Commit --title "EvoHime bootstrap $Version" --notes-file $notes
}
if ($LASTEXITCODE -ne 0) { throw "Не удалось обновить bootstrap release $Tag." }
gh release upload $Tag --repo $repo $setup --clobber
if ($LASTEXITCODE -ne 0) { throw 'Не удалось опубликовать bootstrap installer.' }
gh release upload $Tag --repo $repo $manifestPath --clobber
if ($LASTEXITCODE -ne 0) { throw 'Не удалось опубликовать bootstrap manifest.' }
Write-Host "Published bootstrap installer $Version to $Tag ($($manifest.sha256))"
