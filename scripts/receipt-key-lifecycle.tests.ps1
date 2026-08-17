$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

foreach ($path in @(
    'contracts\receipts\v1\key-transition.schema.json',
    'contracts\receipts\v1\key-history-checkpoint.schema.json',
    'contracts\receipts\v1\rotation-state-v1.schema.json',
    'contracts\receipts\v1\trusted-roots.schema.json',
    'contracts\receipts\v1\key-transition-vectors.json',
    'contracts\receipts\v1\key-history-checkpoint-vectors.json',
    'docs\security\receipt-key-lifecycle-v1.md')) {
    if (-not (Test-Path -LiteralPath (Join-Path $root $path))) { throw "missing lifecycle contract: $path" }
}

Push-Location $root
try {
    cargo test -p evohime-receipts --lib
    cargo build -p evohime-receipts --bin evohime-verify

    $temp = Join-Path ([System.IO.Path]::GetTempPath()) ("evohime-key-lifecycle-" + [guid]::NewGuid())
    New-Item -ItemType Directory -Path $temp | Out-Null
    try {
        $vector = Get-Content -Raw contracts\receipts\v1\key-transition-vectors.json | ConvertFrom-Json
        $positive = $vector.positive[0]
        $transition = [ordered]@{
            transition_version = 1
            transition_id = '018c4f4e-5c00-7abc-8def-0123456789ab'
            created_at = '2025-01-15T12:34:56.789Z'
            reason = 'initial'
            actor = 'system'
            new_key_id = $positive.new_key_id
            new_public_key = $positive.new_public_key
            continuity = 'genesis'
            signed_by_key_id = $positive.new_key_id
            signature = $positive.signature
        }
        $history = Join-Path $temp 'public-history-v1.jsonl'
        $transition | ConvertTo-Json -Compress | Set-Content -Encoding utf8NoBOM $history
        $manifest = [ordered]@{ manifest_version = 1; history_schema = 'key-history-v1'; status = 'current'; active_key_id = $positive.new_key_id; exported_transition_count = 1 }
        $manifest | ConvertTo-Json -Compress | Set-Content -Encoding utf8NoBOM (Join-Path $temp 'public-history-v1.manifest.json')
        $receipts = Join-Path $temp 'receipts.jsonl'
        [System.IO.File]::WriteAllText($receipts, "`n", [System.Text.UTF8Encoding]::new($false))
        $verifier = Join-Path $root 'target\debug\evohime-verify.exe'

        & $verifier verify --receipts $receipts --key-history $history --trust-key $positive.new_key_id
        if ($LASTEXITCODE -ne 0) { throw "trusted verifier result was $LASTEXITCODE" }
        & $verifier verify --receipts $receipts --key-history $history
        if ($LASTEXITCODE -ne 3) { throw "untrusted verifier result was $LASTEXITCODE" }
        $bad = Join-Path $temp 'bad.jsonl'
        [System.IO.File]::WriteAllText($bad, '{', [System.Text.UTF8Encoding]::new($false))
        & $verifier verify --receipts $receipts --key-history $bad
        if ($LASTEXITCODE -ne 2) { throw "damaged verifier result was $LASTEXITCODE" }
        & $verifier verify --receipts $receipts
        if ($LASTEXITCODE -ne 4) { throw "argument verifier result was $LASTEXITCODE" }
    } finally {
        Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
    }
} finally {
    Pop-Location
}

# The last verifier invocation intentionally returns exit code 4 for the
# argument-validation case. Do not leak that expected native exit code from
# the successful PowerShell test script when it runs through the CI wrapper.
exit 0
