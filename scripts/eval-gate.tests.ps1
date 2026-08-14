[CmdletBinding()]
param(
    [string]$FixturePath = (Join-Path $PSScriptRoot '..\tests\evals\fixtures'),
    [string]$ArtifactPath = (Join-Path $PSScriptRoot '..\artifacts\eval-gate\summary.jsonl')
)
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$schemaPath = Join-Path $repo 'tests\evals\schema\case.schema.json'
$thresholdsPath = Join-Path $repo 'tests\evals\thresholds.toml'
foreach ($required in @($schemaPath, $thresholdsPath)) {
    if (-not (Test-Path -LiteralPath $required)) { throw "Evaluation gate input missing: $required" }
}
$forbidden = @('password=', 'api_key', 'bearer ', 'private_key', '@gmail.com')
$cases = @(Get-ChildItem -LiteralPath $FixturePath -Recurse -Filter '*.json' -File)
if ($cases.Count -eq 0) { throw "No evaluation fixtures found in $FixturePath" }
foreach ($case in $cases) {
    if ($case.Length -gt 256KB) { throw "Fixture exceeds 256 KiB: $($case.FullName)" }
    $raw = Get-Content -LiteralPath $case.FullName -Raw
    $lower = $raw.ToLowerInvariant()
    foreach ($marker in $forbidden) { if ($lower.Contains($marker)) { throw "Forbidden secret/PII marker $marker in $($case.FullName)" } }
    $value = $raw | ConvertFrom-Json
    foreach ($field in @('id','schema_version','fixture_version','prompt','assertions','limits')) {
        if ($null -eq $value.$field) { throw "Fixture $($case.FullName) misses $field" }
    }
    if ($value.schema_version -ne '1.0') { throw "Unsupported schema in $($case.FullName)" }
}
$artifactDir = Split-Path -Parent $ArtifactPath
New-Item -ItemType Directory -Force -Path $artifactDir | Out-Null
Remove-Item -LiteralPath $ArtifactPath -Force -ErrorAction SilentlyContinue
Push-Location $repo
try {
    $output = & cargo eval --fixture $FixturePath --mode deterministic --verbose 2>&1
    $exitCode = $LASTEXITCODE
    $output | Set-Content -LiteralPath $ArtifactPath -Encoding utf8
    if ($exitCode -ne 0) { throw "Deterministic evaluation failed with exit code $exitCode" }
} finally { Pop-Location }
Write-Output "evaluation gate: PASS ($($cases.Count) fixtures; artifact: $ArtifactPath)"
