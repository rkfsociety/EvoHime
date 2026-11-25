#Requires -Version 7.0
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot
try {
  $baseRef = 'origin/main'
  git rev-parse --verify $baseRef *> $null
  if ($LASTEXITCODE -ne 0) {
    $baseRef = 'HEAD~1'
  }

  $changedFiles = @(git diff --name-only "$baseRef..HEAD")
  if ($LASTEXITCODE -ne 0) {
    throw "Failed to collect changed files against $baseRef"
  }

  if ($changedFiles.Count -eq 0) {
    Write-Host 'pre-push-check: no outgoing changes against origin/main.'
    exit 0
  }

  $touchesRust = [bool]($changedFiles | Where-Object { $_ -match '(^Cargo\.(toml|lock)$|^crates/|\.rs$)' })
  $touchesRouterOrVersions = [bool]($changedFiles | Where-Object { $_ -match '^(release-versions/|release-notes/|scripts/module-router|crates/evohime-core/Cargo\.toml$)' })
  $touchesElectron = [bool]($changedFiles | Where-Object { $_ -match '^desktop/evohime-electron/' })
  $touchesProto = [bool]($changedFiles | Where-Object { $_ -match '^(crates/desktop-ipc/proto/|desktop/evohime-electron/src/shared/)' })

  if ($touchesRust) {
    Write-Host 'pre-push-check: checking Cargo.lock (--locked metadata)...'
    cargo metadata --locked --format-version 1 *> $null
    if ($LASTEXITCODE -ne 0) {
      throw 'Cargo.lock is out of sync with Cargo.toml. Run: cargo metadata --format-version 1 | Out-Null'
    }

    Write-Host 'pre-push-check: checking rustfmt (cargo fmt --all -- --check)...'
    cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) {
      throw 'Formatting check failed. Run: cargo fmt --all'
    }
  }

  if ($touchesRouterOrVersions) {
    Write-Host 'pre-push-check: running module-router smoke & decoupling guard tests...'
    & pwsh -NoProfile -File (Join-Path $PSScriptRoot 'module-router.tests.ps1')
    if ($LASTEXITCODE -ne 0) {
      throw 'module-router.tests.ps1 failed.'
    }
  }

  if ($touchesElectron) {
    Push-Location (Join-Path $repoRoot 'desktop/evohime-electron')
    try {
      if ($touchesProto) {
        Write-Host 'pre-push-check: checking IPC protocol sync (npm run check:protocol)...'
        npm run check:protocol
        if ($LASTEXITCODE -ne 0) {
          throw 'npm run check:protocol failed.'
        }
      }

      Write-Host 'pre-push-check: running Electron typecheck (npm run typecheck)...'
      npm run typecheck
      if ($LASTEXITCODE -ne 0) {
        throw 'npm run typecheck failed.'
      }
    } finally {
      Pop-Location
    }
  }

  Write-Host 'pre-push-check: all fast pre-push checks passed.'
} finally {
  Pop-Location
}
