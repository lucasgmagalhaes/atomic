@echo off
REM Convenience launcher: runs start-opencode-remote.ps1 with an execution
REM policy bypass scoped to just this process, so you can double-click
REM this .bat file without changing your system-wide PowerShell script
REM policy.
REM
REM Usage:
REM   start-opencode-remote.bat [port] [project-folder]
REM
REM opencode is always rooted at the folder this .bat file lives in,
REM regardless of what working directory Windows happened to launch the
REM .bat with. Pass a second argument to point at a different folder
REM instead.
setlocal

REM %~dp0 always ends with a trailing backslash (e.g. "E:\GitHub\nimble\").
REM Left as-is, wrapping it in quotes ("E:\GitHub\nimble\") makes the
REM trailing backslash escape the closing quote once passed through to
REM powershell.exe's own argument parsing, corrupting the path it
REM receives - this strips it first so the quoted argument below is safe.
set "SCRIPT_DIR=%~dp0"
if "%SCRIPT_DIR:~-1%"=="\" set "SCRIPT_DIR=%SCRIPT_DIR:~0,-1%"

set "PORT_ARG="
if not "%~1"=="" set "PORT_ARG=-Port %~1"

set "PROJECT_DIR=%SCRIPT_DIR%"
if not "%~2"=="" set "PROJECT_DIR=%~2"

powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0start-opencode-remote.ps1" %PORT_ARG% -ProjectPath "%PROJECT_DIR%"
pause
