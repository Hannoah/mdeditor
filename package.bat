@echo off
setlocal
chcp 65001 >nul
title MDEditor Package Builder
pushd "%~dp0"
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0package.ps1" %*
if errorlevel 1 (
    echo.
    echo [패키징 중 오류가 발생했습니다.]
)
echo.
pause
popd
