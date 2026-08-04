$ErrorActionPreference = 'Stop'

. "$PSScriptRoot\native-package.ps1"

$manifest = New-NativePackageManifest -Architecture 'x64' -OsMinimum 'Windows 11 22H2'

if ($manifest.product -ne 'EvoHime') { throw 'manifest product is not EvoHime' }
if ($manifest.client -ne 'native-winui') { throw 'manifest client must be native-winui' }
if ($manifest.architecture -ne 'x64') { throw 'manifest architecture must be x64' }
if ($manifest.components.core -ne 'evohime-core.exe') { throw 'core component is missing' }
if ($manifest.components.supervisor -ne 'evohime-supervisor.exe') { throw 'supervisor component is missing' }
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

& (Join-Path $PSScriptRoot 'build-windows-native.ps1') -SkipBuild -OutputPath $packageRoot | Out-Null
& (Join-Path $PSScriptRoot 'build-windows-native.ps1') -SkipBuild -OutputPath $packageRoot | Out-Null
if (-not (Test-Path -LiteralPath (Join-Path $packageRoot 'evohime.manifest.json'))) {
    throw 'package manifest was not written'
}
Remove-Item -LiteralPath $packageRoot -Recurse -Force

Write-Output 'native-package smoke: PASS'
