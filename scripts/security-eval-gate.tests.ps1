[CmdletBinding()]
param([string]$FixturePath = (Join-Path $PSScriptRoot '..\tests\evals\fixtures\security'))
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$cases = @(Get-ChildItem -LiteralPath $FixturePath -Filter '*.json' -File)
if ($cases.Count -eq 0) { throw "No mandatory security fixtures found in $FixturePath" }
foreach ($case in $cases) {
    $raw = Get-Content -LiteralPath $case.FullName -Raw
    foreach ($marker in @('password=', 'api_key', 'bearer ', 'private_key', '@gmail.com')) {
        if ($raw.ToLowerInvariant().Contains($marker)) { throw "Secret/PII marker $marker in security fixture $($case.Name)" }
    }
}
Push-Location $repo
try {
    & cargo eval --fixture $FixturePath --mode deterministic --verbose
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally { Pop-Location }
Write-Output "security evaluation gate: PASS ($($cases.Count) fixtures)"
