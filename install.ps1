# MDEditor Windows 설치 및 .md 파일 기본 연결 스크립트
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$Host.UI.RawUI.WindowTitle = "MDEditor 설치 마법사"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "   MDEditor Windows 설치 및 .md 파일 기본 연결 마법사" -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
Write-Host ""

$scriptDir = $PSScriptRoot
if (-not $scriptDir) { $scriptDir = (Get-Location).Path }
$releaseExe = Join-Path $scriptDir "target\release\mdeditor.exe"

# 1. 릴리즈 바이너리 확인 및 필요 시 자동 빌드
Write-Host "[1/5] 릴리즈 바이너리 확인 중..." -ForegroundColor Yellow
if (-not (Test-Path $releaseExe)) {
    Write-Host "  -> 바이너리가 없어 빌드를 시작합니다 (cargo build --release)..." -ForegroundColor Gray
    Push-Location $scriptDir
    cargo build --release
    Pop-Location
    if (-not (Test-Path $releaseExe)) {
        Write-Host "[오류] 빌드에 실패했습니다. Rust(https://rustup.rs) 설치 여부를 확인하세요." -ForegroundColor Red
        return
    }
}
Write-Host "  -> 최신 바이너리 확인 완료: $releaseExe" -ForegroundColor Green

# 2. 설치 디렉토리 구성 (%LOCALAPPDATA%\Programs\mdeditor)
$installDir = Join-Path $env:LOCALAPPDATA "Programs\mdeditor"
$targetExe = Join-Path $installDir "mdeditor.exe"
Write-Host "[2/5] 프로그램 설치 경로: $installDir" -ForegroundColor Yellow

if (-not (Test-Path $installDir)) {
    New-Item -Path $installDir -ItemType Directory -Force | Out-Null
}

try {
    Copy-Item -Path $releaseExe -Destination $targetExe -Force
    Write-Host "  -> mdeditor.exe 복사 완료" -ForegroundColor Green
    $appIco = Join-Path $scriptDir "app.ico"
    if (Test-Path $appIco) {
        Copy-Item -Path $appIco -Destination (Join-Path $installDir "app.ico") -Force
        Write-Host "  -> app.ico 아이콘 복사 완료" -ForegroundColor Green
    }
} catch {
    Write-Host "[오류] 파일 복사에 실패했습니다. 실행 중인 mdeditor가 있다면 종료 후 다시 시도하세요." -ForegroundColor Red
    Write-Host $_.Exception.Message -ForegroundColor Red
    return
}

# 3. uninstall 스크립트 복사
$uninstallBat = Join-Path $scriptDir "uninstall.bat"
$uninstallPs1 = Join-Path $scriptDir "uninstall.ps1"
if (Test-Path $uninstallBat) { Copy-Item $uninstallBat (Join-Path $installDir "uninstall.bat") -Force }
if (Test-Path $uninstallPs1) { Copy-Item $uninstallPs1 (Join-Path $installDir "uninstall.ps1") -Force }

# 4. Windows 레지스트리 등록 (.md, .markdown 기본 앱 및 우클릭 메뉴)
Write-Host "[3/5] Windows 레지스트리 등록 (.md 파일 기본 연결)..." -ForegroundColor Yellow

# .md, .markdown 확장자
foreach ($ext in @(".md", ".markdown")) {
    $extPath = "HKCU:\Software\Classes\$ext"
    New-Item -Path $extPath -Force | Out-Null
    Set-ItemProperty -Path $extPath -Name "(Default)" -Value "mdeditor.md"
    Set-ItemProperty -Path $extPath -Name "Content Type" -Value "text/markdown"
    Set-ItemProperty -Path $extPath -Name "PerceivedType" -Value "document"
}

# mdeditor.md ProgID 정의
$progIdPath = "HKCU:\Software\Classes\mdeditor.md"
New-Item -Path $progIdPath -Force | Out-Null
Set-ItemProperty -Path $progIdPath -Name "(Default)" -Value "Markdown Document"

New-Item -Path "$progIdPath\DefaultIcon" -Force | Out-Null
Set-ItemProperty -Path "$progIdPath\DefaultIcon" -Name "(Default)" -Value "`"$targetExe`",0"

New-Item -Path "$progIdPath\shell\open\command" -Force | Out-Null
Set-ItemProperty -Path "$progIdPath\shell\open" -Name "Icon" -Value "`"$targetExe`",0"
Set-ItemProperty -Path "$progIdPath\shell\open\command" -Name "(Default)" -Value "`"$targetExe`" `"%1`""

New-Item -Path "$progIdPath\shell\edit\command" -Force | Out-Null
Set-ItemProperty -Path "$progIdPath\shell\edit" -Name "(Default)" -Value "MDEditor로 편집"
Set-ItemProperty -Path "$progIdPath\shell\edit" -Name "Icon" -Value "`"$targetExe`",0"
Set-ItemProperty -Path "$progIdPath\shell\edit\command" -Name "(Default)" -Value "`"$targetExe`" `"%1`""

