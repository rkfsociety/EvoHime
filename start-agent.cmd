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
if /I "%~1"=="-SkipBuild" (
    pwsh.exe -NoProfile -ExecutionPolicy Bypass -NoExit -File "%~dp0start-dev.ps1" -SkipBuild
) else (
    pwsh.exe -NoProfile -ExecutionPolicy Bypass -NoExit -Command "& '%~dp0scripts\build-windows-native.ps1' -OutputPath '%~dp0.evohime-native\windows-x64' -Configuration Debug; if (`$LASTEXITCODE -eq 0) { & '%~dp0start-dev.ps1' -SkipBuild } else { Write-Error 'Сборка native-агента завершилась с ошибкой.' }"
)
set "exitCode=%errorlevel%"

endlocal & exit /b %exitCode%
