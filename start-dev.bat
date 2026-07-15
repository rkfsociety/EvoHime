@echo off
setlocal EnableExtensions

set "ROOT=%~dp0"
cd /d "%ROOT%"

echo [EvoHime] Local start without Docker
echo.
echo Important: PostgreSQL must already be running locally.
echo.

set "ENV_FILE=%ROOT%local-notes.txt"
if not exist "%ENV_FILE%" (
  >"%ENV_FILE%" (
    echo # EvoHime local launch notes
    echo DATABASE_URL=postgres://evohime:evohime@localhost:5432/evohime
    echo BIND_ADDR=0.0.0.0:3000
    echo WORKSPACE_ROOT=.
    echo DEMO_FILE_PATH=docs/sample-context.md
    echo MODEL_PROVIDER=literouter
    echo LITEROUTER_API_KEY=
    echo LITEROUTER_BASE_URL=https://api.literouter.com/v1
    echo LITEROUTER_MODEL=deepseek:free
  )
  echo Created local-notes.txt with local launch settings
)

for /f "usebackq tokens=1,* delims==" %%A in ("%ENV_FILE%") do (
  if /I "%%A"=="DATABASE_URL" set "DATABASE_URL=%%B"
  if /I "%%A"=="BIND_ADDR" set "BIND_ADDR=%%B"
  if /I "%%A"=="WORKSPACE_ROOT" set "WORKSPACE_ROOT=%%B"
  if /I "%%A"=="DEMO_FILE_PATH" set "DEMO_FILE_PATH=%%B"
  if /I "%%A"=="MODEL_PROVIDER" set "MODEL_PROVIDER=%%B"
  if /I "%%A"=="LITEROUTER_API_KEY" set "LITEROUTER_API_KEY=%%B"
  if /I "%%A"=="LITEROUTER_BASE_URL" set "LITEROUTER_BASE_URL=%%B"
  if /I "%%A"=="LITEROUTER_MODEL" set "LITEROUTER_MODEL=%%B"
)

if not defined DATABASE_URL (
  set "DATABASE_URL=postgres://evohime:evohime@localhost:5432/evohime"
)
if not defined BIND_ADDR set "BIND_ADDR=0.0.0.0:3000"
if not defined WORKSPACE_ROOT set "WORKSPACE_ROOT=."
if not defined DEMO_FILE_PATH set "DEMO_FILE_PATH=docs/sample-context.md"
if not defined MODEL_PROVIDER set "MODEL_PROVIDER=literouter"
if not defined LITEROUTER_BASE_URL set "LITEROUTER_BASE_URL=https://api.literouter.com/v1"
if not defined LITEROUTER_MODEL set "LITEROUTER_MODEL=deepseek:free"

where cargo >nul 2>nul
if errorlevel 1 (
  if exist "%USERPROFILE%\.cargo\bin\cargo.exe" (
    set "CARGO_EXE=%USERPROFILE%\.cargo\bin\cargo.exe"
  ) else (
    echo ERROR: cargo not found in PATH and %USERPROFILE%\.cargo\bin\cargo.exe is missing.
    pause
    exit /b 1
  )
) else (
  set "CARGO_EXE=cargo"
)

where npm >nul 2>nul
if errorlevel 1 (
  echo ERROR: npm not found in PATH.
  pause
  exit /b 1
)

if not exist "frontend\web\node_modules" (
  echo Installing frontend dependencies...
  pushd "frontend\web"
  call npm install
  if errorlevel 1 (
    popd
    echo ERROR: npm install failed.
    pause
    exit /b 1
  )
  popd
)

start "EvoHime Server" /D "%ROOT%" cmd /k "set DATABASE_URL=%DATABASE_URL%&& set BIND_ADDR=%BIND_ADDR%&& set WORKSPACE_ROOT=%WORKSPACE_ROOT%&& set DEMO_FILE_PATH=%DEMO_FILE_PATH%&& set MODEL_PROVIDER=%MODEL_PROVIDER%&& set LITEROUTER_API_KEY=%LITEROUTER_API_KEY%&& set LITEROUTER_BASE_URL=%LITEROUTER_BASE_URL%&& set LITEROUTER_MODEL=%LITEROUTER_MODEL%&& \"%CARGO_EXE%\" run -p evohime-server"
start "EvoHime Web" /D "%ROOT%\frontend\web" cmd /k "npm run dev"

echo.
echo Server: http://localhost:3000/health
echo Web:    http://localhost:5173
echo.
echo If the backend stops with a database error, start PostgreSQL and run this file again.

endlocal
exit /b 0
