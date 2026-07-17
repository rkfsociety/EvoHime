param(
  [switch]$Server,
  [switch]$Web,
  [switch]$Worker,
  [switch]$Setup
)

$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
$serverUrl = 'http://localhost:3000/health'
$webUrl = 'http://localhost:5173'
$setupScript = Join-Path $root 'scripts\setup-local.ps1'

function Import-DotEnv {
  $envPath = Join-Path $root '.env'
  if (-not (Test-Path -LiteralPath $envPath)) {
    return
  }

  foreach ($line in Get-Content -LiteralPath $envPath) {
    if ($line -match '^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)\s*$') {
      $name = $Matches[1]
      $value = $Matches[2].Trim()
      if (($value.StartsWith('"') -and $value.EndsWith('"')) -or ($value.StartsWith("'") -and $value.EndsWith("'"))) {
        $value = $value.Substring(1, $value.Length - 2)
      }
      Set-Item -Path "Env:$name" -Value $value
    }
  }
}

function Invoke-LocalSetup {
  if (-not (Test-Path -LiteralPath $setupScript)) {
    throw "Не найден setup-скрипт: $setupScript"
  }
  & $setupScript -InstallPostgres -ApplyMigrations
  if ($LASTEXITCODE -ne 0) {
    throw "Подготовка локального PostgreSQL завершилась с кодом $LASTEXITCODE."
  }
}

function Stop-LocalDatabase {
  if (Test-Path -LiteralPath $setupScript) {
    & $setupScript -Stop
  }
}

function Test-PortAvailable([int]$port) {
  $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, $port)
  try {
    $listener.Start()
    return $true
  } catch {
    return $false
  } finally {
    $listener.Stop()
  }
}

function Wait-ForHttp([string]$url, [int]$timeoutSeconds = 60) {
  $deadline = [DateTime]::UtcNow.AddSeconds($timeoutSeconds)
  do {
    try {
      $response = Invoke-WebRequest -Uri $url -UseBasicParsing -TimeoutSec 3
      if ($response.StatusCode -ge 200 -and $response.StatusCode -lt 500) {
        return
      }
    } catch {
    }
    Start-Sleep -Milliseconds 500
  } while ([DateTime]::UtcNow -lt $deadline)
  throw "Сервис не ответил за $timeoutSeconds секунд: $url"
}

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

function Get-CargoExe {
  $cargo = Get-Command cargo -ErrorAction SilentlyContinue
  if ($cargo) {
    return $cargo.Source
  }

  $fallback = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
  if (Test-Path $fallback) {
    return $fallback
  }

  throw "cargo not found in PATH and $fallback is missing."
}

function Get-PythonExe {
  foreach ($name in @('python', 'python3')) {
    $command = Get-Command $name -ErrorAction SilentlyContinue
    if ($command) {
      return $command.Source
    }
  }

  $py = Get-Command py -ErrorAction SilentlyContinue
  if ($py) {
    return $py.Source
  }

  throw 'python not found in PATH (tried python, python3, py).'
}

function Get-WorkerBaseUrl {
  if ($env:PYTHON_WORKER_URL -and $env:PYTHON_WORKER_URL.Trim()) {
    return $env:PYTHON_WORKER_URL.Trim().TrimEnd('/')
  }
  return 'http://127.0.0.1:8090'
}

function Get-WorkerListenEndpoint {
  $uri = [Uri](Get-WorkerBaseUrl)
  $hostName = if ([string]::IsNullOrWhiteSpace($uri.Host)) { '127.0.0.1' } else { $uri.Host }
  $port = if ($uri.IsDefaultPort) { 8090 } else { $uri.Port }
  if ($port -le 0) { $port = 8090 }
  return @{
    Host = $hostName
    Port = [int]$port
    HealthUrl = "$(Get-WorkerBaseUrl)/health"
  }
}

function Open-Url([string]$url) {
  Start-Process $url | Out-Null
}

function Stop-Tree([System.Diagnostics.Process]$process) {
  if (-not $process -or $process.HasExited) {
    return
  }

  try {
    & taskkill.exe /PID $process.Id /T /F | Out-Null
  } catch {
    try {
      Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    } catch {
    }
  }
}

function Wait-ForExit([System.Diagnostics.Process]$process, [int]$timeoutMs = 15000) {
  if (-not $process) {
    return
  }

  $deadline = [DateTime]::UtcNow.AddMilliseconds($timeoutMs)
  while (-not $process.HasExited -and [DateTime]::UtcNow -lt $deadline) {
    Start-Sleep -Milliseconds 200
    try {
      $process.Refresh()
    } catch {
    }
  }
}

