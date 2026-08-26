@echo off
REM Convenience launcher: runs start-opencode-remote.ps1 with an execution
REM policy bypass scoped to just this process, so you can double-click
REM this .bat file without changing your system-wide PowerShell script
REM policy.
REM
REM Usage:
REM   start-opencode-remote.bat [port] [project-folder]
REM
REM If [project-folder] is omitted, opencode is rooted at the folder this
REM .bat file lives in (%~dp0), NOT wherever Windows happened to launch it
REM from - this is what fixes "I can only reach C:\": a shortcut/Start Menu
REM entry often launches with C:\ (or C:\Windows\System32) as the working
REM directory, which opencode would otherwise treat as its project root.
setlocal
cd /d "%~dp0"

set "PORT_ARG="
if not "%~1"=="" set "PORT_ARG=-Port %~1"

set "PROJECT_ARG=-ProjectPath \"%~dp0\""
if not "%~2"=="" set "PROJECT_ARG=-ProjectPath \"%~2\""

powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0start-opencode-remote.ps1" %PORT_ARG% %PROJECT_ARG%
pause
