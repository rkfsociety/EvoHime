param(
  [switch]$InstallPostgres,
  [switch]$ApplyMigrations,
  [switch]$Stop
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$stateRoot = Join-Path $env:LOCALAPPDATA 'EvoHime'
$postgresRoot = Join-Path $stateRoot 'postgresql-16'
$postgresData = Join-Path $stateRoot 'postgres-data'
$postgresLog = Join-Path $stateRoot 'postgres.log'
$postgresZip = Join-Path $stateRoot 'postgresql-16.14-1-windows-x64-binaries.zip'
$postgresUrl = 'https://get.enterprisedb.com/postgresql/postgresql-16.14-1-windows-x64-binaries.zip'
$databaseUrl = 'postgres://evohime:evohime@localhost:5432/evohime'

function Get-PostgresBin([string]$name) {
  $local = Join-Path $postgresRoot "bin\$name.exe"
  if (Test-Path -LiteralPath $local) {
    return $local
  }

  $command = Get-Command $name -ErrorAction SilentlyContinue
  if ($command) {
    return $command.Source
  }

  return $null
}

function Add-PostgresToPath {
  $bin = Join-Path $postgresRoot 'bin'
  if ((Test-Path -LiteralPath $bin) -and (($env:PATH -split ';') -notcontains $bin)) {
    $env:PATH = "$bin;$env:PATH"
  }
}

function Ensure-PortablePostgres {
  New-Item -ItemType Directory -Force -Path $stateRoot | Out-Null
  $psql = Get-PostgresBin 'psql'
  if ($psql) {
    Add-PostgresToPath
    return
  }

  if (-not $InstallPostgres) {
    throw "PostgreSQL не найден. Запусти .\scripts\setup-local.ps1 -InstallPostgres -ApplyMigrations."
  }

  Write-Host "Загрузка portable PostgreSQL 16.14..."
  Invoke-WebRequest -Uri $postgresUrl -OutFile $postgresZip
  $extractRoot = Join-Path $stateRoot 'postgresql-extract'
  if (Test-Path -LiteralPath $extractRoot) {
    Remove-Item -LiteralPath $extractRoot -Recurse -Force
  }
  Expand-Archive -LiteralPath $postgresZip -DestinationPath $extractRoot -Force
  $payload = Get-ChildItem -LiteralPath $extractRoot -Directory | Select-Object -First 1
  if (-not $payload) {
    $payload = Get-Item -LiteralPath $extractRoot
  }
  if (Test-Path -LiteralPath $postgresRoot) {
    Remove-Item -LiteralPath $postgresRoot -Recurse -Force
  }
  Move-Item -LiteralPath $payload.FullName -Destination $postgresRoot
  Remove-Item -LiteralPath $extractRoot -Recurse -Force
  Add-PostgresToPath

  if (-not (Get-PostgresBin 'psql')) {
    throw "Portable PostgreSQL распакован некорректно: psql.exe не найден в $postgresRoot."
  }
}

function Invoke-Psql([string]$database, [string]$sql) {
  $psql = Get-PostgresBin 'psql'
  $env:PGPASSWORD = 'evohime'
  try {
    & $psql -h localhost -p 5432 -U postgres -d $database -v ON_ERROR_STOP=1 -Atqc $sql
    if ($LASTEXITCODE -ne 0) {
      throw "psql завершился с кодом $LASTEXITCODE."
    }
  } finally {
    Remove-Item Env:PGPASSWORD -ErrorAction SilentlyContinue
  }
}

function Test-PostgresRunning {
  $pgCtl = Get-PostgresBin 'pg_ctl'
  if (-not $pgCtl -or -not (Test-Path -LiteralPath $postgresData)) {
    return $false
  }
  & $pgCtl -D $postgresData status *> $null
  return $LASTEXITCODE -eq 0
}

function Test-PostgresPort {
  $listener = Get-NetTCPConnection -LocalPort 5432 -State Listen -ErrorAction SilentlyContinue
  if ($listener) {
    return $true
  }

  $client = New-Object System.Net.Sockets.TcpClient
  try {
    $connection = $client.ConnectAsync('127.0.0.1', 5432)
    return $connection.Wait(1000) -and $client.Connected
  } catch {
    return $false
  } finally {
    $client.Dispose()
  }
}

function Start-LocalPostgres {
  $initdb = Get-PostgresBin 'initdb'
  $pgCtl = Get-PostgresBin 'pg_ctl'
  if (-not $initdb -or -not $pgCtl) {
    throw 'Команды initdb/pg_ctl не найдены.'
  }

  if (-not (Test-Path -LiteralPath (Join-Path $postgresData 'PG_VERSION'))) {
    New-Item -ItemType Directory -Force -Path $postgresData | Out-Null
    $passwordFile = Join-Path $stateRoot 'postgres-superuser-password.txt'
    Set-Content -LiteralPath $passwordFile -Value 'evohime' -NoNewline
    try {
      & $initdb -D $postgresData -U postgres --pwfile=$passwordFile --auth-local=trust --auth-host=scram-sha-256 --encoding=UTF8
      if ($LASTEXITCODE -ne 0) {
        throw "initdb завершился с кодом $LASTEXITCODE."
      }
    } finally {
      Remove-Item -LiteralPath $passwordFile -Force -ErrorAction SilentlyContinue
    }
  }

  if ((Test-PostgresRunning) -or (Test-PostgresPort)) {
    return
  }

  if (-not (Test-PostgresRunning)) {
    & $pgCtl -D $postgresData -l $postgresLog -o '"-p 5432"' start
    if ($LASTEXITCODE -ne 0) {
      throw "PostgreSQL не запустился. Подробности: $postgresLog"
    }
  }
}

function Stop-LocalPostgres {
  $pgCtl = Get-PostgresBin 'pg_ctl'
  if ($pgCtl -and (Test-PostgresRunning)) {
    & $pgCtl -D $postgresData stop -m fast
  }
}

function Ensure-Database {
  Invoke-Psql 'postgres' "DO `$`$ BEGIN IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'evohime') THEN CREATE ROLE evohime LOGIN PASSWORD 'evohime'; ELSE ALTER ROLE evohime WITH LOGIN PASSWORD 'evohime'; END IF; END `$`$;"
  $exists = Invoke-Psql 'postgres' "SELECT 1 FROM pg_database WHERE datname = 'evohime';"
  if (($exists -join '').Trim() -ne '1') {
    Invoke-Psql 'postgres' 'CREATE DATABASE evohime OWNER evohime;'
  }
}

function Apply-Migrations {
  $psql = Get-PostgresBin 'psql'
  $env:PGPASSWORD = 'evohime'
  try {
    Get-ChildItem -LiteralPath (Join-Path $root 'migrations') -Filter '*.sql' | Sort-Object Name | ForEach-Object {
      & $psql -h localhost -p 5432 -U evohime -d evohime -v ON_ERROR_STOP=1 -f $_.FullName
      if ($LASTEXITCODE -ne 0) {
        throw "Миграция $($_.Name) завершилась с кодом $LASTEXITCODE."
      }
    }
  } finally {
    Remove-Item Env:PGPASSWORD -ErrorAction SilentlyContinue
  }
}

if ($Stop) {
  Stop-LocalPostgres
  exit 0
}

Ensure-PortablePostgres
Start-LocalPostgres
Ensure-Database
if ($ApplyMigrations) {
  Apply-Migrations
}

Write-Output "DATABASE_URL=$databaseUrl"
Write-Output "PostgreSQL готов: $postgresData"
