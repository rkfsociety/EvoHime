$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$workflow = Get-Content (Join-Path $root '.github/workflows/windows.yml') -Raw
$block = [regex]::Match($workflow, '(?ms)        id: filter\r?\n        shell: pwsh\r?\n        run: \|\r?\n(.*?)(?=\r?\n  documentation:)').Groups[1].Value
if (-not $block) { throw 'Selection block missing' }
$block = [regex]::Replace($block, '(?m)^          ', '')
$originalWorkspace = $env:GITHUB_WORKSPACE
$originalOutput = $env:GITHUB_OUTPUT
$output = [IO.Path]::GetTempFileName()
Push-Location $root
try {
    $env:GITHUB_WORKSPACE = $root
    $env:GITHUB_OUTPUT = $output
    $head = git rev-parse HEAD
    $cases = @(
        @{ Event='push'; Base='3f1799696e536c58085c3c63280903217d6abef2'; Target='d6b7ba648059d22117db28e7bdfd3eb28381e00f'; Electron='true'; Rust='false' },
        @{ Event='push'; Base=$head; Electron='false'; Rust='false' },
        @{ Event='push'; Base=('0' * 40); Electron='true'; Rust='true' },
        @{ Event='workflow_dispatch'; Base=$head; Electron='true'; Rust='true' }
    )
    foreach ($case in $cases) {
        Clear-Content -LiteralPath $output
        $target = if ($case.Target) { $case.Target } else { $head }
        $script = $block.Replace('${{ github.event_name }}', $case.Event).Replace('${{ github.event_name == ''pull_request'' && github.event.pull_request.base.sha || github.event.before }}', $case.Base).Replace('${{ github.sha }}', $target)
        & ([scriptblock]::Create($script))
        $values = @{}
        Get-Content $output | ForEach-Object { $key, $value = $_ -split '=', 2; $values[$key] = $value }
        if ($values.electron -ne $case.Electron -or $values.rust -ne $case.Rust) { throw "Incorrect selection: $($case.Event) / $($case.Base)" }
        $manifests = @($values.rust_manifests | ConvertFrom-Json)
        foreach ($manifest in $manifests) {
            if ([IO.Path]::IsPathRooted($manifest) -or -not (Test-Path $manifest)) { throw "Nonportable manifest: $manifest" }
        }
    }
    Write-Host 'CI selection: PASS (push, empty diff, missing base, manual release)'
} finally {
    Pop-Location
    $env:GITHUB_WORKSPACE = $originalWorkspace
    $env:GITHUB_OUTPUT = $originalOutput
    Remove-Item -LiteralPath $output
}
