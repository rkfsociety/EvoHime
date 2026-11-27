Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $repoRoot 'modules.manifest.json'
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "Missing modules.manifest.json at $manifestPath"
}

$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
if ($manifest.schema -ne 'evohime.modules-manifest.v1') {
    throw "Unexpected manifest schema: $($manifest.schema)"
}

$seenIds = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
foreach ($mod in $manifest.modules) {
    if ([string]::IsNullOrWhiteSpace($mod.id)) { throw 'Module entry with empty id' }
    if (-not $seenIds.Add($mod.id)) { throw "Duplicate module id: $($mod.id)" }

    $versionPath = Join-Path $repoRoot $mod.version_file
    if (-not (Test-Path -LiteralPath $versionPath -PathType Leaf)) {
        throw "Missing version_file for '$($mod.id)': $versionPath"
    }
    $verText = (Get-Content -LiteralPath $versionPath -Raw).Trim()
    if ($verText -notmatch '^\d+\.\d+\.\d+$') {
        throw "Invalid semver in $($mod.version_file): '$verText'"
    }

    $workflowPath = Join-Path $repoRoot $mod.workflow
    if (-not (Test-Path -LiteralPath $workflowPath -PathType Leaf)) {
        throw "Missing workflow for '$($mod.id)': $workflowPath"
    }
    $wfRaw = Get-Content -LiteralPath $workflowPath -Raw
    if ($mod.kind -eq 'native' -and $wfRaw -match 'module-native\.yml') {
        if ($wfRaw -notmatch ('artifact:\s*' + [regex]::Escape($mod.artifact))) {
            throw "Workflow $($mod.workflow) artifact mismatch for '$($mod.id)' (expected $($mod.artifact))"
        }
        if ($wfRaw -notmatch ('version_file:\s*' + [regex]::Escape($mod.version_file))) {
            throw "Workflow $($mod.workflow) version_file mismatch for '$($mod.id)' (expected $($mod.version_file))"
        }
    }
}

foreach ($mod in $manifest.modules) {
    foreach ($dep in @($mod.dependencies)) {
        if (-not $seenIds.Contains($dep)) {
            throw "Module '$($mod.id)' references unknown dependency '$dep'"
        }
        if ($dep -eq $mod.id) {
            throw "Module '$($mod.id)' cannot depend on itself"
        }
    }
}

Write-Host "modules.manifest.json validated: $($manifest.modules.Count) modules OK"
