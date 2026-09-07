$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$router = Get-Content -LiteralPath (Join-Path $root '.github\workflows\module-router.yml') -Raw
if ($router -notmatch 'name: module router') { throw 'Central module router is missing.' }
foreach ($workflow in @('core.yml','supervisor.yml','cli.yml','analysis-worker.yml','listener-module.yml','listener.yml','transaction.yml','verifier.yml','shell-host.yml','ui-bundle.yml')) {
    $text = Get-Content -LiteralPath (Join-Path $root ".github\workflows\$workflow") -Raw
    if ($text -match '(?m)^\s{2}push:') { throw "$workflow still has a direct push trigger." }
    if ($text -notmatch '(?m)^\s{2}workflow_call:') { throw "$workflow is not reusable." }
}
foreach ($module in @('shell-host','ui-bundle','core','supervisor','cli','analysis-worker','listener','listener-runtime','transaction','verifier')) {
    $expected = '{0}: ${{ steps.select.outputs.{0} }}' -f $module
    if ($router -notmatch [regex]::Escape($expected)) { throw "Router output is missing: $module" }
}
Write-Host 'module-router smoke tests passed.'
