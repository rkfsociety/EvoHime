param([switch]$SkipRust, [switch]$SkipElectron)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path -LiteralPath (Join-Path $root 'contracts\receipts\v1\vectors.json'))) { throw 'receipt vectors are missing' }
if (-not $SkipRust) { cargo test -p evohime-receipts }
if (-not $SkipElectron) {
  Push-Location (Join-Path $root 'desktop\evohime-electron')
  try { npm test -- --run tests/receipt-contract.test.ts } finally { Pop-Location }
}
