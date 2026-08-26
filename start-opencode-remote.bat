@echo off
REM Convenience launcher: runs start-opencode-remote.ps1 with an execution
REM policy bypass scoped to just this process, so you can double-click
REM this .bat file without changing your system-wide PowerShell script
REM policy.
REM
REM Usage:
REM   start-opencode-remote.bat [port] [project-folder]
REM
REM If [project-folder] is omitted, opencode is rooted at E:\GitHub (see
REM start-opencode-remote.ps1's own -ProjectPath default) - NOT wherever
REM Windows happened to launch this .bat from. That's what fixes "I can
REM only reach C:\": a shortcut/Start Menu entry often launches with C:\
REM (or C:\Windows\System32) as the working directory, which opencode
REM would otherwise treat as its project root.
setlocal

set "PORT_ARG="
if not "%~1"=="" set "PORT_ARG=-Port %~1"

set "PROJECT_ARG="
if not "%~2"=="" set "PROJECT_ARG=-ProjectPath \"%~2\""

powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0start-opencode-remote.ps1" %PORT_ARG% %PROJECT_ARG%
pause
