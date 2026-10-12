@echo off
setlocal

call "%~dp0build-dev.cmd" build
if errorlevel 1 exit /b 1

set "EXE=%~dp0..\target\debug\miccat.exe"
for %%I in ("%EXE%") do set "EXE=%%~fI"
if not exist "%EXE%" (
    echo [error] binary not found: %EXE%
    exit /b 1
)
echo %EXE%
