# MDEditor Production Windows Uninstaller
# Copyright (c) 2026 MDEditor Team. All rights reserved.

[CmdletBinding()]
param(
    [switch]$Quiet
)

[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
if (-not $Quiet) {
    $Host.UI.RawUI.WindowTitle = "MDEditor 제거 마법사"
}

$AppName = "MDEditor"

function Write-Step {
    param([string]$Step, [string]$Message)
    if (-not $Quiet) {
        Write-Host " [$Step] " -NoNewline -ForegroundColor Cyan
        Write-Host $Message -ForegroundColor White
    }
}

function Write-Success {
    param([string]$Message)
    if (-not $Quiet) {
        Write-Host "       [✓] " -NoNewline -ForegroundColor Green
        Write-Host $Message -ForegroundColor Gray
    }
}

function Write-WarningMsg {
    param([string]$Message)
    if (-not $Quiet) {
        Write-Host "       [!] " -NoNewline -ForegroundColor Yellow
        Write-Host $Message -ForegroundColor Yellow
    }
}

if (-not $Quiet) {
    Write-Host ""
    Write-Host "  ███╗   ███╗██████╗ ███████╗██████╗ ██╗████████╗ ██████╗ ██████╗ " -ForegroundColor Cyan
    Write-Host "  ████╗ ████║██╔══██╗██╔════╝██╔══██╗██║╚══██╔══╝██╔═══██╗██╔══██╗" -ForegroundColor Cyan
    Write-Host "  ██╔████╔██║██║  ██║█████╗  ██║  ██║██║   ██║   ██║   ██║██████╔╝" -ForegroundColor Cyan
    Write-Host "  ██║╚██╔╝██║██║  ██║██╔══╝  ██║  ██║██║   ██║   ██║   ██║██╔══██╗" -ForegroundColor Cyan
    Write-Host "  ██║ ╚═╝ ██║██████╔╝███████╗██████╔╝██║   ██║   ╚██████╔╝██║  ██║" -ForegroundColor Cyan
    Write-Host "  ╚═╝     ╚═╝╚═════╝ ╚══════╝╚═════╝ ╚═╝   ╚═╝    ╚═════╝ ╚═╝  ╚═╝" -ForegroundColor Cyan
    Write-Host "  ------------------------------------------------------------------" -ForegroundColor DarkCyan
    Write-Host "   $AppName 프로그램 제거 및 시스템 복원 마법사" -ForegroundColor White
    Write-Host "  ------------------------------------------------------------------" -ForegroundColor DarkCyan
    Write-Host ""
}

$scriptDir = $PSScriptRoot
if (-not $scriptDir) { $scriptDir = (Get-Location).Path }
$installDir = Join-Path $env:LOCALAPPDATA "Programs\mdeditor"

# 1. 실행 중인 프로세스 검사 및 안전 종료
Write-Step "1/5" "실행 중인 MDEditor 프로세스 확인"
$running = Get-Process -Name mdeditor -ErrorAction SilentlyContinue
if ($running) {
    Write-WarningMsg "실행 중인 MDEditor 프로세스를 종료합니다..."
    $running | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 600
}
Write-Success "프로세스 정리 완료"

# 2. Windows 레지스트리 및 파일 연결 정보 복원
Write-Step "2/5" "Windows 레지스트리 및 파일 연결 정리"

$regKeys = @(
    "HKCU:\Software\Classes\.md",
    "HKCU:\Software\Classes\.markdown",
    "HKCU:\Software\Classes\mdeditor.md",
    "HKCU:\Software\Classes\Applications\mdeditor.exe",
    "HKCU:\Software\Classes\SystemFileAssociations\.md\shell\mdeditor",
    "HKCU:\Software\Classes\SystemFileAssociations\.markdown\shell\mdeditor",
    "HKCU:\Software\Classes\Directory\shell\MDEditor",
    "HKCU:\Software\Classes\Directory\Background\shell\MDEditor",
    "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\MDEditor"
)

foreach ($key in $regKeys) {
    if (Test-Path $key) {
        Remove-Item -Path $key -Recurse -Force -ErrorAction SilentlyContinue
    }
}

foreach ($ext in @(".md", ".markdown")) {
    $fileExtPath = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\$ext"
    Remove-ItemProperty -Path "$fileExtPath\OpenWithProgids" -Name "mdeditor.md" -ErrorAction SilentlyContinue
    Remove-Item -Path "$fileExtPath\UserChoice" -Force -ErrorAction SilentlyContinue
}
Write-Success "파일 연결, 컨텍스트 메뉴, 제어판 등록 정보 제거 완료"

# 3. 바로가기 삭제
Write-Step "3/5" "바로가기(시작 메뉴, 바탕화면) 제거"
$startMenuLnk = [System.IO.Path]::Combine($env:APPDATA, 'Microsoft\Windows\Start Menu\Programs\MDEditor.lnk')
$desktopLnk = [System.IO.Path]::Combine([System.Environment]::GetFolderPath('Desktop'), 'MDEditor.lnk')

if (Test-Path $startMenuLnk) { Remove-Item $startMenuLnk -Force -ErrorAction SilentlyContinue }
if (Test-Path $desktopLnk) { Remove-Item $desktopLnk -Force -ErrorAction SilentlyContinue }
Write-Success "바로가기 제거 완료"

# 4. 환경 변수(PATH) 정리
Write-Step "4/5" "사용자 환경 변수(PATH) 정리"
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($userPath) {
    $parts = $userPath -split ';' | Where-Object {
        $_ -and $_.TrimEnd('\') -ne $installDir.TrimEnd('\')
    }
    [Environment]::SetEnvironmentVariable('Path', ($parts -join ';'), 'User')
    Write-Success "사용자 PATH에서 제거 완료"
} else {
    Write-Success "PATH 정리 불필요"
}

# 5. 설치 디렉토리 및 바이너리 삭제
Write-Step "5/5" "설치 파일 및 디렉토리 정리"
if (Test-Path $installDir) {
    Remove-Item -Path "$installDir\mdeditor.exe" -Force -ErrorAction SilentlyContinue
    Remove-Item -Path "$installDir\app.ico" -Force -ErrorAction SilentlyContinue
    Remove-Item -Path "$installDir\install.bat" -Force -ErrorAction SilentlyContinue
    Remove-Item -Path "$installDir\install.ps1" -Force -ErrorAction SilentlyContinue

    $runningFromInstallDir = ($scriptDir.TrimEnd('\') -ieq $installDir.TrimEnd('\'))
    if ($runningFromInstallDir) {
        # 현재 실행 중인 uninstaller 스크립트 프로세스가 종료된 직후 백그라운드에서 폴더 삭제
        Start-Process -FilePath "cmd.exe" -ArgumentList "/c ping 127.0.0.1 -n 2 >nul & rmdir /s /q `"$installDir`"" -WindowStyle Hidden
        Write-Success "설치 디렉토리 지연 삭제 예약 완료"
    } else {
        Remove-Item -Path $installDir -Recurse -Force -ErrorAction SilentlyContinue
        Write-Success "설치 디렉토리 삭제 완료"
    }
    # 앱 설정(마지막 폴더·보기·창 크기). 문서 파일은 건드리지 않는다.
    Remove-Item -Path (Join-Path $env:APPDATA "mdeditor") -Recurse -Force -ErrorAction SilentlyContinue
}

# 셸 캐시 갱신
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class WinShellUninstall {
    [DllImport("shell32.dll")]
    public static extern void SHChangeNotify(int wEventId, int uFlags, IntPtr dwItem1, IntPtr dwItem2);
}
"@ -ErrorAction SilentlyContinue
[WinShellUninstall]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)

if (-not $Quiet) {
    Write-Host ""
    Write-Host "  ==================================================================" -ForegroundColor Green
    Write-Host "   ✨ MDEditor가 시스템에서 완전히 제거되었습니다." -ForegroundColor Green
    Write-Host "  ==================================================================" -ForegroundColor Green
    Write-Host ""
}
