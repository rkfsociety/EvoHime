[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$InputRoot,
    [Parameter(Mandatory = $true)]
    [string]$EvidencePath
)

$ErrorActionPreference = 'Stop'
$signTool = Get-Command signtool.exe -ErrorAction SilentlyContinue
if ($null -eq $signTool) {
    $sdkTools = Get-ChildItem -Path 'C:\Program Files (x86)\Windows Kits\10\bin' -Filter signtool.exe -Recurse -File -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match '\\x64\\signtool\.exe$' } |
        Sort-Object FullName -Descending
    if ($sdkTools.Count -gt 0) { $signTool = $sdkTools[0].FullName }
}
if ($null -eq $signTool) { throw 'signtool.exe is required for the Windows signing pipeline.' }
$signToolPath = if ($signTool -is [string]) { $signTool } else { $signTool.Source }
$certificatePath = $env:EVOHIME_SIGNING_CERTIFICATE_PATH
$certificatePassword = $env:EVOHIME_SIGNING_CERTIFICATE_PASSWORD
if ([string]::IsNullOrWhiteSpace($certificatePath) -or [string]::IsNullOrWhiteSpace($certificatePassword)) {
    throw 'EVOHIME_SIGNING_CERTIFICATE_PATH and EVOHIME_SIGNING_CERTIFICATE_PASSWORD are required; credentials never come from the repository.'
}
if (-not (Test-Path -LiteralPath $certificatePath)) { throw "Signing certificate not found: $certificatePath" }
if (-not (Test-Path -LiteralPath $InputRoot)) { throw "Signing input does not exist: $InputRoot" }

$files = Get-ChildItem -LiteralPath $InputRoot -Recurse -File | Where-Object { $_.Extension -in @('.exe', '.dll') }
if ($files.Count -eq 0) { throw 'No Windows binaries found to sign.' }
$records = @()
foreach ($file in $files) {
    & $signToolPath sign /fd SHA256 /td SHA256 /tr 'http://timestamp.digicert.com' /f $certificatePath /p $certificatePassword /d 'EvoHime' $file.FullName
    if ($LASTEXITCODE -ne 0) { throw "Signing failed for $($file.Name)." }
    & $signToolPath verify /pa /all $file.FullName
    if ($LASTEXITCODE -ne 0) { throw "Signature verification failed for $($file.Name)." }
    $signature = Get-AuthenticodeSignature -LiteralPath $file.FullName
    if ($signature.Status -ne 'Valid') { throw "Authenticode status is not Valid for $($file.Name): $($signature.Status)" }
    if ($null -eq $signature.TimeStamperCertificate) { throw "RFC3161 timestamp is missing for $($file.Name)." }
    $records += [pscustomobject]@{
        path = [IO.Path]::GetRelativePath((Resolve-Path -LiteralPath $InputRoot), $file.FullName)
        sha256 = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash
        signature_status = [string]$signature.Status
        signer_subject = if ($null -eq $signature.SignerCertificate) { '' } else { [string]$signature.SignerCertificate.Subject }
        signer_thumbprint = if ($null -eq $signature.SignerCertificate) { '' } else { [string]$signature.SignerCertificate.Thumbprint }
        timestamp_present = $true
    }
}
$evidence = [pscustomobject]@{
    evidence_version = 1
    signing_algorithm = 'SHA256 + RFC3161 timestamp'
    binary_count = $records.Count
    redaction_status = 'redacted'
    binaries = $records
}
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $EvidencePath) | Out-Null
$evidence | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $EvidencePath -Encoding UTF8
Write-Output "windows signing: PASS ($($records.Count) binaries)"
