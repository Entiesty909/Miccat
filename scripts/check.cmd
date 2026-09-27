@echo off
setlocal

call "%~dp0build-dev.cmd" fmt -- --check
if errorlevel 1 exit /b 1

call "%~dp0build-dev.cmd" clippy -- -D warnings
if errorlevel 1 exit /b 1

call "%~dp0build-dev.cmd" build
if errorlevel 1 exit /b 1

powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0check-lines.ps1"
exit /b %errorlevel%
