$ErrorActionPreference = 'Stop'

$workflow = Get-Content -Raw (Join-Path $PSScriptRoot '..\.github\workflows\windows.yml')

foreach ($required in @(
    'windows-latest',
    'scripts/build-windows-native.ps1',
    'actions/upload-artifact@v7',
    'cargo test --locked -p evohime-permissions -p evohime-tool-runtime -p evohime-model-gateway',
    'installer/EvoHime.iss',
    'EvoHime-Setup.exe',
    'npm run package',
    'scripts/electron-acceptance.tests.ps1',
    'scripts/electron-fault.tests.ps1',
    'scripts/electron-matrix.tests.ps1',
    'Rollback smoke after failed installer start',
    'Staged rebuild apply smoke',
    '--apply-staging',
    '-Commit $env:GITHUB_SHA',
    'evohime.build.json',
    '--blame-hang',
    '--blame-hang-timeout 5m',
    'dotnet restore desktop/EvoHime.IpcTests/EvoHime.IpcTests.csproj',
    'dotnet build desktop/EvoHime.IpcTests/EvoHime.IpcTests.csproj',
    'dotnet test desktop/EvoHime.IpcTests/EvoHime.IpcTests.csproj',
    'winui-test-diagnostics',
    'iscc',
    'evohime-windows-installer',
    'publish-installer',
    'RELEASE_TAG: installer',
    'installer/release-notes.md',
    '--clobber',
    'Detect touched areas',
    'needs.changes.outputs'
)) {
    if ($workflow -notmatch [regex]::Escape($required)) {
        throw "GitHub workflow is missing required entry: $required"
    }
}

# Установщик живёт в одном постоянном релизе: файл перезаписывается, а тег
# фиксирован. Публикация возможна только по ручному запуску.
if ($workflow -notmatch [regex]::Escape("if: github.event_name == 'workflow_dispatch'")) {
    throw 'Installer publication must stay manual.'
}
if ($workflow -match 'gh release create "\$env:RELEASE_TAG"?\s+"?installer-output') {
    throw 'The installer must be uploaded to the existing release, not published as a new one.'
}
if ($workflow -match [regex]::Escape('gh release delete')) {
    throw 'The workflow must not delete releases.'
}

# Тяжёлые job включаются по затронутым путям, но сам workflow обязан стартовать
# на каждый коммит: клиент обновляется только на коммит с зелёными проверками, а
# у коммита без прогона проверок нет вообще — он остался бы непроверенным
# навсегда. Пропущенный job, в отличие от отсутствующего прогона, засчитывается.
if ($workflow -match '(?m)^\s*paths(-ignore)?:') {
    throw 'Workflow triggers must not filter by path.'
}
foreach ($job in @('rust-native:', 'electron-shell:', 'windows-check:', 'build-native:')) {
    $section = $workflow.Substring($workflow.IndexOf($job))
    if ($section.Substring(0, [Math]::Min(400, $section.Length)) -notmatch 'needs\.changes\.outputs') {
        throw "Job must be gated by the change filter: $job"
    }
}

if ($workflow -match "tags: \['v\*'\]") {
    throw 'GitHub workflow must not require manually pushed version tags.'
}

# Клиент обновляется из исходников по коммитам: workflow проверяет установщик,
# но не публикует релизы и не решает, «пора ли выпускать версию».
foreach ($forbidden in @(
    'prepare-release',
    'should_release',
    'RELEASE_VERSION',
    'cleanup-github-releases.ps1',
    '--generate-notes'
)) {
    if ($workflow -match [regex]::Escape($forbidden)) {
        throw "GitHub workflow must not publish per-version releases: $forbidden"
    }
}

if ($workflow -notmatch 'needs: \[changes, rust-native, windows-check\]') {
    throw 'Native package build must depend on all CI checks.'
}
# Упавшая проверка обязана остановить сборку пакета, а пропущенная — нет.
if ($workflow -notmatch [regex]::Escape("!contains(needs.*.result, 'failure')")) {
    throw 'Native package build must stop on a failed check.'
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

# Обновление идёт из исходников: установщик обязан оставить конфигурацию,
# по которой клиент знает репозиторий, ветку и режим запуска.
foreach ($required in @('update.json', 'repositoryUrl', 'launchPolicy', 'autoupdate', 'requireGreenCommit')) {
    if ($installer -notmatch [regex]::Escape($required)) {
        throw "The installer must configure source updates: $required"
    }
}
if ($installer -notmatch 'https://github\.com/') {
    throw 'The update repository must be an https remote.'
}
# Исходники и staging принадлежат обновлению, но данные пользователя удалять нельзя.
if ($installer -notmatch '\[UninstallDelete\]') {
    throw 'Uninstall must clean the update working directories.'
}
if ($installer -match 'Name: "\{localappdata\}\\EvoHime"\s*$') {
    throw 'Uninstall must not delete the user data directory.'
}

Write-Output 'native-workflow smoke: PASS'