function Start-ManagedProcess([string]$switchName) {
  $powershell = $null
  $powershellCommand = Get-Command powershell.exe -ErrorAction SilentlyContinue
  if ($powershellCommand) {
    $powershell = $powershellCommand.Source
  } else {
    $pwshCommand = Get-Command pwsh.exe -ErrorAction SilentlyContinue
    if ($pwshCommand) {
      $powershell = $pwshCommand.Source
    }
  }
  if (-not $powershell) {
    $fallback = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    if (Test-Path -LiteralPath $fallback) {
      $powershell = $fallback
    }
  }
  if (-not $powershell) {
    throw 'Не найден PowerShell для запуска управляемого процесса.'
  }
  $logRoot = Join-Path $root '.launcher-logs'
  if (-not (Test-Path $logRoot)) {
    New-Item -ItemType Directory -Path $logRoot | Out-Null
  }

  $name = $switchName.TrimStart('-').ToLowerInvariant()
  return Start-Process -WindowStyle Hidden -PassThru -FilePath $powershell -ArgumentList @(
    '-NoProfile',
    '-ExecutionPolicy',
    'Bypass',
    '-File',
    $PSCommandPath,
    $switchName
  ) -WorkingDirectory $root -RedirectStandardOutput (Join-Path $logRoot "$name.out.log") -RedirectStandardError (Join-Path $logRoot "$name.err.log")
}

function Restart-ServerProcess {
  Write-Host '[EvoHime] Restarting server...'
  $script:serverRestartEnabled = $true
  $script:serverWasRunning = $false
  Stop-Tree $script:serverProcess
  Wait-ForExit $script:serverProcess
  $script:serverProcess = Start-ManagedProcess '-Server'
  $script:serverWasRunning = $true
}

function Restart-WorkerProcess {
  Write-Host '[EvoHime] Restarting Python worker...'
  $script:workerRestartEnabled = $true
  $script:workerWasRunning = $false
  Stop-Tree $script:workerProcess
  Wait-ForExit $script:workerProcess
  $script:workerProcess = Start-ManagedProcess '-Worker'
  $script:workerWasRunning = $true
}

function Set-NotifyIconState {
  param(
    [System.Windows.Forms.NotifyIcon]$Icon,
    [System.Drawing.Icon]$RunningIcon,
    [System.Drawing.Icon]$StoppedIcon,
    [bool]$Running,
    [string]$RunningText,
    [string]$StoppedText
  )

  if ($Running) {
    $Icon.Icon = $RunningIcon
    $Icon.Text = $RunningText
  } else {
    $Icon.Icon = $StoppedIcon
    $Icon.Text = $StoppedText
  }
}

if ($Server) {
  Set-Location $root
  Import-DotEnv
  if (-not $env:DATABASE_URL) { $env:DATABASE_URL = 'postgres://evohime:evohime@localhost:5432/evohime' }
  if (-not $env:BIND_ADDR) { $env:BIND_ADDR = '127.0.0.1:3000' }
  if (-not $env:WORKSPACE_ROOT) { $env:WORKSPACE_ROOT = '.' }
  if (-not $env:DEMO_FILE_PATH) { $env:DEMO_FILE_PATH = 'docs/sample-context.md' }
  if (-not $env:MODEL_PROVIDER) { $env:MODEL_PROVIDER = 'literouter' }
  if (-not $env:LITEROUTER_BASE_URL) { $env:LITEROUTER_BASE_URL = 'https://api.literouter.com/v1' }
  if (-not $env:LITEROUTER_MODEL) { $env:LITEROUTER_MODEL = 'deepseek:free' }
  if (-not $env:PYTHON_WORKER_URL) { $env:PYTHON_WORKER_URL = 'http://127.0.0.1:8090' }
  Invoke-LocalSetup

  & (Get-CargoExe) run -p evohime-server
  exit $LASTEXITCODE
}

if ($Web) {
  Set-Location (Join-Path $root 'frontend\web')
  & npm run dev
  exit $LASTEXITCODE
}

