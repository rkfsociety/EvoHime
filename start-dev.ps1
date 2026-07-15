param(
  [switch]$Server,
  [switch]$Web,
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
  $powershell = Join-Path $PSHOME 'powershell.exe'
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
  Stop-Tree $script:serverProcess
  Wait-ForExit $script:serverProcess
  $script:serverProcess = Start-ManagedProcess '-Server'
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
  if (-not $env:BIND_ADDR) { $env:BIND_ADDR = '0.0.0.0:3000' }
  if (-not $env:WORKSPACE_ROOT) { $env:WORKSPACE_ROOT = '.' }
  if (-not $env:DEMO_FILE_PATH) { $env:DEMO_FILE_PATH = 'docs/sample-context.md' }
  if (-not $env:MODEL_PROVIDER) { $env:MODEL_PROVIDER = 'literouter' }
  if (-not $env:LITEROUTER_BASE_URL) { $env:LITEROUTER_BASE_URL = 'https://api.literouter.com/v1' }
  if (-not $env:LITEROUTER_MODEL) { $env:LITEROUTER_MODEL = 'deepseek:free' }
  Invoke-LocalSetup

  & (Get-CargoExe) run -p evohime-server
  exit $LASTEXITCODE
}

if ($Web) {
  Set-Location (Join-Path $root 'frontend\web')
  & npm run dev
  exit $LASTEXITCODE
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

if (-not (Test-PortAvailable 3000)) {
  throw 'Порт 3000 уже занят. Останови старый backend перед запуском EvoHime.'
}
if (-not (Test-PortAvailable 5173)) {
  throw 'Порт 5173 уже занят. Останови старый frontend перед запуском EvoHime.'
}

Invoke-LocalSetup

if (-not (Get-Command npm -ErrorAction SilentlyContinue)) {
  throw 'npm not found in PATH.'
}

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
try {
  Wait-ForHttp $serverUrl
  Wait-ForHttp $webUrl
} catch {
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

$serverMenu = New-Object System.Windows.Forms.ContextMenuStrip
$webMenu = New-Object System.Windows.Forms.ContextMenuStrip

$serverOpen = $serverMenu.Items.Add('Открыть health')
$serverRestart = $serverMenu.Items.Add('Перезапустить сервер')
$serverStop = $serverMenu.Items.Add('Остановить сервер')
$serverExit = $serverMenu.Items.Add('Выйти')

$webOpen = $webMenu.Items.Add('Открыть панель')
$webStop = $webMenu.Items.Add('Остановить панель')
$webExit = $webMenu.Items.Add('Выйти')

$serverOpen.Add_Click({ Open-Url $serverUrl })
$serverRestart.Add_Click({
  Restart-ServerProcess
})
$serverStop.Add_Click({
  Stop-Tree $script:serverProcess
})
$serverExit.Add_Click({
  $form.Close()
})

$webOpen.Add_Click({ Open-Url $webUrl })
$webStop.Add_Click({
  Stop-Tree $script:webProcess
})
$webExit.Add_Click({
  $form.Close()
})

$serverIcon.ContextMenuStrip = $serverMenu
$webIcon.ContextMenuStrip = $webMenu
$serverIcon.Visible = $true
$webIcon.Visible = $true
$serverIcon.Text = 'Сервер запускается...'
$webIcon.Text = 'Панель запускается...'
$serverIcon.Icon = [System.Drawing.SystemIcons]::Warning
$webIcon.Icon = [System.Drawing.SystemIcons]::Warning

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

$script:serverWasRunning = $true
$script:webWasRunning = $true

$timer = New-Object System.Windows.Forms.Timer
$timer.Interval = 1000
$timer.Add_Tick({
  $serverRunning = -not $script:serverProcess.HasExited
  $webRunning = -not $script:webProcess.HasExited

  Set-NotifyIconState -Icon $serverIcon -RunningIcon ([System.Drawing.SystemIcons]::Application) -StoppedIcon ([System.Drawing.SystemIcons]::Error) -Running $serverRunning -RunningText 'Сервер работает' -StoppedText 'Сервер остановлен'
  Set-NotifyIconState -Icon $webIcon -RunningIcon ([System.Drawing.SystemIcons]::Information) -StoppedIcon ([System.Drawing.SystemIcons]::Error) -Running $webRunning -RunningText 'Панель работает' -StoppedText 'Панель остановлена'

  if ($script:serverWasRunning -and -not $serverRunning) {
    $serverIcon.ShowBalloonTip(3000, 'EvoHime', 'Сервер остановлен', [System.Windows.Forms.ToolTipIcon]::Error)
  }
  if ($script:webWasRunning -and -not $webRunning) {
    $webIcon.ShowBalloonTip(3000, 'EvoHime', 'Панель остановлена', [System.Windows.Forms.ToolTipIcon]::Error)
  }

  $script:serverWasRunning = $serverRunning
  $script:webWasRunning = $webRunning
})

$form.Add_Shown({
  $form.Hide()
})

$form.Add_FormClosing({
  $timer.Stop()
  Stop-Tree $script:webProcess
  Stop-Tree $script:serverProcess
  Stop-LocalDatabase
  $serverIcon.Visible = $false
  $webIcon.Visible = $false
  $serverIcon.Dispose()
  $webIcon.Dispose()
  $serverMenu.Dispose()
  $webMenu.Dispose()
  $timer.Dispose()
})

$timer.Start()

Write-Host ''
Write-Host 'Server: http://localhost:3000/health'
Write-Host 'Web:    http://localhost:5173'
Write-Host ''
Write-Host 'Click the tray icons to open browser tabs. Right-click to stop each process.'

[System.Windows.Forms.Application]::Run($form)
