function Resolve-ModuleReleaseChanges {
    param(
        [Parameter(Mandatory)] [string]$Module,
        [Parameter(Mandatory)] [string]$Version,
        [string]$NotesPath,
        [string[]]$ModulePaths = @(),
        [string]$RepoRoot
    )

    $changes = [System.Collections.Generic.List[string]]::new()

    if (-not $NotesPath) {
        $baseDir = if ($RepoRoot) { $RepoRoot } else { Split-Path -Parent $PSScriptRoot }
        $NotesPath = Join-Path $baseDir "release-notes/$Module.md"
    }

    if (Test-Path -LiteralPath $NotesPath -PathType Leaf) {
        $lines = @(Get-Content -LiteralPath $NotesPath -Encoding utf8)
        $sectionPattern = "^\s*##\s+v?$([regex]::Escape($Version))(?:\s|$)"
        $inSection = $false

        foreach ($rawLine in $lines) {
            $line = [string]$rawLine
            if (-not $inSection) {
                if ($line -match $sectionPattern) {
                    $inSection = $true
                }
                continue
            }

            if ($line -match '^\s*#{1,2}\s+') {
                break
            }

            $trimmed = $line.Trim()
            if (-not $trimmed) { continue }
            if ($trimmed -match '^<!--.*-->$') { continue }

            if ($trimmed -match '^[-*•]\s+(.+)$') {
                [void]$changes.Add("- $($Matches[1].Trim())")
            } else {
                [void]$changes.Add("- $trimmed")
            }
        }
    }

    if ($changes.Count -eq 0) {
        $currentTag = "module-$Module-v$Version"
        $tagPattern = "module-$Module-v*"
        $existingTags = @(git tag -l $tagPattern --sort=-v:refname 2>$null)
        $previousTag = $null
        foreach ($candidate in $existingTags) {
            $tagText = ([string]$candidate).Trim()
            if ($tagText -and $tagText -ne $currentTag) {
                $previousTag = $tagText
                break
            }
        }

        $gitCommits = @()
        if ($previousTag) {
            $range = "$previousTag..HEAD"
            if ($ModulePaths.Count -gt 0) {
                $gitCommits = @(git log $range --pretty=format:'- %s' -- $ModulePaths 2>$null)
            } else {
                $gitCommits = @(git log $range --pretty=format:'- %s' 2>$null)
            }
        } elseif ($ModulePaths.Count -gt 0) {
            $gitCommits = @(git log -5 --pretty=format:'- %s' -- $ModulePaths 2>$null)
        }

        foreach ($change in $gitCommits) {
            $text = ([string]$change).Trim()
            if (-not $text) { continue }
            if ($text -match '^-\s*chore\(release\):') { continue }
            [void]$changes.Add($text)
        }
    }

    if ($changes.Count -eq 0) {
        [void]$changes.Add('- Обновлён состав поставки модуля и его проверенный бинарный артефакт.')
    }

    return ,$changes.ToArray()
}
