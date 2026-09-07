param(
    [Parameter(Mandatory)] [ValidatePattern('^[a-z0-9][a-z0-9-]*$')] [string]$Module,
    [Parameter(Mandatory)] [ValidatePattern('^\d+\.\d+\.\d+$')] [string]$Version,
    [Parameter(Mandatory)] [string]$Artifact,
    [string]$NotesFile = 'installer/release-notes.md'
)

$ErrorActionPreference = 'Stop'
if (-not $env:GH_TOKEN) { throw 'GH_TOKEN is required.' }
if (-not (Test-Path -LiteralPath $Artifact -PathType Leaf)) { throw "Module artifact missing: $Artifact" }

$repo = if ($env:GITHUB_REPOSITORY) { $env:GITHUB_REPOSITORY } else { (gh repo view --json nameWithOwner --jq .nameWithOwner) }
$tag = "module-$Module"
$artifactPath = (Resolve-Path -LiteralPath $Artifact).Path
$artifactName = Split-Path -Leaf $artifactPath
$hash = (Get-FileHash -LiteralPath $artifactPath -Algorithm SHA256).Hash.ToLowerInvariant()
$size = [int64](Get-Item -LiteralPath $artifactPath).Length
$manifestPath = Join-Path $env:RUNNER_TEMP "$Module.manifest.json"
$manifest = [ordered]@{
    schema = 'evohime.module-release.v1'
    module = $Module
    version = $Version
    artifact = $artifactName
    size = $size
    sha256 = $hash
    release_tag = $tag
    dependencies = @()
    restart = 'module'
}
$manifest | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $manifestPath -Encoding utf8NoBOM

gh release view $tag --repo $repo 2>$null | Out-Null
if ($LASTEXITCODE -ne 0) {
    gh release create $tag --repo $repo --title "EvoHime — $Module $Version" --notes-file $NotesFile
} else {
    gh release edit $tag --repo $repo --title "EvoHime — $Module $Version" --notes-file $NotesFile
}
gh release upload $tag --repo $repo $artifactPath --clobber
if ($LASTEXITCODE -ne 0) { throw "Failed to publish $artifactName." }
gh release upload $tag --repo $repo $manifestPath --clobber
if ($LASTEXITCODE -ne 0) { throw "Failed to publish module manifest." }
Write-Host "Published $Module $Version to $tag ($hash)"