if ($Worker) {
  Set-Location $root
  Import-DotEnv
  if (-not $env:PYTHON_WORKER_URL) { $env:PYTHON_WORKER_URL = 'http://127.0.0.1:8090' }
  $endpoint = Get-WorkerListenEndpoint
  $python = Get-PythonExe
  $workerScript = Join-Path $root 'workers\python\worker.py'
  if (-not (Test-Path -LiteralPath $workerScript)) {
    throw "Не найден Python worker: $workerScript"
  }

  $pythonArgs = @()
  if ([System.IO.Path]::GetFileNameWithoutExtension($python) -eq 'py') {
    $pythonArgs += '-3'
  }
  $pythonArgs += @(
    $workerScript,
    '--host', $endpoint.Host,
    '--port', "$($endpoint.Port)"
  )
  Write-Host "[EvoHime] Starting Python worker on $($endpoint.Host):$($endpoint.Port)"
  & $python @pythonArgs
  exit $LASTEXITCODE
}

function Get-EvoHimeProcesses {
  $scriptPath = [regex]::Escape($PSCommandPath)
  $launcherCommandPattern = '(?i)(?:^|\s)-File\s+["'']?' + $scriptPath + '(?:["'']?\s|$)'
  $serverPath = [regex]::Escape((Join-Path $root 'target\debug\evohime-server.exe'))
  $webPath = [regex]::Escape((Join-Path $root 'frontend\web'))
  $workerPath = [regex]::Escape((Join-Path $root 'workers\python\worker.py'))
  Get-CimInstance Win32_Process |
    Where-Object {
      $_.ProcessId -ne $PID -and
      $_.CommandLine -and
      (
        $_.Name -eq 'evohime-server.exe' -or
        $_.CommandLine -match $launcherCommandPattern -or
        $_.CommandLine -match $serverPath -or
        $_.CommandLine -match $webPath -or
        $_.CommandLine -match $workerPath
      )
    } |
    ForEach-Object {
      Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue
    }
}

function Stop-PreviousLaunchers {
  foreach ($process in @(Get-EvoHimeProcesses)) {
    Write-Host "[EvoHime] Останавливаю старый процесс EvoHime (PID $($process.Id))..."
    Stop-Tree $process
    Wait-ForExit $process
  }
}

function Acquire-LauncherLock {
  $script:launcherMutex = New-Object System.Threading.Mutex($false, 'Global\EvoHime.NativeLauncher')
  try {
    $acquired = $script:launcherMutex.WaitOne(0)
  } catch [System.Threading.AbandonedMutexException] {
    $acquired = $true
  }

  if (-not $acquired) {
    Stop-PreviousLaunchers
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
      try {
        $acquired = $script:launcherMutex.WaitOne(250)
      } catch [System.Threading.AbandonedMutexException] {
        $acquired = $true
      }
    } while (-not $acquired -and [DateTime]::UtcNow -lt $deadline)
  }

  if (-not $acquired) {
    throw 'Не удалось остановить предыдущий launcher EvoHime.'
  }
}

if ($Setup) {
  Set-Location $root
  Invoke-LocalSetup
  exit 0
}

Write-Host '[EvoHime] Native local start'
Write-Host ''
Write-Host 'Important: PostgreSQL must already be running locally.'
Write-Host ''

Set-Location $root

Import-DotEnv

$workerEndpoint = Get-WorkerListenEndpoint
$workerUrl = $workerEndpoint.HealthUrl
if (-not $env:PYTHON_WORKER_URL) {
  $env:PYTHON_WORKER_URL = Get-WorkerBaseUrl
}

Acquire-LauncherLock
Stop-PreviousLaunchers

if (-not (Test-PortAvailable 3000)) {
  throw 'Порт 3000 уже занят процессом, который не принадлежит launcher EvoHime.'
}
if (-not (Test-PortAvailable 5173)) {
  throw 'Порт 5173 уже занят процессом, который не принадлежит launcher EvoHime.'
}
if (-not (Test-PortAvailable $workerEndpoint.Port)) {
  throw "Порт $($workerEndpoint.Port) уже занят процессом, который не принадлежит launcher EvoHime (Python worker)."
}

Invoke-LocalSetup

if (-not (Get-Command npm -ErrorAction SilentlyContinue)) {
  throw 'npm not found in PATH.'
}

# Fail fast if Python is missing before spawning server/web.
$null = Get-PythonExe

if (-not (Test-Path (Join-Path $root 'frontend\web\node_modules'))) {
  Write-Host 'Installing frontend dependencies...'
  Push-Location (Join-Path $root 'frontend\web')
  try {
    & npm install
  } finally {
    Pop-Location
  }
  if ($LASTEXITCODE -ne 0) {
    throw 'npm install failed.'
  }
}

