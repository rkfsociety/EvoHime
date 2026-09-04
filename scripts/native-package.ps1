Set-StrictMode -Version Latest

function New-NativePackageManifest {
    param(
        [Parameter(Mandatory)]
        [string]$Architecture,
        [Parameter(Mandatory)]
        [string]$OsMinimum
    )

    [pscustomobject]@{
        product      = 'EvoHime'
        client       = 'electron-shell'
        architecture = $Architecture
        os_minimum   = $OsMinimum
        protocol     = 'desktop-ipc-v1'
        components   = [pscustomobject]@{
            ui         = 'EvoHime.exe'
            browser_backend = 'EvoHime.exe'
            core       = 'evohime-core.exe'
            cli        = 'eva.exe'
            supervisor = 'evohime-supervisor.exe'
            analysis_worker = 'evohime-analysis-worker.exe'
            listener   = 'evohime-listener.exe'
            updater    = 'evohime-transaction.exe'
            verifier   = 'evohime-verify.exe'
        }
    }
}

function Invoke-NativeCommand {
    param(
        [Parameter(Mandatory)]
        [string]$Executable,
        [Parameter(Mandatory)]
        [string[]]$Arguments
    )

    & $Executable @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Команда завершилась с кодом ${LASTEXITCODE}: $Executable $($Arguments -join ' ')"
    }
}

function Write-NativeBuildMarker {
    <#
        .SYNOPSIS
        Пишет evohime.build.json — коммит и ветку, из которых собран пакет.

        .DESCRIPTION
        Клиент сравнивает этот коммит с вершиной отслеживаемой ветки и решает,
        нужна ли локальная пересборка. Коммит берётся из параметра (CI передаёт
        его явно) либо из git текущего checkout.
    #>
    param(
        [Parameter(Mandatory)]
        [string]$OutputPath,
        [Parameter(Mandatory)]
        [string]$RepositoryRoot,
        [string]$Commit,
        [string]$Branch = 'main'
    )

    $resolvedCommit = $Commit
    if ([string]::IsNullOrWhiteSpace($resolvedCommit)) {
        $git = Get-Command git -ErrorAction SilentlyContinue
        if ($null -ne $git) {
            $resolvedCommit = (& $git.Source -C $RepositoryRoot rev-parse HEAD 2>$null)
        }
    }
    $resolvedCommit = "$resolvedCommit".Trim()
    if ($resolvedCommit -notmatch '^[0-9a-f]{40}$') {
        Write-Warning 'Коммит сборки неизвестен: клиент пересоберёт себя при первом запуске.'
        return
    }

    [pscustomobject]@{
        commit    = $resolvedCommit
        branch    = $Branch
        builtAtMs = [long][Math]::Floor((Get-Date).ToUniversalTime().Subtract([datetime]'1970-01-01').TotalMilliseconds)
    } | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath $OutputPath -Encoding utf8NoBOM
}

function Write-NativePackageManifest {
    param(
        [Parameter(Mandatory)]
        [string]$OutputPath,
        [Parameter(Mandatory)]
        [object]$Manifest
    )

    $Manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $OutputPath -Encoding utf8NoBOM
}

function Write-ComponentManifest {
    param(
        [Parameter(Mandatory)] [string]$OutputPath,
        [Parameter(Mandatory)] [string]$PackageRoot,
        [Parameter(Mandatory)] [string]$Commit,
        [string]$Version = '0.1.0'
    )
    $componentFiles = @(
        @{ id = 'shell-host'; path = 'EvoHime.exe'; restart = 'shell' },
        @{ id = 'ui-bundle'; path = 'ui-bundle.zip'; restart = 'shell' },
        @{ id = 'core'; path = 'evohime-core.exe'; restart = 'core' },
        @{ id = 'supervisor'; path = 'evohime-supervisor.exe'; restart = 'supervisor' },
        @{ id = 'cli'; path = 'eva.exe'; restart = 'none' },
        @{ id = 'analysis-worker'; path = 'evohime-analysis-worker.exe'; restart = 'core' },
        @{ id = 'listener'; path = 'evohime-listener.exe'; restart = 'listener' },
        @{ id = 'transaction'; path = 'evohime-transaction.exe'; restart = 'transaction' },
        @{ id = 'verifier'; path = 'evohime-verify.exe'; restart = 'none' }
    )
    $components = foreach ($item in $componentFiles) {
        $file = Join-Path $PackageRoot $item.path
        if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw "Component is missing: $file" }
        $hash = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()
        [pscustomobject]@{
            id = $item.id; version = $Version; artifact = $item.path; path = $item.path
            size = [int64](Get-Item -LiteralPath $file).Length; sha256 = $hash
            dependencies = @(); required = $true; protocol = 'desktop-ipc-v1'; restart = $item.restart
        }
    }
    [pscustomobject]@{
        schema = 'evohime.component-manifest.v1'; product = 'EvoHime'; release_id = $Commit
        os = 'windows'; architecture = 'x64'; release_commit = $Commit; components = @($components)
    } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $OutputPath -Encoding utf8NoBOM
}
