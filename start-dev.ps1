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

# Visible progress window for Stop/Restart from the tray — restarts take
# several seconds (graceful process stop + relaunch + health checks), and
# with no feedback it looked like the button did nothing. Unlike a Marquee
# bar (which just slides back and forth forever and never actually shows
# how far along things are), this is a real determinate bar driven by a
# 0-100 percent per stage, animated smoothly toward each new value instead
# of jumping instantly. Mirrors the progress bar in the release Launcher's
# egui window (crates/launcher), which is fraction-driven the same way.
$script:progressForm = $null
$script:progressLabel = $null
$script:progressPercentLabel = $null
$script:progressBar = $null
$script:progressShown = 0

function Show-RestartProgress([string]$title) {
  $script:progressShown = 0

  $script:progressForm = New-Object System.Windows.Forms.Form
  $script:progressForm.Text = $title
  $script:progressForm.FormBorderStyle = [System.Windows.Forms.FormBorderStyle]::FixedDialog
  $script:progressForm.MaximizeBox = $false
  $script:progressForm.MinimizeBox = $false
  $script:progressForm.StartPosition = [System.Windows.Forms.FormStartPosition]::CenterScreen
  $script:progressForm.TopMost = $true
  $script:progressForm.ShowInTaskbar = $false
  $script:progressForm.BackColor = [System.Drawing.Color]::FromArgb(255, 246, 247, 251)
  $script:progressForm.Font = New-Object System.Drawing.Font('Segoe UI', 9.5)
  $script:progressForm.ClientSize = New-Object System.Drawing.Size(420, 130)
  if ($script:appBrandIcon) {
    $script:progressForm.Icon = $script:appBrandIcon
  }

  $script:progressLabel = New-Object System.Windows.Forms.Label
  $script:progressLabel.Text = 'Запускаю...'
  $script:progressLabel.AutoSize = $false
  $script:progressLabel.Font = New-Object System.Drawing.Font('Segoe UI', 11, [System.Drawing.FontStyle]::Regular)
  $script:progressLabel.ForeColor = [System.Drawing.Color]::FromArgb(255, 32, 34, 46)
  $script:progressLabel.Bounds = New-Object System.Drawing.Rectangle(24, 26, 300, 28)
  $script:progressForm.Controls.Add($script:progressLabel)

  $script:progressPercentLabel = New-Object System.Windows.Forms.Label
  $script:progressPercentLabel.Text = '0%'
  $script:progressPercentLabel.AutoSize = $false
  $script:progressPercentLabel.TextAlign = [System.Drawing.ContentAlignment]::MiddleRight
  $script:progressPercentLabel.Font = New-Object System.Drawing.Font('Segoe UI Semibold', 11)
  $script:progressPercentLabel.ForeColor = [System.Drawing.Color]::FromArgb(255, 90, 95, 235)
  $script:progressPercentLabel.Bounds = New-Object System.Drawing.Rectangle(324, 26, 72, 28)
  $script:progressForm.Controls.Add($script:progressPercentLabel)

  $script:progressBar = New-Object System.Windows.Forms.ProgressBar
  $script:progressBar.Style = [System.Windows.Forms.ProgressBarStyle]::Continuous
  $script:progressBar.Minimum = 0
  $script:progressBar.Maximum = 100
  $script:progressBar.Value = 0
  $script:progressBar.Bounds = New-Object System.Drawing.Rectangle(24, 68, 372, 26)
  $script:progressForm.Controls.Add($script:progressBar)

  $script:progressForm.Show()
  [System.Windows.Forms.Application]::DoEvents()
}

function Set-RestartProgress([string]$text, [int]$percent) {
  if (-not $script:progressForm) {
    return
  }
  $script:progressLabel.Text = $text
  $target = [Math]::Max(0, [Math]::Min(100, $percent))

  # Animate the fill smoothly toward the target instead of jumping straight
  # there — pumping the message loop on every step is what makes the bar
  # (and the rest of the window) actually repaint while we're otherwise
  # blocked synchronously on process stop/start/health-check calls.
  while ($script:progressShown -lt $target) {
    $script:progressShown = [Math]::Min($target, $script:progressShown + 2)
    $script:progressBar.Value = $script:progressShown
    $script:progressPercentLabel.Text = "$($script:progressShown)%"
    [System.Windows.Forms.Application]::DoEvents()
    Start-Sleep -Milliseconds 8
  }
  [System.Windows.Forms.Application]::DoEvents()
}

