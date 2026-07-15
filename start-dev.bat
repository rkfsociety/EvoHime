@echo off
setlocal EnableExtensions

set "ROOT=%~dp0"
cd /d "%ROOT%"

echo [EvoHime] Local start without Docker
echo.
echo Important: PostgreSQL must already be running locally.
echo.

if not exist ".env" (
  if exist ".env.example" (
    copy ".env.example" ".env" >nul
    echo Created .env from .env.example
  ) else (
    echo ERROR: .env.example not found.
    pause
    exit /b 1
  )
)

where cargo >nul 2>nul
if errorlevel 1 (
  echo ERROR: cargo not found in PATH.
  pause
  exit /b 1
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

start "EvoHime Server" /D "%ROOT%" cmd /k "cargo run -p evohime-server"
start "EvoHime Web" /D "%ROOT%\frontend\web" cmd /k "npm run dev"

echo.
echo Server: http://localhost:3000/health
echo Web:    http://localhost:5173
echo.
echo If the backend stops with a database error, start PostgreSQL and run this file again.

endlocal
exit /b 0
