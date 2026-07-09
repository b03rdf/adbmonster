@echo off
chcp 65001 >nul
setlocal

echo === ADB Monster Dev Mode ===
cd /d "%~dp0"

if not exist "node_modules" (
  echo [Setup] Installing dependencies...
  call npm install
  if errorlevel 1 goto :err
)

echo Starting Tauri dev (Vite + Rust, with hot reload)...
call npm run tauri dev
if errorlevel 1 goto :err

pause
exit /b 0

:err
echo.
echo Dev server failed with error %errorlevel%.
pause
exit /b %errorlevel%
