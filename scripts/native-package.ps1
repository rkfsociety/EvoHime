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
            supervisor = 'evohime-supervisor.exe'
            analysis_worker = 'evohime-analysis-worker.exe'
            listener   = 'evohime-listener.exe'
            updater    = 'evohime-transaction.exe'
            verifier   = 'evohime-verify.exe'
        }
    }
}

function Get-DotNetExecutable {
    $command = Get-Command dotnet -ErrorAction SilentlyContinue
    if ($null -ne $command) {
        return $command.Source
    }

    $knownPath = Join-Path ${env:ProgramFiles} 'dotnet\dotnet.exe'
    if (Test-Path -LiteralPath $knownPath) {
        return $knownPath
    }

    throw 'dotnet SDK не найден. Установите .NET SDK 10 и повторите сборку.'
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
