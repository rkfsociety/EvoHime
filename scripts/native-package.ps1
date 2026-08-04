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
        client       = 'native-winui'
        architecture = $Architecture
        os_minimum   = $OsMinimum
        protocol     = 'desktop-ipc-v1'
        components   = [pscustomobject]@{
            ui         = 'EvoHime.Desktop.exe'
            core       = 'evohime-core.exe'
            supervisor = 'evohime-supervisor.exe'
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

function Write-NativePackageManifest {
    param(
        [Parameter(Mandatory)]
        [string]$OutputPath,
        [Parameter(Mandatory)]
        [object]$Manifest
    )

    $Manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $OutputPath -Encoding utf8NoBOM
}
