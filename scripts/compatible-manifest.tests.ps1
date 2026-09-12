$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$script = Get-Content -LiteralPath (Join-Path $root 'scripts\publish-compatible-manifest.ps1') -Raw
$workflow = Get-Content -LiteralPath (Join-Path $root '.github\workflows\compatible-manifest.yml') -Raw
foreach ($marker in @(
    "schema = 'evohime.compatible-set.v1'",
    'gh api --paginate --slurp',
    '-not $_.draft',
    '-not $_.prerelease',
    'release_tag = $release.tag_name',
    'minimum_version = $updaterVersion',
    'sha256 = ([string]$moduleManifest.sha256).ToLowerInvariant()',
    'Normalize-StringArray',
    'if ($null -eq $value) { return $result.ToArray() }',
    'null-элемент недопустим',
    "dependencies = [string[]]@(Normalize-StringArray",
    "changes = [string[]]@(Normalize-StringArray",
    'gh release upload $tag'
)) {
    if ($script -notmatch [regex]::Escape($marker)) { throw "Compatible manifest contract is missing: $marker" }
}
if ($workflow -notmatch 'workflow_run:' -or $workflow -notmatch 'workflow_dispatch:') {
    throw 'Compatible manifest workflow must support automatic and manual publication.'
}
if ($workflow -notmatch 'cancel-in-progress: false') { throw 'Compatible manifest publication must be serialized.' }
Write-Output 'compatible manifest smoke: PASS'
