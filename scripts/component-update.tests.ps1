[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string]$UpdaterPath
)

$ErrorActionPreference = 'Stop'
$tempRoot = if ([string]::IsNullOrWhiteSpace($env:RUNNER_TEMP)) { [IO.Path]::GetTempPath() } else { $env:RUNNER_TEMP }
$root = Join-Path $tempRoot 'EvoHime-component-update-smoke'
if (Test-Path -LiteralPath $root) { Remove-Item -LiteralPath $root -Recurse -Force }
$staging = Join-Path $root 'staging'
$install = Join-Path $root 'install'
New-Item -ItemType Directory -Force -Path (Join-Path $staging 'ui-bundle'), $install | Out-Null
foreach ($component in @('EvoHime.exe','evohime-core.exe','evohime-supervisor.exe','eva.exe','evohime-analysis-worker.exe','evohime-listener.exe','evohime-transaction.exe','evohime-verify.exe')) {
    Set-Content -LiteralPath (Join-Path $install $component) -Value "old:$component"
}
Set-Content -LiteralPath (Join-Path $staging 'EvoHime.exe') -Value 'new:EvoHime.exe'
Set-Content -LiteralPath (Join-Path $staging 'ui-bundle\index.html') -Value '<!doctype html><title>component smoke</title>'
Set-Content -LiteralPath (Join-Path $staging 'ui-bundle\app.js') -Value 'window.__componentSmoke = true'
$process = Start-Process -FilePath $UpdaterPath -ArgumentList @(
    '--worker', '--apply-ui', '--staging', $staging, '--install-root', $install, '--ui-version', 'smoke-1'
) -Wait -PassThru -NoNewWindow
if ($process.ExitCode -ne 0) { throw "UI component apply failed with exit code $($process.ExitCode)." }
$pointer = Get-Content -LiteralPath (Join-Path $install 'ui-active.json') -Raw | ConvertFrom-Json
if ($pointer.version -ne 'smoke-1') { throw 'UI active pointer has an unexpected version.' }
if (-not (Test-Path -LiteralPath (Join-Path $install 'ui-bundles\smoke-1\index.html'))) { throw 'Versioned UI bundle is missing.' }
Remove-Item -LiteralPath $root -Recurse -Force
New-Item -ItemType Directory -Force -Path (Join-Path $staging 'ui-bundle'), $install | Out-Null
foreach ($component in @('EvoHime.exe','evohime-core.exe','evohime-supervisor.exe','eva.exe','evohime-analysis-worker.exe','evohime-listener.exe','evohime-transaction.exe','evohime-verify.exe')) {
    Set-Content -LiteralPath (Join-Path $install $component) -Value "old:$component"
}
Set-Content -LiteralPath (Join-Path $staging 'EvoHime.exe') -Value 'new:EvoHime.exe'
Set-Content -LiteralPath (Join-Path $staging 'ui-bundle\index.html') -Value '<!doctype html><title>mixed smoke</title>'
$process = Start-Process -FilePath $UpdaterPath -ArgumentList @('--worker','--apply-components','--staging',$staging,'--install-dir',$install,'--state-dir',(Join-Path $root 'state'),'--selected','EvoHime.exe','--ui-version','smoke-2') -Wait -PassThru -NoNewWindow
if ($process.ExitCode -ne 0) { throw "Mixed component apply failed with exit code $($process.ExitCode)." }
if ((Get-Content -LiteralPath (Join-Path $install 'evohime-core.exe') -Raw) -ne "old:evohime-core.exe`r`n") { throw 'Mixed apply changed an unselected component.' }
if (-not (Test-Path -LiteralPath (Join-Path $install 'ui-bundles\smoke-2\index.html'))) { throw 'Mixed apply UI bundle is missing.' }
Remove-Item -LiteralPath $root -Recurse -Force
Write-Output 'component update smoke: PASS (UI-only and mixed set)'
