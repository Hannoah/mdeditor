@echo off
rem ============================================================
rem  mdeditor launcher (shortcut substitute)
rem  - double click : run server in this folder + open browser
rem  - drop a file  : open that .md file
rem  stop: Ctrl + C in this window
rem ============================================================
set "EXE=%~dp0target\release\mdeditor.exe"
if not exist "%EXE%" (
  echo [mdeditor] release binary not found. Building: cargo build --release
  pushd "%~dp0"
  cargo build --release
  popd
  if not exist "%EXE%" (
    echo [mdeditor] build failed. Install Rust ^(https://rustup.rs^) and retry.
    pause
    exit /b 1
  )
)
rem work folder = current folder when double-clicked from the project folder;
rem when a file is passed, its parent folder is used.
if "%~1"=="" cd /d "%~dp0"
"%EXE%" %*
pause
