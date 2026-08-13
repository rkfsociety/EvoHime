$ErrorActionPreference = 'Stop'

$workflow = Get-Content -Raw (Join-Path $PSScriptRoot '..\.github\workflows\windows.yml')

foreach ($required in @(
    'windows-latest',
    'scripts/build-windows-native.ps1',
    'actions/upload-artifact@v7',
    'cargo test --locked -p evohime-permissions -p evohime-tool-runtime -p evohime-model-gateway',
    'installer/EvoHime.iss',
    'EvoHime-Setup.exe',
    '-Version $env:RELEASE_VERSION',
    'npm run package',
    'scripts/electron-acceptance.tests.ps1',
    'scripts/electron-fault.tests.ps1',
    'scripts/electron-matrix.tests.ps1',
    'Rollback smoke after failed installer start',
    '--blame-hang',
    '--blame-hang-timeout 5m',
    'dotnet restore desktop/EvoHime.IpcTests/EvoHime.IpcTests.csproj',
    'dotnet build desktop/EvoHime.IpcTests/EvoHime.IpcTests.csproj',
    'dotnet test desktop/EvoHime.IpcTests/EvoHime.IpcTests.csproj',
    'winui-test-diagnostics',
    'iscc',
    'gh release create',
    'Remove old releases after publication',
    'cleanup-github-releases.ps1 -KeepCount 1',
    'Determine release from project version',
    'RELEASE_VERSION',
    'should_release',
    'contents: write'
)) {
    if ($workflow -notmatch [regex]::Escape($required)) {
        throw "GitHub workflow is missing required entry: $required"
    }
}

if ($workflow -match "tags: \['v\*'\]") {
    throw 'GitHub workflow must not require manually pushed version tags.'
}

if ($workflow -notmatch 'needs: \[rust-native, windows-check, prepare-release\]') {
    throw 'Native package build must depend on all CI checks and release decision.'
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
$buildScript = Get-Content -Raw (Join-Path $PSScriptRoot 'build-windows-native.ps1')
if ($buildScript -notmatch 'electron-builder') {
    throw 'The native package must build the Electron payload.'
}
if ($installer -notmatch 'IconFilename:') {
    throw 'The desktop shortcut must define an icon.'
}
if (($installer | Select-String -Pattern '\{autodesktop\}' -AllMatches).Matches.Count -ne 1) {
    throw 'The installer must create exactly one desktop shortcut.'
}
if ($installer -match '\{autoprograms\}') {
    throw 'The installer must not create an additional Start Menu shortcut.'
}

Write-Output 'native-workflow smoke: PASS'
