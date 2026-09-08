$ErrorActionPreference = 'Stop'

. "$PSScriptRoot\native-package.ps1"

$manifest = New-NativePackageManifest -Architecture 'x64' -OsMinimum 'Windows 10 2004 / Windows 11'

if ($manifest.product -ne 'EvoHime') { throw 'manifest product is not EvoHime' }
if ($manifest.client -ne 'electron-shell') { throw 'manifest client must be electron-shell' }
if ($manifest.architecture -ne 'x64') { throw 'manifest architecture must be x64' }
if ($manifest.components.core -ne 'evohime-core.exe') { throw 'core component is missing' }
if ($manifest.components.cli -ne 'eva.exe') { throw 'CLI component is missing' }
if ($manifest.components.supervisor -ne 'evohime-supervisor.exe') { throw 'supervisor component is missing' }
if ($manifest.components.analysis_worker -ne 'evohime-analysis-worker.exe') { throw 'analysis worker component is missing' }
if ($manifest.components.listener -ne 'evohime-listener.exe') { throw 'listener component is missing' }
if ($manifest.components.updater -ne 'evohime-updater.exe') { throw 'updater component is missing' }
if ($manifest.components.updater_ui -ne 'EvoHimeUpdater.exe') { throw 'Electron updater UI component is missing' }
if ($manifest.components.verifier -ne 'evohime-verify.exe') { throw 'verifier component is missing' }
if ($manifest.components.ui -ne 'EvoHime.exe') { throw 'UI component is missing' }
if ($manifest.components.browser_backend -ne 'EvoHime.exe') { throw 'browser backend component is missing' }
if ($manifest.PSObject.Properties.Name -contains 'web') { throw 'web component must not be packaged' }
if ($manifest.PSObject.Properties.Name -contains 'postgresql') { throw 'PostgreSQL must not be packaged' }

