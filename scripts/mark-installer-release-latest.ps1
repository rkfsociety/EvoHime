[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string]$Repository,
    [string]$Tag = 'installer'
)

$ErrorActionPreference = 'Stop'
if (-not $env:GH_TOKEN) { throw 'GH_TOKEN is required.' }

$maxAttempts = 5
for ($attempt = 1; $attempt -le $maxAttempts; $attempt++) {
    $global:LASTEXITCODE = 0
    $viewOut = @(gh release view $Tag --repo $Repository 2>&1)
    if ($LASTEXITCODE -eq 0) { break }
    $viewText = ($viewOut | ForEach-Object { "$_" }) -join "`n"
    if ($viewText -match '(?i)(release not found|HTTP/?[0-9.]*\s*404|404 Not Found)' -or $attempt -eq $maxAttempts) {
        throw "Не найден installer release $Tag`: $viewText"
    }
    Start-Sleep -Seconds (5 * $attempt)
}

for ($attempt = 1; $attempt -le $maxAttempts; $attempt++) {
    $global:LASTEXITCODE = 0
    $editOut = @(gh release edit $Tag --repo $Repository --latest 2>&1)
    if ($LASTEXITCODE -eq 0) { break }
    $editText = ($editOut | ForEach-Object { "$_" }) -join "`n"
    if ($attempt -eq $maxAttempts) {
        throw "Не удалось отметить installer release $Tag как Latest: $editText"
    }
    Write-Warning "Не удалось отметить $Tag как Latest (попытка $attempt/$maxAttempts): $editText"
    Start-Sleep -Seconds (5 * $attempt)
}

Write-Host "Marked $Tag as the repository Latest release."
