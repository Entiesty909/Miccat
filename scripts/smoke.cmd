@echo off
setlocal

set "EXE=%~dp0..\target\debug\mintcat-proc-mic.exe"
for %%I in ("%EXE%") do set "EXE=%%~fI"
if not exist "%EXE%" (
    call "%~dp0build-dev.cmd" build
    if errorlevel 1 exit /b 1
)

"%EXE%" --list-devices
if errorlevel 1 exit /b 1

"%EXE%" --list-processes
if errorlevel 1 exit /b 1

set "SNAPSHOT=%TEMP%\processmic-smoke.raw"
set "MENU_SNAPSHOT=%TEMP%\processmic-smoke-menu.raw"
set "CATEGORY_MENU_SNAPSHOT=%TEMP%\processmic-smoke-category-menu.raw"
set "CATEGORY_ROW_MENU_SNAPSHOT=%TEMP%\processmic-smoke-category-row-menu.raw"
"%EXE%" --snapshot-ui --out "%SNAPSHOT%"
if errorlevel 1 exit /b 1
"%EXE%" --snapshot-ui --menu --out "%MENU_SNAPSHOT%"
if errorlevel 1 exit /b 1
"%EXE%" --snapshot-ui --category-menu --out "%CATEGORY_MENU_SNAPSHOT%"
if errorlevel 1 exit /b 1
"%EXE%" --snapshot-ui --category-row-menu --out "%CATEGORY_ROW_MENU_SNAPSHOT%"
if errorlevel 1 exit /b 1

echo Snapshots saved to:
echo   %SNAPSHOT%
echo   %MENU_SNAPSHOT%
echo   %CATEGORY_MENU_SNAPSHOT%
echo   %CATEGORY_ROW_MENU_SNAPSHOT%
