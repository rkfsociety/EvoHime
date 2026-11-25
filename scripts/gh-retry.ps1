function Invoke-WithRetry {
    param(
        [Parameter(Mandatory)] [scriptblock]$Command,
        [Parameter(Mandatory)] [string]$FailureMessage,
        [int]$MaxAttempts = 5,
        [int]$InitialDelaySeconds = 5,
        [switch]$Gh,
        [switch]$AllowNotFound
    )

    for ($attempt = 1; $attempt -le $MaxAttempts; $attempt++) {
        try {
            if ($Gh) {
                return (Invoke-GhRaw -Command $Command -AllowNotFound:$AllowNotFound)
            }
            return (& $Command)
        }
        catch {
            $details = $_.Exception.Message
            if ($attempt -lt $MaxAttempts) {
                $delay = $InitialDelaySeconds * $attempt
                Write-Warning "$FailureMessage (попытка $attempt/$MaxAttempts): $details. Повтор через ${delay}с..."
                Start-Sleep -Seconds $delay
            } else {
                throw "$FailureMessage (после $MaxAttempts попыток): $details"
            }
        }
    }
}

function Invoke-GhRaw {
    param(
        [Parameter(Mandatory)] [scriptblock]$Command,
        [switch]$AllowNotFound
    )

    $stderrFile = [System.IO.Path]::GetTempFileName()
    try {
        $prevErrorAction = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        try {
            $global:LASTEXITCODE = 0
            $stdout = @(& $Command 2>$stderrFile)
            $exitCode = $LASTEXITCODE
        }
        finally {
            $ErrorActionPreference = $prevErrorAction
        }
        $stderr = if (Test-Path -LiteralPath $stderrFile) {
            (Get-Content -LiteralPath $stderrFile -Raw -ErrorAction SilentlyContinue)
        } else { '' }
    }
    finally {
        Remove-Item -LiteralPath $stderrFile -Force -ErrorAction SilentlyContinue
    }

    if ($exitCode -eq 0) {
        $global:LASTEXITCODE = 0
        return ,$stdout
    }

    $details = if ([string]::IsNullOrWhiteSpace($stderr)) {
        ($stdout | ForEach-Object { "$_" }) -join "`n"
    } else {
        $stderr.Trim()
    }

    if ($AllowNotFound -and $details -match '(?i)(release not found|HTTP/?[0-9.]*\s*404|404 Not Found)') {
        $global:LASTEXITCODE = 1
        return $null
    }

    throw "gh завершился с кодом ${exitCode}: $details"
}

function Invoke-GhWithRetry {
    param(
        [Parameter(Mandatory)] [scriptblock]$Command,
        [Parameter(Mandatory)] [string]$FailureMessage,
        [int]$MaxAttempts = 5,
        [int]$InitialDelaySeconds = 5,
        [switch]$AllowNotFound
    )

    return Invoke-WithRetry -Command $Command -FailureMessage $FailureMessage -MaxAttempts $MaxAttempts -InitialDelaySeconds $InitialDelaySeconds -Gh -AllowNotFound:$AllowNotFound
}
