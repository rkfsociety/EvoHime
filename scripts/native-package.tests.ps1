$ErrorActionPreference = 'Stop'

. "$PSScriptRoot\native-package.ps1"

$manifest = New-NativePackageManifest -Architecture 'x64' -OsMinimum 'Windows 10 2004 / Windows 11'

if ($manifest.product -ne 'EvoHime') { throw 'manifest product is not EvoHime' }
if ($manifest.client -ne 'electron-shell') { throw 'manifest client must be electron-shell' }
if ($manifest.architecture -ne 'x64') { throw 'manifest architecture must be x64' }
if ($manifest.components.core -ne 'evohime-core.exe') { throw 'core component is missing' }
if ($manifest.components.supervisor -ne 'evohime-supervisor.exe') { throw 'supervisor component is missing' }
if ($manifest.components.updater -ne 'evohime-transaction.exe') { throw 'updater component is missing' }
if ($manifest.components.ui -ne 'EvoHime.exe') { throw 'UI component is missing' }
if ($manifest.PSObject.Properties.Name -contains 'web') { throw 'web component must not be packaged' }
if ($manifest.PSObject.Properties.Name -contains 'postgresql') { throw 'PostgreSQL must not be packaged' }

$packageRoot = Join-Path $PSScriptRoot '..\artifacts\native-test'
if (Test-Path -LiteralPath $packageRoot) {
    Remove-Item -LiteralPath $packageRoot -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $packageRoot | Out-Null
Set-Content -LiteralPath (Join-Path $packageRoot 'EvoHime.exe') -Value 'ui'
Set-Content -LiteralPath (Join-Path $packageRoot 'evohime-core.exe') -Value 'core'
Set-Content -LiteralPath (Join-Path $packageRoot 'evohime-supervisor.exe') -Value 'supervisor'
Set-Content -LiteralPath (Join-Path $packageRoot 'evohime-transaction.exe') -Value 'updater'

$commit = 'a' * 40
& (Join-Path $PSScriptRoot 'build-windows-native.ps1') -SkipBuild -OutputPath $packageRoot -Commit $commit | Out-Null
& (Join-Path $PSScriptRoot 'build-windows-native.ps1') -SkipBuild -OutputPath $packageRoot -Commit $commit | Out-Null
if (-not (Test-Path -LiteralPath (Join-Path $packageRoot 'evohime.manifest.json'))) {
    throw 'package manifest was not written'
}

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
