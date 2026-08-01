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

function Get-PostgresZip {
  # Архив ~310 МБ. Invoke-WebRequest в Windows PowerShell 5.1 перерисовывает
  # progress bar на каждый чанк и упирается в ~0.2 МБ/с — полчаса молчаливой
  # загрузки выглядит как зависший запуск. curl.exe (есть в Windows 10+) даёт
  # реальную скорость канала и умеет докачку по -C -, поэтому он основной путь,
  # а IWR остаётся запасным — уже с выключенным прогрессом.
  $expectedSize = $null
  try {
    $head = Invoke-WebRequest -Uri $postgresUrl -Method Head -TimeoutSec 30 -UseBasicParsing
    $expectedSize = [int64]($head.Headers['Content-Length'] | Select-Object -First 1)
  } catch {
    $expectedSize = $null
  }

  if ($expectedSize -and (Test-Path -LiteralPath $postgresZip) -and ((Get-Item -LiteralPath $postgresZip).Length -eq $expectedSize)) {
    Write-Host 'Архив уже скачан, повторная загрузка не требуется.'
    return
  }

  $curl = Get-Command curl.exe -ErrorAction SilentlyContinue
  if ($curl) {
    & $curl.Source -L --fail --retry 3 --retry-delay 2 -C - -o $postgresZip $postgresUrl
    if ($LASTEXITCODE -ne 0) {
      Remove-Item -LiteralPath $postgresZip -Force -ErrorAction SilentlyContinue
      throw "Не удалось скачать PostgreSQL (curl завершился с кодом $LASTEXITCODE): $postgresUrl"
    }
  } else {
    $previousProgress = $ProgressPreference
    $ProgressPreference = 'SilentlyContinue'
    try {
      Invoke-WebRequest -Uri $postgresUrl -OutFile $postgresZip -UseBasicParsing
    } finally {
      $ProgressPreference = $previousProgress
    }
  }

  if (-not (Test-Path -LiteralPath $postgresZip)) {
    throw "Не удалось скачать PostgreSQL: $postgresUrl"
  }
  $actualSize = (Get-Item -LiteralPath $postgresZip).Length
  if ($expectedSize -and $actualSize -ne $expectedSize) {
    Remove-Item -LiteralPath $postgresZip -Force -ErrorAction SilentlyContinue
    throw "Архив PostgreSQL скачан не полностью ($actualSize из $expectedSize байт). Запусти запуск ещё раз."
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

  Write-Host "Загрузка portable PostgreSQL 16.14 (~310 МБ)..."
  Get-PostgresZip
  $extractRoot = Join-Path $stateRoot 'postgresql-extract'
  if (Test-Path -LiteralPath $extractRoot) {
    Remove-Item -LiteralPath $extractRoot -Recurse -Force
  }
  Write-Host 'Распаковка архива...'
  # Expand-Archive в PS 5.1 распаковывает ~40 тысяч файлов дистрибутива минутами;
  # ZipFile::ExtractToDirectory делает то же самое в разы быстрее.
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  [System.IO.Compression.ZipFile]::ExtractToDirectory($postgresZip, $extractRoot)
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

function Test-PostgresReady {
  $pgIsReady = Get-PostgresBin 'pg_isready'
  if ($pgIsReady) {
    & $pgIsReady -h localhost -p 5432 -q *> $null
    return $LASTEXITCODE -eq 0
  }

  # Без pg_isready остаётся проверка порта — она грубее, но лучше, чем ничего.
  return Test-PostgresPort
}

function Wait-PostgresReady([int]$timeoutSeconds) {
  $deadline = [DateTime]::UtcNow.AddSeconds($timeoutSeconds)
  do {
    if (Test-PostgresReady) {
      return $true
    }
    Start-Sleep -Milliseconds 500
  } while ([DateTime]::UtcNow -lt $deadline)
  return $false
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

  # Раньше здесь хватало «слушает ли кто-то 5432», и это ловило сокет умирающего
  # сервера: запуск шёл дальше, а psql тут же падал с «сервер неожиданно закрыл
  # соединение». Единственный надёжный признак — сервер реально принимает
  # подключения, это и проверяем (pg_isready), с ожиданием и повтором старта.
  if (Wait-PostgresReady 3) {
    return
  }

  & $pgCtl -D $postgresData -l $postgresLog -o '"-p 5432"' start
  if ($LASTEXITCODE -ne 0) {
    # Типичный случай — остался postmaster.pid от убитого сервера: даём ему
    # доуйти и пробуем ещё раз, прежде чем сдаваться.
    Start-Sleep -Seconds 3
    & $pgCtl -D $postgresData -l $postgresLog -o '"-p 5432"' start
    if ($LASTEXITCODE -ne 0) {
      throw "PostgreSQL не запустился. Подробности: $postgresLog"
    }
  }

  if (-not (Wait-PostgresReady 60)) {
    throw "PostgreSQL запущен, но не принимает подключения на 5432. Подробности: $postgresLog"
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
  # Миграции идемпотентные (CREATE ... IF NOT EXISTS), поэтому на повторном
  # запуске psql забивает stderr сотнями «уже существует, пропускается».
  # В логе от этого не видно настоящих ошибок — глушим до warning.
  $previousPgOptions = $env:PGOPTIONS
  $env:PGOPTIONS = '-c client_min_messages=warning'
  try {
    Get-ChildItem -LiteralPath (Join-Path $root 'migrations') -Filter '*.sql' | Sort-Object Name | ForEach-Object {
      & $psql -h localhost -p 5432 -U evohime -d evohime -v ON_ERROR_STOP=1 -f $_.FullName
      if ($LASTEXITCODE -ne 0) {
        throw "Миграция $($_.Name) завершилась с кодом $LASTEXITCODE."
      }
    }
  } finally {
    Remove-Item Env:PGPASSWORD -ErrorAction SilentlyContinue
    if ($null -eq $previousPgOptions) {
      Remove-Item Env:PGOPTIONS -ErrorAction SilentlyContinue
    } else {
      $env:PGOPTIONS = $previousPgOptions
    }
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
