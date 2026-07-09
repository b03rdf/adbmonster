@echo off
chcp 65001 >nul
setlocal

echo === ADB Monster Build ===
cd /d "%~dp0"

echo [1/2] Running npm install if needed...
if not exist "node_modules" (
  call npm install
  if errorlevel 1 goto :err
)

echo [2/2] Building Tauri app...
call npm run tauri build
if errorlevel 1 goto :err

echo.
echo Build complete. Artifacts:
echo   src-tauri\target\release\adb-monster.exe
echo   src-tauri\target\release\bundle\msi\
echo   src-tauri\target\release\bundle\nsis\
echo.
pause
exit /b 0

:err
echo.
echo Build failed with error %errorlevel%.
pause
exit /b %errorlevel%
