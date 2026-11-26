[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$forbiddenTargets = @(
    'docs/release-audit.md',
    'docs/repository-research.md',
    '.project/code-audit-plan.md'
)

Push-Location $repo
try {
    $files = @(git ls-files | Where-Object { $_ -match '\.(md|markdown|txt|rst|adoc)$' })
    $errors = [System.Collections.Generic.List[string]]::new()
    foreach ($relative in $files) {
        $path = Join-Path $repo $relative
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { continue }
        $content = Get-Content -Raw -LiteralPath $path
        foreach ($match in [regex]::Matches($content, '\[[^\]]+\]\(([^)]+)\)')) {
            $target = $match.Groups[1].Value.Split('#')[0].Split('?')[0]
            if ([string]::IsNullOrWhiteSpace($target) -or $target -match '^(https?|mailto):') { continue }
            try { $target = [Uri]::UnescapeDataString($target) } catch { $errors.Add("${relative}: invalid link encoding '$target'"); continue }
            $resolved = Join-Path (Split-Path -Parent $path) $target
            if (-not (Test-Path -LiteralPath $resolved)) { $errors.Add("${relative}: missing relative link '$target'") }
        }
        foreach ($forbidden in $forbiddenTargets) {
            if ($content.Replace('\', '/') -match [regex]::Escape($forbidden)) {
                $errors.Add("${relative}: stale reference '$forbidden'")
            }
        }
        $normRel = $relative.Replace('\', '/')
        if ($normRel -eq 'docs/architecture.md') {
            $lineCount = @(Get-Content -LiteralPath $path).Count
            if ($lineCount -gt 500) {
                $errors.Add("docs/architecture.md: exceeds line limit ($lineCount > 500)")
            }
        } elseif ($normRel -like 'docs/architecture/*.md') {
            $lineCount = @(Get-Content -LiteralPath $path).Count
            if ($lineCount -gt 2000) {
                $errors.Add("${normRel}: exceeds domain architecture line limit ($lineCount > 2000)")
            }
        }
    }
    if ($errors.Count -gt 0) {
        $errors | ForEach-Object { Write-Error $_ }
        exit 1
    }
    Write-Output "documentation gate: PASS ($($files.Count) tracked text files)"
} finally {
    Pop-Location
}
