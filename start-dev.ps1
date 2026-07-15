param(
  [switch]$Server,
  [switch]$Web
)

$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot

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

if ($Server) {
  Set-Location $root

  $env:DATABASE_URL = 'postgres://evohime:evohime@localhost:5432/evohime'
  $env:BIND_ADDR = '0.0.0.0:3000'
  $env:WORKSPACE_ROOT = '.'
  $env:DEMO_FILE_PATH = 'docs/sample-context.md'
  $env:MODEL_PROVIDER = 'literouter'
  $env:LITEROUTER_API_KEY = ''
  $env:LITEROUTER_BASE_URL = 'https://api.literouter.com/v1'
  $env:LITEROUTER_MODEL = 'deepseek:free'

  & (Get-CargoExe) run -p evohime-server
  exit $LASTEXITCODE
}

if ($Web) {
  Set-Location (Join-Path $root 'frontend\web')
  & npm run dev
  exit $LASTEXITCODE
}

Write-Host '[EvoHime] Local start without Docker'
Write-Host ''
Write-Host 'Important: PostgreSQL must already be running locally.'
Write-Host ''

Set-Location $root

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

$powershell = Join-Path $PSHOME 'powershell.exe'
$script = $PSCommandPath

Start-Process -WindowStyle Hidden -FilePath $powershell -ArgumentList @(
  '-NoProfile',
  '-ExecutionPolicy',
  'Bypass',
  '-File',
  $script,
  '-Server'
)

Start-Process -WindowStyle Hidden -FilePath $powershell -ArgumentList @(
  '-NoProfile',
  '-ExecutionPolicy',
  'Bypass',
  '-File',
  $script,
  '-Web'
)

Write-Host ''
Write-Host 'Server: http://localhost:3000/health'
Write-Host 'Web:    http://localhost:5173'
Write-Host ''
Write-Host 'If the backend stops with a database error, start PostgreSQL and run this file again.'