$script:serverProcess = Start-ManagedProcess '-Server'
$script:webProcess = Start-ManagedProcess '-Web'
$script:workerProcess = Start-ManagedProcess '-Worker'
try {
  Wait-ForHttp $serverUrl
  Wait-ForHttp $webUrl
  Wait-ForHttp $workerUrl
} catch {
  Stop-Tree $script:workerProcess
  Stop-Tree $script:webProcess
  Stop-Tree $script:serverProcess
  Stop-LocalDatabase
  throw
}

$form = New-Object System.Windows.Forms.Form
$form.ShowInTaskbar = $false
$form.WindowState = [System.Windows.Forms.FormWindowState]::Minimized
$form.Opacity = 0
$form.StartPosition = [System.Windows.Forms.FormStartPosition]::Manual
$form.Location = New-Object System.Drawing.Point(-32000, -32000)
$form.Width = 1
$form.Height = 1

$serverIcon = New-Object System.Windows.Forms.NotifyIcon
$webIcon = New-Object System.Windows.Forms.NotifyIcon
$workerIcon = New-Object System.Windows.Forms.NotifyIcon

$serverMenu = New-Object System.Windows.Forms.ContextMenuStrip
$webMenu = New-Object System.Windows.Forms.ContextMenuStrip
$workerMenu = New-Object System.Windows.Forms.ContextMenuStrip

$serverOpen = $serverMenu.Items.Add('Открыть health')
$serverRestart = $serverMenu.Items.Add('Перезапустить сервер')
$serverStop = $serverMenu.Items.Add('Остановить сервер')
$serverExit = $serverMenu.Items.Add('Выйти')

$webOpen = $webMenu.Items.Add('Открыть панель')
$webStop = $webMenu.Items.Add('Остановить панель')
$webExit = $webMenu.Items.Add('Выйти')

$workerOpen = $workerMenu.Items.Add('Открыть health')
$workerRestart = $workerMenu.Items.Add('Перезапустить worker')
$workerStop = $workerMenu.Items.Add('Остановить worker')
$workerExit = $workerMenu.Items.Add('Выйти')

$serverOpen.Add_Click({ Open-Url $serverUrl })
$serverRestart.Add_Click({
  Restart-ServerProcess
})
$serverStop.Add_Click({
  $script:serverRestartEnabled = $false
  Stop-Tree $script:serverProcess
})
$serverExit.Add_Click({
  $script:serverRestartEnabled = $false
  $form.Close()
})

$webOpen.Add_Click({ Open-Url $webUrl })
$webStop.Add_Click({
  $script:webRestartEnabled = $false
  Stop-Tree $script:webProcess
})
$webExit.Add_Click({
  $script:webRestartEnabled = $false
  $form.Close()
})

$workerOpen.Add_Click({ Open-Url $workerUrl })
$workerRestart.Add_Click({
  Restart-WorkerProcess
})
$workerStop.Add_Click({
  $script:workerRestartEnabled = $false
  Stop-Tree $script:workerProcess
})
$workerExit.Add_Click({
  $script:workerRestartEnabled = $false
  $form.Close()
})

$serverIcon.ContextMenuStrip = $serverMenu
$webIcon.ContextMenuStrip = $webMenu
$workerIcon.ContextMenuStrip = $workerMenu
$serverIcon.Visible = $true
$webIcon.Visible = $true
$workerIcon.Visible = $true
$serverIcon.Text = 'Сервер запускается...'
$webIcon.Text = 'Панель запускается...'
$workerIcon.Text = 'Worker запускается...'
$serverIcon.Icon = [System.Drawing.SystemIcons]::Warning
$webIcon.Icon = [System.Drawing.SystemIcons]::Warning
$workerIcon.Icon = [System.Drawing.SystemIcons]::Warning

$serverIcon.Add_MouseUp({
  param($sender, $eventArgs)
  if ($eventArgs.Button -eq [System.Windows.Forms.MouseButtons]::Left) {
    Open-Url $serverUrl
  }
})

$webIcon.Add_MouseUp({
  param($sender, $eventArgs)
  if ($eventArgs.Button -eq [System.Windows.Forms.MouseButtons]::Left) {
    Open-Url $webUrl
  }
})

