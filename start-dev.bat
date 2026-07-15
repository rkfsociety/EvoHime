@echo off
setlocal EnableExtensions

set "ROOT=%~dp0"
cd /d "%ROOT%"

echo [EvoHime] Local start without Docker
echo.
echo Important: PostgreSQL must already be running locally.
echo.

set "DATABASE_URL=postgres://evohime:evohime@localhost:5432/evohime"
set "BIND_ADDR=0.0.0.0:3000"
set "WORKSPACE_ROOT=."
set "DEMO_FILE_PATH=docs/sample-context.md"
set "MODEL_PROVIDER=literouter"
set "LITEROUTER_API_KEY="
set "LITEROUTER_BASE_URL=https://api.literouter.com/v1"
set "LITEROUTER_MODEL=deepseek:free"

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

start "EvoHime Server" /D "%ROOT%" cmd /k "set DATABASE_URL=%DATABASE_URL%&& set BIND_ADDR=%BIND_ADDR%&& set WORKSPACE_ROOT=%WORKSPACE_ROOT%&& set DEMO_FILE_PATH=%DEMO_FILE_PATH%&& set MODEL_PROVIDER=%MODEL_PROVIDER%&& set LITEROUTER_API_KEY=%LITEROUTER_API_KEY%&& set LITEROUTER_BASE_URL=%LITEROUTER_BASE_URL%&& set LITEROUTER_MODEL=%LITEROUTER_MODEL%&& call ""%CARGO_EXE%"" run -p evohime-server"
start "EvoHime Web" /D "%ROOT%\frontend\web" cmd /k "npm run dev"

echo.
echo Server: http://localhost:3000/health
echo Web:    http://localhost:5173
echo.
echo If the backend stops with a database error, start PostgreSQL and run this file again.

endlocal
exit /b 0