function Close-RestartProgress([int]$lingerMs = 900) {
  if (-not $script:progressForm) {
    return
  }
  Start-Sleep -Milliseconds $lingerMs
  $script:progressForm.Close()
  $script:progressForm.Dispose()
  $script:progressForm = $null
  $script:progressLabel = $null
  $script:progressPercentLabel = $null
  $script:progressBar = $null
}

function Get-QuietHealth([string]$url, [int]$timeoutSeconds = 20) {
  try {
    Wait-ForHttp $url $timeoutSeconds
    return $true
  } catch {
    return $false
  }
}

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

function Hide-LauncherConsole {
  # Tray host only — no console spam, NotifyIcons are the UI.
  if (-not ('EvoHime.ConsoleWindow' -as [type])) {
    Add-Type -Namespace EvoHime -Name ConsoleWindow -MemberDefinition @'
[DllImport("kernel32.dll")]
public static extern IntPtr GetConsoleWindow();
[DllImport("user32.dll")]
public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
'@
  }
  $hwnd = [EvoHime.ConsoleWindow]::GetConsoleWindow()
  if ($hwnd -ne [IntPtr]::Zero) {
    [void][EvoHime.ConsoleWindow]::ShowWindow($hwnd, 0) # SW_HIDE
  }
}

if ($Setup) {
  Set-Location $root
  Invoke-LocalSetup
  exit 0
}