$workerIcon.Add_MouseUp({
  param($sender, $eventArgs)
  if ($eventArgs.Button -eq [System.Windows.Forms.MouseButtons]::Left) {
    Open-Url $workerUrl
  }
})

$script:serverWasRunning = $true
$script:webWasRunning = $true
$script:workerWasRunning = $true
$script:serverRestartEnabled = $true
$script:webRestartEnabled = $true
$script:workerRestartEnabled = $true

$timer = New-Object System.Windows.Forms.Timer
$timer.Interval = 1000
$timer.Add_Tick({
  $serverRunning = -not $script:serverProcess.HasExited
  $webRunning = -not $script:webProcess.HasExited
  $workerRunning = -not $script:workerProcess.HasExited

  Set-NotifyIconState -Icon $serverIcon -RunningIcon ([System.Drawing.SystemIcons]::Application) -StoppedIcon ([System.Drawing.SystemIcons]::Error) -Running $serverRunning -RunningText 'Сервер работает' -StoppedText 'Сервер остановлен'
  Set-NotifyIconState -Icon $webIcon -RunningIcon ([System.Drawing.SystemIcons]::Information) -StoppedIcon ([System.Drawing.SystemIcons]::Error) -Running $webRunning -RunningText 'Панель работает' -StoppedText 'Панель остановлена'
  Set-NotifyIconState -Icon $workerIcon -RunningIcon ([System.Drawing.SystemIcons]::Shield) -StoppedIcon ([System.Drawing.SystemIcons]::Error) -Running $workerRunning -RunningText 'Python worker работает' -StoppedText 'Python worker остановлен'

  if ($script:serverWasRunning -and -not $serverRunning) {
    $serverIcon.ShowBalloonTip(3000, 'EvoHime', 'Сервер остановлен', [System.Windows.Forms.ToolTipIcon]::Error)
    if ($script:serverRestartEnabled) {
      Write-Host '[EvoHime] Сервер завершился неожиданно, перезапускаю...'
      $script:serverProcess = Start-ManagedProcess '-Server'
      $serverRunning = -not $script:serverProcess.HasExited
    }
  }
  if ($script:webWasRunning -and -not $webRunning) {
    $webIcon.ShowBalloonTip(3000, 'EvoHime', 'Панель остановлена', [System.Windows.Forms.ToolTipIcon]::Error)
    if ($script:webRestartEnabled) {
      Write-Host '[EvoHime] Панель завершилась неожиданно, перезапускаю...'
      $script:webProcess = Start-ManagedProcess '-Web'
      $webRunning = -not $script:webProcess.HasExited
    }
  }
  if ($script:workerWasRunning -and -not $workerRunning) {
    $workerIcon.ShowBalloonTip(3000, 'EvoHime', 'Python worker остановлен', [System.Windows.Forms.ToolTipIcon]::Error)
    if ($script:workerRestartEnabled) {
      Write-Host '[EvoHime] Python worker завершился неожиданно, перезапускаю...'
      $script:workerProcess = Start-ManagedProcess '-Worker'
      $workerRunning = -not $script:workerProcess.HasExited
    }
  }

  $script:serverWasRunning = $serverRunning
  $script:webWasRunning = $webRunning
  $script:workerWasRunning = $workerRunning
})

$form.Add_Shown({
  $form.Hide()
})

$form.Add_FormClosing({
  $timer.Stop()
  $script:serverRestartEnabled = $false
  $script:webRestartEnabled = $false
  $script:workerRestartEnabled = $false
  Stop-Tree $script:workerProcess
  Stop-Tree $script:webProcess
  Stop-Tree $script:serverProcess
  Stop-LocalDatabase
  $serverIcon.Visible = $false
  $webIcon.Visible = $false
  $workerIcon.Visible = $false
  $serverIcon.Dispose()
  $webIcon.Dispose()
  $workerIcon.Dispose()
  $serverMenu.Dispose()
  $webMenu.Dispose()
  $workerMenu.Dispose()
  $timer.Dispose()
  if ($script:launcherMutex) {
    $script:launcherMutex.ReleaseMutex()
    $script:launcherMutex.Dispose()
  }
})

$timer.Start()

Write-Host ''
Write-Host 'Server: http://localhost:3000/health'
Write-Host 'Web:    http://localhost:5173'
Write-Host "Worker: $workerUrl"
Write-Host ''
Write-Host 'Click the tray icons to open browser tabs. Right-click to stop each process.'

[System.Windows.Forms.Application]::Run($form)
