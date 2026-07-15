@echo off
setlocal EnableExtensions

set "ROOT=%~dp0"
set "POWERSHELL_EXE=%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe"

cd /d "%ROOT%"

"%POWERSHELL_EXE%" -NoProfile -ExecutionPolicy Bypass -File "%ROOT%start-dev.ps1"
exit /b %errorlevel%
