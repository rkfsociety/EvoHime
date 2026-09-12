$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$router = Get-Content -LiteralPath (Join-Path $root '.github\workflows\module-router.yml') -Raw
if ($router -notmatch 'name: module router') { throw 'Central module router is missing.' }
if ($router -match '(?m)^\s+uses:\s+\.\/\.github/workflows/') { throw 'Router still nests module workflows instead of dispatching separate runs.' }
if ($router -notmatch 'gh workflow run') { throw 'Router does not dispatch independent module workflow runs.' }
if ($router -match 'git diff|git log|github\.event\.before|MANUAL_BASE|base:') { throw 'Router still uses commit/path diff as the production release criterion.' }
if ($router -notmatch 'module-\$module-v|Latest-Version|release-versions/\$module\.txt') { throw 'Router does not compare module versions with module releases.' }
if ($router -notmatch 'release-versions/installer\.txt') { throw 'Router does not track installer version.' }
if ($router -notmatch "'installer' = 'windows\.yml'") { throw 'Router does not dispatch the installer workflow.' }
if ($router -notmatch 'probeOutput = @\(gh api') { throw 'Router does not probe the installer release explicitly.' }
if ($router -notmatch 'releaseAbsent.*HTTP.*404') { throw 'Router does not distinguish a missing installer release from other API failures.' }
if ($router -notmatch 'Не удалось проверить installer release') { throw 'Router does not fail closed on installer release network errors.' }
if ($router -match 'gh release download installer.*2>\$null') { throw 'Router hides installer release errors.' }
foreach ($workflow in @('core.yml','supervisor.yml','cli.yml','analysis-worker.yml','listener-module.yml','listener.yml','transaction.yml','verifier.yml','shell-host.yml','ui-bundle.yml','update-agent.yml')) {
    $text = Get-Content -LiteralPath (Join-Path $root ".github\workflows\$workflow") -Raw
    if ($text -match '(?m)^\s{2}push:') { throw "$workflow still has a direct push trigger." }
    if ($text -notmatch '(?m)^\s{2}workflow_call:') { throw "$workflow is not reusable." }
    if ($text -notmatch '(?m)^\s{2}workflow_dispatch:') { throw "$workflow cannot be dispatched separately." }
    if ($text -notmatch '(?m)^concurrency:\s*$') { throw "$workflow has no concurrency block." }
    if ($text -notmatch [regex]::Escape('group: evohime-module-${{ github.workflow }}')) { throw "$workflow does not isolate its own concurrency group." }
    if ($text -notmatch '(?m)^\s+cancel-in-progress:\s*true\s*$') { throw "$workflow does not cancel the previous module run." }
}
$shellWorkflow = Get-Content -LiteralPath (Join-Path $root '.github\workflows\shell-host.yml') -Raw
if ($shellWorkflow -notmatch 'shell-host\.zip') { throw 'Shell host workflow does not publish a complete archive.' }
if ($shellWorkflow -notmatch 'resources\\app\.asar') { throw 'Shell host workflow does not assert app.asar delivery.' }
$nativeWorkflow = Get-Content -LiteralPath (Join-Path $root '.github\workflows\windows.yml') -Raw
if ($nativeWorkflow -notmatch 'actions/download-artifact@v8' -or $nativeWorkflow -notmatch 'build-windows-native\.ps1 -SkipBuild') { throw 'Native workflow still rebuilds instead of consuming checked artifacts.' }
$installer = Get-Content -LiteralPath (Join-Path $root '.github\workflows\windows.yml') -Raw
if ($installer -match '(?m)^\s{2}push:') { throw 'installer workflow still has a direct push trigger.' }
if ($installer -notmatch '(?m)^\s{2}workflow_call:\s*$') { throw 'installer workflow is not reusable like the other module workflows.' }
if ($installer -notmatch "github\.event_name == 'workflow_dispatch'") { throw 'installer publish job is not enabled for routed runs.' }
if ($installer -notmatch "github\.event_name == 'workflow_call'") { throw 'installer publish job is not enabled for reusable runs.' }
if ($installer -notmatch 'github\.ref == .refs/heads/main') { throw 'installer publish job is not restricted to main.' }
if ($installer -notmatch [regex]::Escape('group: evohime-installer')) { throw 'installer workflow does not use a fixed concurrency group.' }
if ($installer -notmatch '(?m)^\s+cancel-in-progress:\s*true\s*$') { throw 'installer workflow does not cancel the previous run.' }
if ($installer -notmatch 'release-versions\\installer\.txt') { throw 'installer workflow does not consume the canonical installer version.' }
if ($installer -notmatch 'gh release upload \$env:RELEASE_TAG installer-output\\EvoHime-Setup\.exe') { throw 'installer workflow does not publish the setup asset.' }
if ($installer -notmatch 'gh release upload \$env:RELEASE_TAG installer-output\\EvoHime-Setup\.json') { throw 'installer workflow does not publish the setup manifest.' }
foreach ($module in @('shell-host','ui-bundle','core','supervisor','cli','analysis-worker','listener','listener-runtime','transaction','verifier','updater')) {
    $expected = $module + ': ${{ steps.select.outputs.' + $module + ' }}'
    if ($router -notmatch [regex]::Escape($expected)) { throw "Router output is missing: $module" }
}
if ($router -notmatch 'MANUAL_MODULES') { throw 'Router does not expose an explicit manual module override.' }
$dispatchBlock = ($router -split '(?m)^  dispatch:', 2)[1]
if ($dispatchBlock -notmatch 'Resolve-DispatchedRun') { throw 'Router does not resolve dispatched module run ids.' }
if ($dispatchBlock -notmatch 'gh run view') { throw 'Router does not wait for dispatched module workflows.' }
if ($dispatchBlock -notmatch 'conclusion -ne .success') { throw 'Router does not fail when a dispatched module workflow fails.' }
if ($dispatchBlock -notmatch 'gh workflow run compatible-manifest\.yml') { throw 'Router does not publish the compatibility manifest after module workflows.' }
foreach ($module in @('shell-host','ui-bundle','core','supervisor','cli','analysis-worker','listener','listener-runtime','transaction','verifier','updater')) {
    if ($dispatchBlock -notmatch [regex]::Escape("'$module'")) { throw "Dispatch map is missing: $module" }
}
Write-Host 'module-router smoke tests passed.'
