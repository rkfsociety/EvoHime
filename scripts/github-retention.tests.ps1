$ErrorActionPreference = 'Stop'

$workflow = Get-Content -Raw (Join-Path $PSScriptRoot '..\.github\workflows\rust.yml')
$cleanup = Get-Content -Raw (Join-Path $PSScriptRoot 'cleanup-github-releases.ps1')

foreach ($required in @(
    'schedule:',
    'workflow_dispatch:',
    'contents: write',
    'cleanup-github-releases.ps1',
    'KeepCount',
    '--cleanup-tag',
    'refs/tags/v'
)) {
    if ($workflow -notmatch [regex]::Escape($required) -and $cleanup -notmatch [regex]::Escape($required)) {
        throw "Retention configuration is missing: $required"
    }
}

if ($cleanup -notmatch "\^v\\d\+\\.\\d\+\\.\\d\+\$") {
    throw 'Cleanup must be restricted to semantic version tags.'
}

Write-Output 'github-retention smoke: PASS'