# Applications 등록 (Windows 10/11 연결 프로그램 목록)
$appPath = "HKCU:\Software\Classes\Applications\mdeditor.exe"
New-Item -Path "$appPath\shell\open\command" -Force | Out-Null
Set-ItemProperty -Path $appPath -Name "FriendlyAppName" -Value "MDEditor"
New-Item -Path "$appPath\SupportedTypes" -Force | Out-Null
Set-ItemProperty -Path "$appPath\SupportedTypes" -Name ".md" -Value ""
Set-ItemProperty -Path "$appPath\SupportedTypes" -Name ".markdown" -Value ""
Set-ItemProperty -Path "$appPath\shell\open\command" -Name "(Default)" -Value "`"$targetExe`" `"%1`""

# 탐색기 우클릭 컨텍스트 메뉴
foreach ($ext in @(".md", ".markdown")) {
    $menuPath = "HKCU:\Software\Classes\SystemFileAssociations\$ext\shell\mdeditor"
    $assocPath = "$menuPath\command"
    New-Item -Path $assocPath -Force | Out-Null
    Set-ItemProperty -Path $menuPath -Name "(Default)" -Value "MDEditor로 편집"
    Set-ItemProperty -Path $menuPath -Name "Icon" -Value "`"$targetExe`",0"
    Set-ItemProperty -Path $assocPath -Name "(Default)" -Value "`"$targetExe`" `"%1`""
}

# FileExts 캐시 정리 및 mdeditor 등록
foreach ($ext in @(".md", ".markdown")) {
    $fileExtPath = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\$ext"
    Remove-Item -Path "$fileExtPath\UserChoice" -Force -ErrorAction SilentlyContinue
    New-Item -Path "$fileExtPath\OpenWithProgids" -Force | Out-Null
    New-ItemProperty -Path "$fileExtPath\OpenWithProgids" -Name "mdeditor.md" -PropertyType None -Force -ErrorAction SilentlyContinue | Out-Null
}

Write-Host "  -> 레지스트리 등록 완료" -ForegroundColor Green

# 5. 바로가기 생성 및 환경 변수(PATH) 등록
Write-Host "[4/5] 바로가기 및 환경 변수 등록 중..." -ForegroundColor Yellow
$wsh = New-Object -ComObject WScript.Shell

# 시작 메뉴 바로가기
$startMenuDir = [System.IO.Path]::Combine($env:APPDATA, 'Microsoft\Windows\Start Menu\Programs')
$startLnk = $wsh.CreateShortcut([System.IO.Path]::Combine($startMenuDir, 'mdeditor.lnk'))
$startLnk.TargetPath = $targetExe
$startLnk.IconLocation = "$targetExe,0"
$startLnk.WorkingDirectory = [System.Environment]::GetFolderPath('UserProfile')
$startLnk.Description = 'MDEditor - 초경량 Markdown 편집기'
$startLnk.Save()

# 바탕화면 바로가기
$desktopDir = [System.Environment]::GetFolderPath('Desktop')
$desktopLnk = $wsh.CreateShortcut([System.IO.Path]::Combine($desktopDir, 'mdeditor.lnk'))
$desktopLnk.TargetPath = $targetExe
$desktopLnk.IconLocation = "$targetExe,0"
$desktopLnk.WorkingDirectory = [System.Environment]::GetFolderPath('UserProfile')
$desktopLnk.Description = 'MDEditor - 초경량 Markdown 편집기'
$desktopLnk.Save()

# 사용자 PATH 등록
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($userPath -notlike "*$installDir*") {
    $newPath = if ($userPath) { "$($userPath.TrimEnd(';'));$installDir" } else { $installDir }
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    Write-Host "  -> 사용자 PATH에 추가 완료" -ForegroundColor Green
}

# 셸 변경 알림 (아이콘 및 연결 즉시 갱신)
Write-Host "[5/5] Windows 셸 캐시 갱신 중..." -ForegroundColor Yellow
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class WinShell {
    [DllImport("shell32.dll")]
    public static extern void SHChangeNotify(int wEventId, int uFlags, IntPtr dwItem1, IntPtr dwItem2);
}
"@ -ErrorAction SilentlyContinue
[WinShell]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)

Write-Host ""
Write-Host "============================================================" -ForegroundColor Green
Write-Host "   MDEditor 설치가 완료되었습니다!" -ForegroundColor Green
Write-Host "   - 이제 어떤 .md 파일을 더블클릭해도 MDEditor로 열립니다." -ForegroundColor White
Write-Host "   - 시작 메뉴 및 바탕화면에 바로가기가 추가되었습니다." -ForegroundColor White
Write-Host "   - 터미널 어디서나 'mdeditor [파일명]' 명령어로 실행할 수 있습니다." -ForegroundColor White
Write-Host "   - 설치 제거는 언제든지 uninstall.bat을 실행하시면 됩니다." -ForegroundColor Gray
Write-Host "============================================================" -ForegroundColor Green
Write-Host ""
