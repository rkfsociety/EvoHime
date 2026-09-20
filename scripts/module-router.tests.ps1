$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$router = Get-Content -LiteralPath (Join-Path $root '.github\workflows\module-router.yml') -Raw
if ($router -notmatch 'name: module router') { throw 'Central module router is missing.' }
if ($router -match '(?m)^\s+uses:\s+\.\/\.github/workflows/') { throw 'Router still nests module workflows instead of dispatching separate runs.' }
if ($router -notmatch 'gh workflow run') { throw 'Router does not dispatch independent module workflow runs.' }
if ($router -match 'git diff|git log|github\.event\.before|MANUAL_BASE|base:') { throw 'Router still uses commit/path diff as the production release criterion.' }
if ($router -notmatch 'module-\$module-v|Latest-Version|release-versions/\$module\.txt') { throw 'Router does not compare module versions with module releases.' }
if ($router -notmatch 'release-versions/installer\.txt') { throw 'Router does not track installer version.' }
if ($router -notmatch "'installer' = 'installer\.yml'") { throw 'Router does not dispatch the web installer workflow.' }
if ($router -notmatch 'installerTag = .installer.') { throw 'Router does not probe the single installer release explicitly.' }
if ($router -notmatch 'probeOutput = @\(gh api') { throw 'Router does not probe the installer release explicitly.' }
if ($router -notmatch 'releaseAbsent.*HTTP.*404') { throw 'Router does not distinguish a missing installer release from other API failures.' }
if ($router -notmatch 'Не удалось проверить installer release') { throw 'Router does not fail closed on installer release network errors.' }
if ($router -notmatch '\$releaseAbsent\) \{ \$global:LASTEXITCODE = 0 \}') { throw 'Router does not clear the expected 404 exit code.' }
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
$cliWorkflow = Get-Content -LiteralPath (Join-Path $root '.github\workflows\cli.yml') -Raw
if ($cliWorkflow -notmatch '(?m)^\s+needs:\s+linux-contract\s*$') { throw 'CLI publication is not gated by the Linux contract job.' }
$shellWorkflow = Get-Content -LiteralPath (Join-Path $root '.github\workflows\shell-host.yml') -Raw
if ($shellWorkflow -notmatch 'shell-host\.zip') { throw 'Shell host workflow does not publish a complete archive.' }
if ($shellWorkflow -notmatch 'resources\\app\.asar') { throw 'Shell host workflow does not assert app.asar delivery.' }
$coreWorkflow = Get-Content -LiteralPath (Join-Path $root '.github\workflows\core.yml') -Raw
if ($coreWorkflow -notmatch "crates/model-gateway/\*\*") { throw 'Core workflow does not watch the model-gateway crate.' }
if ($coreWorkflow -notmatch "crates/permissions/\*\*") { throw 'Core workflow does not watch the permissions crate.' }
if ($coreWorkflow -match 'crates/evohime-model-gateway|crates/evohime-permissions') { throw 'Core workflow contains stale crate paths.' }
$moduleRelease = Get-Content -LiteralPath (Join-Path $root 'scripts\module-release.ps1') -Raw
if ($moduleRelease -notmatch "'crates/model-gateway'") { throw 'Core module release routing does not include model-gateway.' }
if ($moduleRelease -notmatch "'crates/permissions'") { throw 'Core module release routing does not include permissions.' }
if ($moduleRelease -notmatch "'crates/evohime-cli-protocol'") { throw 'CLI module release routing does not include evohime-cli-protocol.' }
if ($moduleRelease -match 'crates/evohime-model-gateway|crates/evohime-permissions') { throw 'Core module release routing contains stale crate paths.' }
$nativeWorkflow = Get-Content -LiteralPath (Join-Path $root '.github\workflows\windows.yml') -Raw
if ($nativeWorkflow -notmatch 'actions/download-artifact@v8' -or $nativeWorkflow -notmatch 'build-windows-native\.ps1 -SkipBuild') { throw 'Native workflow still rebuilds instead of consuming checked artifacts.' }
$installer = Get-Content -LiteralPath (Join-Path $root '.github\workflows\installer.yml') -Raw
if ($installer -match '(?m)^\s{2}push:') { throw 'web installer workflow still has a direct push trigger.' }
if ($installer -notmatch '(?m)^\s{2}workflow_call:\s*$') { throw 'web installer workflow is not reusable.' }
if ($installer -notmatch "github\.event_name == 'workflow_dispatch'") { throw 'web installer publish job is not enabled for routed runs.' }
if ($installer -notmatch "github\.event_name == 'workflow_call'") { throw 'web installer publish job is not enabled for reusable runs.' }
if ($installer -notmatch 'github\.ref == .refs/heads/main') { throw 'web installer publish job is not restricted to main.' }
if ($installer -notmatch [regex]::Escape('group: evohime-web-installer')) { throw 'web installer workflow does not use a fixed concurrency group.' }
if ($installer -notmatch '(?m)^\s+cancel-in-progress:\s*true\s*$') { throw 'web installer workflow does not cancel the previous run.' }
if ($installer -notmatch 'release-versions\\installer\.txt') { throw 'web installer workflow does not consume the canonical installer version.' }
if ($installer -notmatch 'build-installer-source\.ps1') { throw 'web installer workflow does not build the minimal source.' }
if ($installer -notmatch 'installer-content\.tests\.ps1') { throw 'web installer workflow misses the content gate.' }
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
