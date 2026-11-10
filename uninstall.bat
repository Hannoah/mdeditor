@echo off
setlocal
chcp 65001 >nul
title MDEditor Uninstall
pushd "%~dp0"
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0uninstall.ps1" %*
if errorlevel 1 (
    echo.
    echo [제거 중 오류가 발생했습니다.]
)
echo.
pause
popd
