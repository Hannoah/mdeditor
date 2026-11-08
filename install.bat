@echo off
setlocal
chcp 65001 >nul
pushd "%~dp0"
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1"
if errorlevel 1 (
    echo.
    echo [오류가 발생했습니다.]
)
echo.
pause
popd

