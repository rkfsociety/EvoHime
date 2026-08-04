[CmdletBinding()]
param(
    [ValidateRange(1, 100)]
    [int]$KeepCount = 1
)

$ErrorActionPreference = 'Stop'

if (-not $env:GITHUB_REPOSITORY) {
    throw 'GITHUB_REPOSITORY is required.'
}
if (-not $env:GH_TOKEN) {
    throw 'GH_TOKEN is required.'
}

$releases = @(gh release list --repo $env:GITHUB_REPOSITORY --limit 1000 --json tagName,publishedAt,isDraft,isPrerelease | ConvertFrom-Json)
$versioned = @(
    $releases |
        Where-Object { $_.tagName -match '^v\d+\.\d+\.\d+$' } |
        Sort-Object @{ Expression = { if ($_.publishedAt) { [datetime]$_.publishedAt } else { [datetime]::MinValue } }; Descending = $true }
)

$obsolete = @($versioned | Select-Object -Skip $KeepCount)
foreach ($release in $obsolete) {
    Write-Host "Deleting release and tag $($release.tagName)"
    gh release delete $release.tagName --repo $env:GITHUB_REPOSITORY --cleanup-tag --yes
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to delete release $($release.tagName)."
    }
}

$keptTags = @($versioned | Select-Object -First $KeepCount | ForEach-Object tagName)
$tags = @(gh api --repo $env:GITHUB_REPOSITORY --paginate 'repos/{owner}/{repo}/tags?per_page=100' --jq '.[].name')
foreach ($tag in $tags) {
    if ($tag -match '^v\d+\.\d+\.\d+$' -and $keptTags -notcontains $tag -and $obsolete.tagName -notcontains $tag) {
        Write-Host "Deleting orphaned version tag $tag"
        gh api --repo $env:GITHUB_REPOSITORY --method DELETE "repos/{owner}/{repo}/git/refs/tags/$tag"
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to delete tag $tag."
        }
    }
}

Write-Host "Release retention complete. Kept $($keptTags.Count) versioned releases."
