@echo off
REM Convenience launcher: runs start-opencode-remote.ps1 with an execution
REM policy bypass scoped to just this process, so you can double-click
REM this .bat file without changing your system-wide PowerShell script
REM policy. Pass a port as the first argument if you want one other than
REM the default (4096), e.g.: start-opencode-remote.bat 8080
setlocal
set "PORT_ARG="
if not "%~1"=="" set "PORT_ARG=-Port %~1"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0start-opencode-remote.ps1" %PORT_ARG%
pause
