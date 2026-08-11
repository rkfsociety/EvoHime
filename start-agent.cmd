@echo off
setlocal

cd /d "%~dp0"

where pwsh.exe >nul 2>&1
if errorlevel 1 goto noPowerShell

echo [EvoHime] Build and start native agent...
echo [EvoHime] This window stays open while the agent is running.
if /I "%~1"=="-SkipBuild" goto skipBuild
pwsh.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\build-windows-native.ps1" -OutputPath "%~dp0.evohime-native\windows-x64" -Configuration Debug
if errorlevel 1 goto buildFailed
goto launch

:skipBuild
:launch
pwsh.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0start-dev.ps1" -SkipBuild
goto finish

:buildFailed
echo [EvoHime] Native build failed.
pause

:finish
set "exitCode=%errorlevel%"

endlocal & exit /b %exitCode%

:noPowerShell
echo [EvoHime] PowerShell 7 (pwsh.exe) was not found.
echo Install PowerShell 7 and retry.
endlocal
exit /b 1