$packageRoot = Join-Path $PSScriptRoot '..\artifacts\native-test'
if (Test-Path -LiteralPath $packageRoot) {
    Remove-Item -LiteralPath $packageRoot -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $packageRoot | Out-Null
Set-Content -LiteralPath (Join-Path $packageRoot 'EvoHime.exe') -Value 'ui'
Set-Content -LiteralPath (Join-Path $packageRoot 'evohime-core.exe') -Value 'core'
Set-Content -LiteralPath (Join-Path $packageRoot 'eva.exe') -Value 'cli'
Set-Content -LiteralPath (Join-Path $packageRoot 'evohime-supervisor.exe') -Value 'supervisor'
Set-Content -LiteralPath (Join-Path $packageRoot 'evohime-analysis-worker.exe') -Value 'analysis-worker'
Set-Content -LiteralPath (Join-Path $packageRoot 'evohime-listener.exe') -Value 'listener'
Set-Content -LiteralPath (Join-Path $packageRoot 'evohime-transaction.exe') -Value 'updater'
Set-Content -LiteralPath (Join-Path $packageRoot 'evohime-updater.exe') -Value 'update-agent'
Set-Content -LiteralPath (Join-Path $packageRoot 'evohime-verify.exe') -Value 'verifier'
Set-Content -LiteralPath (Join-Path $packageRoot 'ui-bundle.zip') -Value 'ui-archive-fixture'
New-Item -ItemType Directory -Force -Path (Join-Path $packageRoot 'resources') | Out-Null
Set-Content -LiteralPath (Join-Path $packageRoot 'resources\app.asar') -Value 'electron-shell-fixture'

$commit = 'a' * 40
& (Join-Path $PSScriptRoot 'build-windows-native.ps1') -SkipBuild -OutputPath $packageRoot -Commit $commit | Out-Null
& (Join-Path $PSScriptRoot 'build-windows-native.ps1') -SkipBuild -OutputPath $packageRoot -Commit $commit | Out-Null
if (-not (Test-Path -LiteralPath (Join-Path $packageRoot 'evohime.manifest.json'))) {
    throw 'package manifest was not written'
}
if (-not (Test-Path -LiteralPath (Join-Path $packageRoot 'EvoHimeUpdater.exe'))) {
    throw 'Electron updater executable was not packaged'
}
$componentMarkerPath = Join-Path $packageRoot 'evohime.components.json'
if (-not (Test-Path -LiteralPath $componentMarkerPath)) { throw 'component manifest was not written' }
$componentMarker = Get-Content -LiteralPath $componentMarkerPath -Raw | ConvertFrom-Json
if ($componentMarker.schema -ne 'evohime.component-manifest.v1') { throw 'component manifest schema mismatch' }
if ($componentMarker.components.Count -ne 10) { throw 'component manifest inventory mismatch' }
if ($componentMarker.components[0].sha256.Length -ne 64) { throw 'component manifest hash is missing' }
if ($componentMarker.release_commit -ne $commit) { throw 'component manifest release commit mismatch' }

# Native-упаковка может только собрать уже проверенные результаты CI и не
# должна повторно запускать Cargo или Electron.
$nativeInput = Join-Path $PSScriptRoot '..\artifacts\native-input-test'
$electronInput = Join-Path $PSScriptRoot '..\artifacts\electron-input-test'
New-Item -ItemType Directory -Force -Path $nativeInput, (Join-Path $electronInput 'resources') | Out-Null
Set-Content -LiteralPath (Join-Path $nativeInput 'evohime-core.exe') -Value 'native-input'
foreach ($name in @('eva.exe', 'evohime-supervisor.exe', 'evohime-analysis-worker.exe', 'evohime-listener.exe', 'evohime-transaction.exe', 'evohime-updater.exe', 'evohime-verify.exe')) {
    Set-Content -LiteralPath (Join-Path $nativeInput $name) -Value "input:$name"
}
Set-Content -LiteralPath (Join-Path $electronInput 'EvoHime.exe') -Value 'electron-input'
Set-Content -LiteralPath (Join-Path $electronInput 'resources\app.asar') -Value 'electron-input-asar'
$inputPackage = Join-Path $PSScriptRoot '..\artifacts\native-input-package-test'
New-Item -ItemType Directory -Force -Path $inputPackage | Out-Null
Set-Content -LiteralPath (Join-Path $inputPackage 'ui-bundle.zip') -Value 'ui-input'
& (Join-Path $PSScriptRoot 'build-windows-native.ps1') -SkipBuild -NativeInputPath $nativeInput -ElectronInputPath $electronInput -OutputPath $inputPackage -Commit $commit | Out-Null
if ((Get-Content -LiteralPath (Join-Path $inputPackage 'evohime-core.exe') -Raw).Trim() -ne 'native-input') { throw 'Native CI artifact was not reused.' }
if ((Get-Content -LiteralPath (Join-Path $inputPackage 'resources\app.asar') -Raw).Trim() -ne 'electron-input-asar') { throw 'Electron CI artifact was not reused.' }
Remove-Item -LiteralPath $nativeInput, $electronInput, $inputPackage -Recurse -Force

# Маркер сборки: без него клиент не знает своей версии и пересобирается зря.
$markerPath = Join-Path $packageRoot 'evohime.build.json'
if (-not (Test-Path -LiteralPath $markerPath)) { throw 'build marker was not written' }
$marker = Get-Content -LiteralPath $markerPath -Raw | ConvertFrom-Json
if ($marker.commit -ne $commit) { throw 'build marker commit mismatch' }
if ($marker.branch -ne 'main') { throw 'build marker branch mismatch' }
if ($marker.builtAtMs -le 0) { throw 'build marker timestamp is missing' }

# Неизвестный коммит не подделывается: маркер просто не пишется.
Remove-Item -LiteralPath $markerPath -Force
& (Join-Path $PSScriptRoot 'build-windows-native.ps1') -SkipBuild -OutputPath $packageRoot -Commit 'HEAD' -WarningAction SilentlyContinue | Out-Null
if (Test-Path -LiteralPath $markerPath) { throw 'build marker must stay absent for an unknown commit' }

Remove-Item -LiteralPath $packageRoot -Recurse -Force

Write-Output 'native-package smoke: PASS'
