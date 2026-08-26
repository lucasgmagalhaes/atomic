@echo off
REM Convenience launcher: runs start-opencode-remote.ps1 with an execution
REM policy bypass scoped to just this process, so you can double-click
REM this .bat file without changing your system-wide PowerShell script
REM policy.
REM
REM Usage:
REM   start-opencode-remote.bat [port] [project-folder]
REM
REM opencode is always rooted at the folder this .bat file lives in
REM (%~dp0, explicitly passed as -ProjectPath below) - i.e. just this one
REM project (nimble), regardless of what working directory Windows
REM happened to launch the .bat with (a shortcut/Start Menu entry often
REM defaults to C:\ or C:\Windows\System32, which is what caused "I can
REM only reach C:\" before this was pinned down explicitly). Pass a second
REM argument to point at a different folder instead.
setlocal

set "PORT_ARG="
if not "%~1"=="" set "PORT_ARG=-Port %~1"

set "PROJECT_ARG=-ProjectPath \"%~dp0\""
if not "%~2"=="" set "PROJECT_ARG=-ProjectPath \"%~2\""

powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0start-opencode-remote.ps1" %PORT_ARG% %PROJECT_ARG%
pause
