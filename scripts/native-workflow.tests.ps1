$ErrorActionPreference = 'Stop'

$workflow = Get-Content -Raw (Join-Path $PSScriptRoot '..\.github\workflows\rust.yml')

foreach ($required in @(
    'windows-latest',
    'scripts/build-windows-native.ps1',
    'actions/upload-artifact@v4',
    'installer/EvoHime.iss',
    'EvoHime-Setup.exe',
    '-Version $version',
    'iscc',
    'gh release create',
    'contents: write'
)) {
    if ($workflow -notmatch [regex]::Escape($required)) {
        throw "GitHub workflow is missing required entry: $required"
    }
}

if ($workflow -notmatch 'refs/tags/v') {
    throw 'GitHub workflow must publish releases only for v* tags.'
}

if ($workflow -notmatch 'needs: \[rust-native, windows-check\]') {
    throw 'Native package build must depend on all CI checks.'
}

$buildIndex = $workflow.IndexOf('Build native package after CI checks')
$testIndex = $workflow.IndexOf('Test WinUI shell')
if ($buildIndex -lt $testIndex) {
    throw 'Native package build must happen after the WinUI CI test.'
}

if ($workflow -match 'path: native-package\s*$' -or $workflow -match 'evohime-native-windows-x64\.zip') {
    throw 'The workflow must publish only the single installer executable.'
}

$installer = Get-Content -Raw (Join-Path $PSScriptRoot '..\installer\EvoHime.iss')
if (($installer | Select-String -Pattern '\{autodesktop\}' -AllMatches).Matches.Count -ne 1) {
    throw 'The installer must create exactly one desktop shortcut.'
}
if ($installer -match '\{autoprograms\}') {
    throw 'The installer must not create an additional Start Menu shortcut.'
}

Write-Output 'native-workflow smoke: PASS'
