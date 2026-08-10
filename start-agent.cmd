@echo off
setlocal

cd /d "%~dp0"

where pwsh.exe >nul 2>&1
if errorlevel 1 (
    echo [EvoHime] Не найден PowerShell 7 (pwsh.exe).
    echo Установите PowerShell 7 и повторите запуск.
    endlocal
    exit /b 1
)

echo [EvoHime] Сборка и запуск native-агента...
echo [EvoHime] Это окно останется открытым, пока работает агент.
echo [EvoHime] Для завершения сначала закройте приложение, затем введите exit.
pwsh.exe -NoProfile -ExecutionPolicy Bypass -NoExit -File "%~dp0start-dev.ps1" %*
set "exitCode=%errorlevel%"

endlocal & exit /b %exitCode%
