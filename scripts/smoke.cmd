@echo off
setlocal

set "EXE=%~dp0..\target\debug\miccat.exe"
for %%I in ("%EXE%") do set "EXE=%%~fI"
if not exist "%EXE%" (
    call "%~dp0build-dev.cmd" build
    if errorlevel 1 exit /b 1
)

"%EXE%" --list-devices
if errorlevel 1 exit /b 1

"%EXE%" --list-processes
if errorlevel 1 exit /b 1

set "SNAPSHOT=%TEMP%\miccat-smoke.raw"
set "MENU_SNAPSHOT=%TEMP%\miccat-smoke-menu.raw"
set "CATEGORY_MENU_SNAPSHOT=%TEMP%\miccat-smoke-category-menu.raw"
set "CATEGORY_ROW_MENU_SNAPSHOT=%TEMP%\miccat-smoke-category-row-menu.raw"
set "SOURCE_PROCESS_SNAPSHOT=%TEMP%\miccat-smoke-source-process.raw"
set "SOURCE_AUDIO_SNAPSHOT=%TEMP%\miccat-smoke-source-audio.raw"
set "SOURCE_MICROPHONE_SNAPSHOT=%TEMP%\miccat-smoke-source-microphone.raw"
set "SOURCE_REOPEN_SNAPSHOT=%TEMP%\miccat-smoke-source-reopen.raw"
set "SOURCE_PROCESS_CLICK=%TEMP%\miccat-smoke-source-process-click.raw"
set "SOURCE_AUDIO_CLICK=%TEMP%\miccat-smoke-source-audio-click.raw"
set "SOURCE_MICROPHONE_CLICK=%TEMP%\miccat-smoke-source-microphone-click.raw"
set "SOURCE_MENU_CLICK=%TEMP%\miccat-smoke-source-menu-click.raw"
set "SOURCE_METER_SNAPSHOT=%TEMP%\miccat-smoke-source-meter.raw"
"%EXE%" --snapshot-ui --out "%SNAPSHOT%"
if errorlevel 1 exit /b 1
"%EXE%" --snapshot-ui --menu --out "%MENU_SNAPSHOT%"
if errorlevel 1 exit /b 1
"%EXE%" --snapshot-ui --category-menu --out "%CATEGORY_MENU_SNAPSHOT%"
if errorlevel 1 exit /b 1
"%EXE%" --snapshot-ui --category-row-menu --out "%CATEGORY_ROW_MENU_SNAPSHOT%"
if errorlevel 1 exit /b 1
"%EXE%" --snapshot-ui --page route --source-dialog-tab process --out "%SOURCE_PROCESS_SNAPSHOT%"
if errorlevel 1 exit /b 1
"%EXE%" --snapshot-ui --page route --source-dialog-tab process --source-dialog-reopen --out "%SOURCE_REOPEN_SNAPSHOT%"
if errorlevel 1 exit /b 1
"%EXE%" --snapshot-ui --page route --source-dialog-tab audio --out "%SOURCE_AUDIO_SNAPSHOT%"
if errorlevel 1 exit /b 1
"%EXE%" --snapshot-ui --page route --source-dialog-tab microphone --out "%SOURCE_MICROPHONE_SNAPSHOT%"
if errorlevel 1 exit /b 1
"%EXE%" --snapshot-ui --page route --source-dialog-click-tab process --out "%SOURCE_PROCESS_CLICK%"
if errorlevel 1 exit /b 1
"%EXE%" --snapshot-ui --page route --source-dialog-click-tab audio --out "%SOURCE_AUDIO_CLICK%"
if errorlevel 1 exit /b 1
"%EXE%" --snapshot-ui --page route --source-dialog-click-tab microphone --out "%SOURCE_MICROPHONE_CLICK%"
if errorlevel 1 exit /b 1
"%EXE%" --snapshot-ui --page route --source-menu-click --out "%SOURCE_MENU_CLICK%"
if errorlevel 1 exit /b 1
"%EXE%" --snapshot-ui --page route --source-meter --out "%SOURCE_METER_SNAPSHOT%"
if errorlevel 1 exit /b 1

echo Snapshots saved to:
echo   %SNAPSHOT%
echo   %MENU_SNAPSHOT%
echo   %CATEGORY_MENU_SNAPSHOT%
echo   %CATEGORY_ROW_MENU_SNAPSHOT%
echo   %SOURCE_PROCESS_SNAPSHOT%
echo   %SOURCE_AUDIO_SNAPSHOT%
echo   %SOURCE_MICROPHONE_SNAPSHOT%
echo   %SOURCE_PROCESS_CLICK%
echo   %SOURCE_AUDIO_CLICK%
echo   %SOURCE_MICROPHONE_CLICK%
echo   %SOURCE_MENU_CLICK%
echo   %SOURCE_METER_SNAPSHOT%
echo   %SOURCE_REOPEN_SNAPSHOT%
