[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$script = Join-Path $repo 'scripts\sign-windows-release.ps1'
if (-not (Test-Path -LiteralPath $script)) { throw 'Signing pipeline script is missing.' }
$raw = Get-Content -LiteralPath $script -Raw
foreach ($marker in @('signtool.exe', 'EVOHIME_SIGNING_CERTIFICATE_PATH', 'EVOHIME_SIGNING_CERTIFICATE_PASSWORD', '/fd SHA256', '/td SHA256', '/tr')) {
    if (-not $raw.Contains($marker)) { throw "Signing pipeline marker missing: $marker" }
}
if ($raw -match '(?i)(BEGIN (RSA|EC|OPENSSH) PRIVATE KEY|password\s*=\s*[''\"]|base64.{0,20}certificate)') {
    throw 'Signing pipeline contains a credential-like literal.'
}
Write-Output 'signing pipeline definition: PASS; certificate-backed execution required for release evidence'
