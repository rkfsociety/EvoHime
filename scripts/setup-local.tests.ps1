$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$setup = Join-Path $PSScriptRoot 'setup-local.ps1'
$psql = Join-Path $env:LOCALAPPDATA 'EvoHime\postgresql-16\bin\psql.exe'
$pgIsReady = Join-Path $env:LOCALAPPDATA 'EvoHime\postgresql-16\bin\pg_isready.exe'

& $setup -ApplyMigrations
if ($LASTEXITCODE -ne 0) {
  throw "setup-local.ps1 failed with exit code $LASTEXITCODE."
}

if (-not (Test-Path -LiteralPath $psql)) {
  throw "psql not found: $psql"
}
if (-not (Test-Path -LiteralPath $pgIsReady)) {
  throw "pg_isready not found: $pgIsReady"
}

& $pgIsReady -h localhost -p 5432
if ($LASTEXITCODE -ne 0) {
  throw 'PostgreSQL is not accepting connections on localhost:5432.'
}

$env:PGPASSWORD = 'evohime'
try {
  $result = & $psql -h localhost -p 5432 -U evohime -d evohime -Atqc "select current_user || ':' || current_database();"
  if (($result -join '').Trim() -ne 'evohime:evohime') {
    throw "Unexpected SQL result: $result"
  }
} finally {
  Remove-Item Env:PGPASSWORD -ErrorAction SilentlyContinue
}

Write-Output 'setup-local smoke: PASS'
