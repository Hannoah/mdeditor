# MDEditor 제거 스크립트
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$Host.UI.RawUI.WindowTitle = "MDEditor 제거 마법사"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "   MDEditor 제거 및 파일 연결 초기화" -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
Write-Host ""

$installDir = Join-Path $env:LOCALAPPDATA "Programs\mdeditor"

# 1. 레지스트리 연결 해제
Write-Host "[1/4] 레지스트리 연결 정보 정리 중..." -ForegroundColor Yellow

$regKeys = @(
    "HKCU:\Software\Classes\.md",
    "HKCU:\Software\Classes\.markdown",
    "HKCU:\Software\Classes\mdeditor.md",
    "HKCU:\Software\Classes\Applications\mdeditor.exe",
    "HKCU:\Software\Classes\SystemFileAssociations\.md\shell\mdeditor",
    "HKCU:\Software\Classes\SystemFileAssociations\.markdown\shell\mdeditor"
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
Write-Host "  -> 레지스트리 키 삭제 완료" -ForegroundColor Green

# 2. 바로가기 삭제
Write-Host "[2/4] 바로가기 삭제 중..." -ForegroundColor Yellow
$startMenuLnk = [System.IO.Path]::Combine($env:APPDATA, 'Microsoft\Windows\Start Menu\Programs\mdeditor.lnk')
$desktopLnk = [System.IO.Path]::Combine([System.Environment]::GetFolderPath('Desktop'), 'mdeditor.lnk')

if (Test-Path $startMenuLnk) { Remove-Item $startMenuLnk -Force -ErrorAction SilentlyContinue }
if (Test-Path $desktopLnk) { Remove-Item $desktopLnk -Force -ErrorAction SilentlyContinue }
Write-Host "  -> 바로가기 삭제 완료" -ForegroundColor Green

# 3. 환경 변수 정리
Write-Host "[3/4] 환경 변수(PATH) 정리 중..." -ForegroundColor Yellow
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($userPath) {
    $parts = $userPath -split ';' | Where-Object {
        $_ -and $_.TrimEnd('\') -ne $installDir.TrimEnd('\')
    }
    [Environment]::SetEnvironmentVariable('Path', ($parts -join ';'), 'User')
    Write-Host "  -> 사용자 PATH에서 제거 완료" -ForegroundColor Green
}

# 4. 설치 디렉토리 정리
Write-Host "[4/4] 설치 파일 삭제 중..." -ForegroundColor Yellow
if (Test-Path $installDir) {
    Remove-Item -Path "$installDir\mdeditor.exe" -Force -ErrorAction SilentlyContinue
    Remove-Item -Path "$installDir\app.ico" -Force -ErrorAction SilentlyContinue
    Remove-Item -Path "$installDir\install.bat" -Force -ErrorAction SilentlyContinue
    Remove-Item -Path "$installDir\install.ps1" -Force -ErrorAction SilentlyContinue
    # 현재 실행 중인 uninstall 스크립트 제외 후 디렉토리 정리 시도
    Get-ChildItem -Path $installDir -Exclude "uninstall.*" -ErrorAction SilentlyContinue | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
    Write-Host "  -> 프로그램 바이너리 삭제 완료" -ForegroundColor Green
}

# 셸 캐시 갱신
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class WinShellUn {
    [DllImport("shell32.dll")]
    public static extern void SHChangeNotify(int wEventId, int uFlags, IntPtr dwItem1, IntPtr dwItem2);
}
"@ -ErrorAction SilentlyContinue
[WinShellUn]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)

Write-Host ""
Write-Host "============================================================" -ForegroundColor Green
Write-Host "   MDEditor가 정상적으로 제거되었습니다." -ForegroundColor Green
Write-Host "============================================================" -ForegroundColor Green
Write-Host ""
