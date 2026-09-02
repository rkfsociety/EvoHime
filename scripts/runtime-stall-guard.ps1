[CmdletBinding()]
param(
  [string]$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path,
  [string]$Out = (Join-Path $Root 'artifacts/runtime-stall-findings.json'),
  [string]$Suppressions = (Join-Path $Root '.codex/runtime-stall-suppressions.json'),
  [switch]$FailOnUnsuppressed
)

$ErrorActionPreference = 'Stop'
$maxFindings = 4096
$maxReportBytes = 10MB
$extensions = @('.rs', '.ts', '.tsx', '.js', '.mjs')
$rules = @(
  @{ Kind='Filesystem'; Pattern='\b(?:std::fs|fs\.(?:readFileSync|writeFileSync|readdirSync|statSync)|read_to_string)\b' },
  @{ Kind='ProcessWait'; Pattern='\b(?:std::process::Command|execFileSync|spawnSync|wait_with_output)\b' },
  @{ Kind='Sleep'; Pattern='\b(?:thread::sleep|Atomics\.wait)\b' },
  @{ Kind='Network'; Pattern='\b(?:reqwest::blocking|fetchSync)\b' },
  @{ Kind='Database'; Pattern='\b(?:rusqlite|better-sqlite3)\b' }
)
$ignored = @('\target\', '\node_modules\', '\artifacts\', '\dist\', '\release\')
$suppress = @{}
if (Test-Path -LiteralPath $Suppressions) {
  $raw = Get-Content -LiteralPath $Suppressions -Raw | ConvertFrom-Json
  foreach ($item in @($raw.suppressions)) { if ($item.fingerprint) { $suppress[$item.fingerprint] = $item.reason } }
}
$findings = [System.Collections.Generic.List[object]]::new()
Get-ChildItem -LiteralPath $Root -Recurse -File | Where-Object { $extensions -contains $_.Extension.ToLowerInvariant() } | ForEach-Object {
  $full = $_.FullName
  if ($ignored | Where-Object { $full.Contains($_) }) { return }
  $relative = [IO.Path]::GetRelativePath($Root, $full).Replace('\','/')
  $lines = Get-Content -LiteralPath $full
  for ($index = 0; $index -lt $lines.Count; $index++) {
    foreach ($rule in $rules) {
      if ($lines[$index] -match $rule.Pattern) {
        $context = "$relative|$($index + 1)|$($rule.Kind)|$($lines[$index].Trim())"
        $fingerprint = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($context))).ToLowerInvariant()
        $findings.Add([pscustomobject]@{ id="stall-$($fingerprint.Substring(0,16))"; component='static-source-scan'; file=$relative; line=$index + 1; call_kind=$rule.Kind; severity_hint='review'; detector_rule='known-sync-api'; fingerprint=$fingerprint; suppressed=$suppress.ContainsKey($fingerprint); suppression_reason=$suppress[$fingerprint] })
        if ($findings.Count -ge $maxFindings) { break }
      }
    }
    if ($findings.Count -ge $maxFindings) { break }
  }
}
$report = [pscustomobject]@{ schema_version=1; generated_at=(Get-Date).ToUniversalTime().ToString('o'); detector='runtime-stall-guard'; findings=@($findings); truncated=($findings.Count -ge $maxFindings) }
$json = $report | ConvertTo-Json -Depth 6
if ([Text.Encoding]::UTF8.GetByteCount($json) -gt $maxReportBytes) { throw 'Runtime Stall Guard report exceeds 10 MiB.' }
$parent = Split-Path -Parent $Out
if (-not (Test-Path -LiteralPath $parent)) { New-Item -ItemType Directory -Path $parent -Force | Out-Null }
[IO.File]::WriteAllText($Out, $json, [Text.UTF8Encoding]::new($false))
if ($FailOnUnsuppressed -and @($findings | Where-Object { -not $_.suppressed }).Count -gt 0) { exit 2 }