# Full launcher = tray icons. Hide the console immediately so migrations/setup
# do not dump a PowerShell window on the desktop.
if (-not ($Server -or $Web -or $Worker)) {
  Hide-LauncherConsole
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

$appIcon = New-Object System.Windows.Forms.NotifyIcon
$appMenu = New-Object System.Windows.Forms.ContextMenuStrip

# Mirrors the release Launcher's tray menu shape (crates/launcher/src/main.rs
# build_tray_menu: Open Dashboard / Check Updates / Stop / Restart / Settings
# / separator / Exit — one icon, global Stop/Restart). "Check Updates" is
# dropped here: a dev checkout has no self-update mechanism to check.
# Stop/Restart act on the whole dev stack (server+web+worker) as one unit,
# matching the release Launcher's single-process semantics; running them as
# three separate dev processes underneath is the dev-only difference being
# tested. Settings deep-links into the web panel's settings modal via
# ?settings=1 (frontend/web/src/app.tsx), same as the release Launcher.
$menuOpenDashboard = $appMenu.Items.Add('Open Dashboard')
$menuStop = $appMenu.Items.Add('Stop')
$menuRestart = $appMenu.Items.Add('Restart')
$menuSettings = $appMenu.Items.Add('Settings')
[void]$appMenu.Items.Add('-')
$menuExit = $appMenu.Items.Add('Exit')

$menuOpenDashboard.Add_Click({ Open-Url $webUrl })
$menuSettings.Add_Click({ Open-Url "$webUrl/?settings=1" })
$menuStop.Add_Click({
  Show-RestartProgress 'EvoHime — остановка'
  $script:serverRestartEnabled = $false
  $script:webRestartEnabled = $false
  $script:workerRestartEnabled = $false

  Set-RestartProgress 'Останавливаю сервер...' 25
  Stop-Tree $script:serverProcess
  Set-RestartProgress 'Останавливаю панель...' 55
  Stop-Tree $script:webProcess
  Set-RestartProgress 'Останавливаю worker...' 85
  Stop-Tree $script:workerProcess

  Set-RestartProgress 'Остановлено' 100
  Close-RestartProgress
})
$menuRestart.Add_Click({
  Show-RestartProgress 'EvoHime — перезапуск'

  Set-RestartProgress 'Останавливаю сервер...' 10
  $script:serverRestartEnabled = $true
  $script:serverWasRunning = $false
  Stop-Tree $script:serverProcess
  Wait-ForExit $script:serverProcess

  Set-RestartProgress 'Останавливаю панель...' 25
  $script:webRestartEnabled = $true
  $script:webWasRunning = $false
  Stop-Tree $script:webProcess
  Wait-ForExit $script:webProcess

  Set-RestartProgress 'Останавливаю worker...' 40
  $script:workerRestartEnabled = $true
  $script:workerWasRunning = $false
  Stop-Tree $script:workerProcess
  Wait-ForExit $script:workerProcess

  Set-RestartProgress 'Запускаю сервер...' 55
  $script:serverProcess = Start-ManagedProcess '-Server'
  $script:serverWasRunning = $true

  Set-RestartProgress 'Запускаю панель...' 70
  $script:webProcess = Start-ManagedProcess '-Web'
  $script:webWasRunning = $true

  Set-RestartProgress 'Запускаю worker...' 82
  $script:workerProcess = Start-ManagedProcess '-Worker'
  $script:workerWasRunning = $true

  Set-RestartProgress 'Проверяю health...' 92
  $serverHealthy = Get-QuietHealth $serverUrl
  $webHealthy = Get-QuietHealth $webUrl
  $workerHealthy = Get-QuietHealth $workerUrl

  if ($serverHealthy -and $webHealthy -and $workerHealthy) {
    Set-RestartProgress 'Готово' 100
  } else {
    Set-RestartProgress 'Готово (есть проблемы, см. трей)' 100
  }
  Close-RestartProgress
})
$menuExit.Add_Click({ $form.Close() })

$appIconPath = Join-Path $root 'frontend\web\public\favicon.ico'
$appBrandIcon = $null
if (Test-Path -LiteralPath $appIconPath) {
  try {
    $appBrandIcon = New-Object System.Drawing.Icon($appIconPath)
  } catch {
    $appBrandIcon = $null
  }
}
if (-not $appBrandIcon) {
  $appBrandIcon = [System.Drawing.SystemIcons]::Application
}

$appIcon.ContextMenuStrip = $appMenu
$appIcon.Visible = $true
$appIcon.Text = 'EvoHime: запускается...'
$appIcon.Icon = $appBrandIcon

$appIcon.Add_MouseUp({
  param($sender, $eventArgs)
  if ($eventArgs.Button -eq [System.Windows.Forms.MouseButtons]::Left) {
    Open-Url $webUrl
  }
})

function Update-AppIconState {
  param([bool]$ServerRunning, [bool]$WebRunning, [bool]$WorkerRunning)

  $mark = { param($ok) if ($ok) { 'OK' } else { 'X' } }
  $tooltip = "EvoHime | Server:$(& $mark $ServerRunning) Web:$(& $mark $WebRunning) Worker:$(& $mark $WorkerRunning)"
  # NotifyIcon.Text throws if >= 64 chars (WinForms/.NET Framework limit). Icon stays the EvoHime brand icon regardless of state.
  $appIcon.Text = $tooltip.Substring(0, [Math]::Min(63, $tooltip.Length))
}

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

  Update-AppIconState -ServerRunning $serverRunning -WebRunning $webRunning -WorkerRunning $workerRunning

  if ($script:serverWasRunning -and -not $serverRunning) {
    $appIcon.ShowBalloonTip(3000, 'EvoHime', 'Сервер остановлен', [System.Windows.Forms.ToolTipIcon]::Error)
    if ($script:serverRestartEnabled) {
      Write-Host '[EvoHime] Сервер завершился неожиданно, перезапускаю...'
      $script:serverProcess = Start-ManagedProcess '-Server'
      $serverRunning = -not $script:serverProcess.HasExited
    }
  }
  if ($script:webWasRunning -and -not $webRunning) {
    $appIcon.ShowBalloonTip(3000, 'EvoHime', 'Панель остановлена', [System.Windows.Forms.ToolTipIcon]::Error)
    if ($script:webRestartEnabled) {
      Write-Host '[EvoHime] Панель завершилась неожиданно, перезапускаю...'
      $script:webProcess = Start-ManagedProcess '-Web'
      $webRunning = -not $script:webProcess.HasExited
    }
  }
  if ($script:workerWasRunning -and -not $workerRunning) {
    $appIcon.ShowBalloonTip(3000, 'EvoHime', 'Python worker остановлен', [System.Windows.Forms.ToolTipIcon]::Error)
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
  $appIcon.Visible = $false
  $appIcon.Dispose()
  $appMenu.Dispose()
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
Write-Host 'Click the tray icon (or "Open Dashboard") to open the panel. Right-click for Stop/Restart/Exit.'

[System.Windows.Forms.Application]::Run($form)
