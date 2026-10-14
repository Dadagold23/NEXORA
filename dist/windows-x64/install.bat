@echo off
REM NEXORA / NXL - Windows installer entry point for Command Prompt users.
REM Delegates to install.ps1, which does the real work: SHA-256 verification,
REM user-level install, PATH configuration and post-install verification.
REM
REM   install.bat              install for the current user
REM   install.bat --dryrun     verify and show what would happen, change nothing
REM   install.bat --uninstall  remove an existing installation

setlocal enabledelayedexpansion
set "SCRIPT_DIR=%~dp0"
set "PS1=%SCRIPT_DIR%install.ps1"

set "ARGS="
if /I "%~1"=="--dryrun" set "ARGS=-DryRun"
if /I "%~1"=="--uninstall" set "ARGS=-Uninstall"

if not exist "%PS1%" goto :missing

where powershell >nul 2>&1
if errorlevel 1 goto :nopowershell

powershell -NoProfile -ExecutionPolicy Bypass -File "%PS1%" %ARGS%
set "RC=%ERRORLEVEL%"
if not "%RC%"=="0" goto :failed
exit /b 0

:missing
echo ERROR: install.ps1 was not found next to install.bat in "%SCRIPT_DIR%".
echo Re-download the complete NEXORA windows-x64 release package.
exit /b 1

:nopowershell
echo ERROR: Windows PowerShell is required but was not found on PATH.
exit /b 1

:failed
echo.
echo Installation did not complete ^(exit %RC%^). Nothing was changed.
exit /b %RC%