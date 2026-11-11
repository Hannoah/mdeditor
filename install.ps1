# MDEditor Production Windows Installer
# Copyright (c) 2026 MDEditor Team. All rights reserved.

[CmdletBinding()]
param(
    [switch]$Quiet,
    [switch]$NoDesktopShortcut,
    [switch]$RunAfterInstall,
    [string]$SourceExe   # 단일 설치 파일이 이미 풀어 둔 exe 경로를 넘긴다
)

[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
if (-not $Quiet) {
    $Host.UI.RawUI.WindowTitle = "MDEditor 설치 마법사"
}

$AppName = "MDEditor"
$AppVersion = "0.2.0"
$AppPublisher = "MDEditor Team"
$AppDescription = "Rust 네이티브 Markdown 편집기 & 미리보기"

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

function Write-ErrorMsg {
    param([string]$Message)
    Write-Host " [오류] " -NoNewline -ForegroundColor Red
    Write-Host $Message -ForegroundColor Red
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
    Write-Host "   $AppName v$AppVersion Production Setup" -ForegroundColor White
    Write-Host "   $AppDescription" -ForegroundColor Gray
    Write-Host "  ------------------------------------------------------------------" -ForegroundColor DarkCyan
    Write-Host ""
}

$scriptDir = $PSScriptRoot
if (-not $scriptDir) { $scriptDir = (Get-Location).Path }
$installDir = Join-Path $env:LOCALAPPDATA "Programs\mdeditor"
$targetExe = Join-Path $installDir "mdeditor.exe"
$targetIco = Join-Path $installDir "app.ico"

# 1. 실행 중인 프로세스 검사 및 안전 종료
Write-Step "1/6" "실행 중인 MDEditor 프로세스 확인"
$running = Get-Process -Name mdeditor -ErrorAction SilentlyContinue
if ($running) {
    Write-WarningMsg "실행 중인 프로세스가 감지되었습니다. 업데이트를 위해 안전하게 종료합니다..."
    $running | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 600
}
Write-Success "프로세스 상태 정상"

# 2. 설치할 바이너리 결정: 설치 파일이 넘긴 경로 > 포터블 zip의 mdeditor.exe > 소스에서 릴리즈 빌드
Write-Step "2/6" "릴리즈 바이너리 준비"
$portableExe = Join-Path $scriptDir "mdeditor.exe"
if ($SourceExe) {
    $releaseExe = $SourceExe
} elseif (Test-Path $portableExe) {
    $releaseExe = $portableExe
} else {
    # 오래된 바이너리가 설치되지 않도록 항상 빌드 (변경이 없으면 즉시 끝난다)
    $releaseExe = Join-Path $scriptDir "target\release\mdeditor.exe"
    Push-Location $scriptDir
    cargo build --release
    $buildOk = $LASTEXITCODE -eq 0
    Pop-Location
    if (-not $buildOk) {
        Write-ErrorMsg "바이너리 빌드에 실패했습니다. Rust(https://rustup.rs) 설치 상태를 확인하세요."
        exit 1
    }
}
if (-not (Test-Path $releaseExe)) {
    Write-ErrorMsg "설치할 mdeditor.exe를 찾을 수 없습니다: $releaseExe"
    exit 1
}
$binarySizeKb = [Math]::Round((Get-Item $releaseExe).Length / 1KB, 1)
Write-Success "바이너리 준비 완료 ($releaseExe, $binarySizeKb KB)"

# 3. 설치 디렉토리 구성 및 파일 복사
Write-Step "3/6" "프로그램 파일 설치 ($installDir)"
if (-not (Test-Path $installDir)) {
    New-Item -Path $installDir -ItemType Directory -Force | Out-Null
}

try {
    if ((Resolve-Path $releaseExe).Path -ne $targetExe) {
        Copy-Item -Path $releaseExe -Destination $targetExe -Force
    }
    Write-Success "mdeditor.exe 복사 완료"

    $sourceIco = Join-Path $scriptDir "app.ico"
    if (Test-Path $sourceIco) {
        Copy-Item -Path $sourceIco -Destination $targetIco -Force
        Write-Success "app.ico 아이콘 복사 완료"
    }

    $uninstallBat = Join-Path $scriptDir "uninstall.bat"
    $uninstallPs1 = Join-Path $scriptDir "uninstall.ps1"
    if (Test-Path $uninstallBat) { Copy-Item $uninstallBat (Join-Path $installDir "uninstall.bat") -Force }
    if (Test-Path $uninstallPs1) { Copy-Item $uninstallPs1 (Join-Path $installDir "uninstall.ps1") -Force }
    Write-Success "언인스톨러 스크립트 등록 완료"
} catch {
    Write-ErrorMsg "파일 복사 중 오류가 발생했습니다: $($_.Exception.Message)"
    exit 1
}

# 4. Windows 레지스트리 등록 (파일 확장자, 탐색기 메뉴, 제어판/설정 앱)
Write-Step "4/6" "Windows 시스템 통합 및 레지스트리 등록"

# 4-1. .md, .markdown 확장자 연결
foreach ($ext in @(".md", ".markdown")) {
    $extPath = "HKCU:\Software\Classes\$ext"
    New-Item -Path $extPath -Force | Out-Null
    Set-ItemProperty -Path $extPath -Name "(Default)" -Value "mdeditor.md"
    Set-ItemProperty -Path $extPath -Name "Content Type" -Value "text/markdown"
    Set-ItemProperty -Path $extPath -Name "PerceivedType" -Value "document"
}

# 4-2. mdeditor.md ProgID 정의
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

# 4-3. Applications 등록 (연결 프로그램 목록)
$appPath = "HKCU:\Software\Classes\Applications\mdeditor.exe"
New-Item -Path "$appPath\shell\open\command" -Force | Out-Null
Set-ItemProperty -Path $appPath -Name "FriendlyAppName" -Value "MDEditor"
New-Item -Path "$appPath\SupportedTypes" -Force | Out-Null
Set-ItemProperty -Path "$appPath\SupportedTypes" -Name ".md" -Value ""
Set-ItemProperty -Path "$appPath\SupportedTypes" -Name ".markdown" -Value ""
Set-ItemProperty -Path "$appPath\shell\open\command" -Name "(Default)" -Value "`"$targetExe`" `"%1`""

# 4-4. 파일 우클릭 컨텍스트 메뉴 (SystemFileAssociations)
foreach ($ext in @(".md", ".markdown")) {
    $menuPath = "HKCU:\Software\Classes\SystemFileAssociations\$ext\shell\mdeditor"
    $assocPath = "$menuPath\command"
    New-Item -Path $assocPath -Force | Out-Null
    Set-ItemProperty -Path $menuPath -Name "(Default)" -Value "MDEditor로 편집"
    Set-ItemProperty -Path $menuPath -Name "Icon" -Value "`"$targetExe`",0"
    Set-ItemProperty -Path $assocPath -Name "(Default)" -Value "`"$targetExe`" `"%1`""
}

# 4-5. 폴더 및 폴더 배경 우클릭 컨텍스트 메뉴 ("MDEditor로 이 폴더 열기")
$dirShellPath = "HKCU:\Software\Classes\Directory\shell\MDEditor"
New-Item -Path "$dirShellPath\command" -Force | Out-Null
Set-ItemProperty -Path $dirShellPath -Name "(Default)" -Value "MDEditor로 이 폴더 열기"
Set-ItemProperty -Path $dirShellPath -Name "Icon" -Value "`"$targetExe`",0"
Set-ItemProperty -Path "$dirShellPath\command" -Name "(Default)" -Value "`"$targetExe`" `"%V`""

$dirBgShellPath = "HKCU:\Software\Classes\Directory\Background\shell\MDEditor"
New-Item -Path "$dirBgShellPath\command" -Force | Out-Null
Set-ItemProperty -Path $dirBgShellPath -Name "(Default)" -Value "MDEditor로 이 폴더 열기"
Set-ItemProperty -Path $dirBgShellPath -Name "Icon" -Value "`"$targetExe`",0"
Set-ItemProperty -Path "$dirBgShellPath\command" -Name "(Default)" -Value "`"$targetExe`" `"%V`""

# 4-6. Explorer FileExts 캐시 정리
foreach ($ext in @(".md", ".markdown")) {
    $fileExtPath = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\$ext"
    Remove-Item -Path "$fileExtPath\UserChoice" -Force -ErrorAction SilentlyContinue
    New-Item -Path "$fileExtPath\OpenWithProgids" -Force | Out-Null
    New-ItemProperty -Path "$fileExtPath\OpenWithProgids" -Name "mdeditor.md" -PropertyType None -Force -ErrorAction SilentlyContinue | Out-Null
}

# 4-7. Windows 설정 > 앱 > 설치된 앱 (프로그램 추가/제거) 등록
$uninstallRegKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\MDEditor"
New-Item -Path $uninstallRegKey -Force | Out-Null

$estimatedSizeKb = [int]((Get-ChildItem -Path $installDir -Recurse | Measure-Object -Property Length -Sum).Sum / 1KB)

Set-ItemProperty -Path $uninstallRegKey -Name "DisplayName" -Value $AppName
Set-ItemProperty -Path $uninstallRegKey -Name "DisplayVersion" -Value $AppVersion
Set-ItemProperty -Path $uninstallRegKey -Name "Publisher" -Value $AppPublisher
Set-ItemProperty -Path $uninstallRegKey -Name "DisplayIcon" -Value "`"$targetExe`",0"
Set-ItemProperty -Path $uninstallRegKey -Name "InstallLocation" -Value $installDir
Set-ItemProperty -Path $uninstallRegKey -Name "UninstallString" -Value "`"$installDir\uninstall.bat`""
Set-ItemProperty -Path $uninstallRegKey -Name "QuietUninstallString" -Value "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$installDir\uninstall.ps1`" -Quiet"
Set-ItemProperty -Path $uninstallRegKey -Name "EstimatedSize" -Value $estimatedSizeKb -Type DWord
Set-ItemProperty -Path $uninstallRegKey -Name "URLInfoAbout" -Value "https://github.com/kornah/mdeditor"
Set-ItemProperty -Path $uninstallRegKey -Name "HelpLink" -Value "https://github.com/kornah/mdeditor"
Set-ItemProperty -Path $uninstallRegKey -Name "NoModify" -Value 1 -Type DWord
Set-ItemProperty -Path $uninstallRegKey -Name "NoRepair" -Value 1 -Type DWord

Write-Success "파일 연결 및 Windows 설치된 앱 등록 완료"

# 5. 바로가기 및 환경 변수(PATH) 구성
Write-Step "5/6" "바로가기 생성 및 사용자 PATH 등록"
$wsh = New-Object -ComObject WScript.Shell

# 시작 메뉴 바로가기
$startMenuDir = [System.IO.Path]::Combine($env:APPDATA, 'Microsoft\Windows\Start Menu\Programs')
$startLnk = $wsh.CreateShortcut([System.IO.Path]::Combine($startMenuDir, 'MDEditor.lnk'))
$startLnk.TargetPath = $targetExe
$startLnk.IconLocation = "$targetExe,0"
$startLnk.WorkingDirectory = [System.Environment]::GetFolderPath('UserProfile')
$startLnk.Description = "$AppName - $AppDescription"
$startLnk.Save()
Write-Success "시작 메뉴 바로가기 등록 완료"

# 바탕화면 바로가기
if (-not $NoDesktopShortcut) {
    $desktopDir = [System.Environment]::GetFolderPath('Desktop')
    $desktopLnk = $wsh.CreateShortcut([System.IO.Path]::Combine($desktopDir, 'MDEditor.lnk'))
    $desktopLnk.TargetPath = $targetExe
    $desktopLnk.IconLocation = "$targetExe,0"
    $desktopLnk.WorkingDirectory = [System.Environment]::GetFolderPath('UserProfile')
    $desktopLnk.Description = "$AppName - $AppDescription"
    $desktopLnk.Save()
    Write-Success "바탕화면 바로가기 등록 완료"
}

# 사용자 PATH 등록
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($userPath -notlike "*$installDir*") {
    $newPath = if ($userPath) { "$($userPath.TrimEnd(';'));$installDir" } else { $installDir }
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    Write-Success "사용자 PATH 등록 완료"
} else {
    Write-Success "사용자 PATH 이미 등록됨"
}

# 6. Windows 셸 캐시 갱신
Write-Step "6/6" "Windows 셸 캐시 및 아이콘 갱신"
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class WinShellInstall {
    [DllImport("shell32.dll")]
    public static extern void SHChangeNotify(int wEventId, int uFlags, IntPtr dwItem1, IntPtr dwItem2);
}
"@ -ErrorAction SilentlyContinue
[WinShellInstall]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)
Write-Success "셸 알림 전달 완료"

if (-not $Quiet) {
    Write-Host ""
    Write-Host "  ==================================================================" -ForegroundColor Green
    Write-Host "   ✨ MDEditor v$AppVersion 설치가 성공적으로 완료되었습니다!" -ForegroundColor Green
    Write-Host "  ==================================================================" -ForegroundColor Green
    Write-Host "   • .md 파일을 더블클릭하면 즉시 MDEditor로 열립니다." -ForegroundColor White
    Write-Host "   • 폴더 또는 빈 공간 우클릭 -> 'MDEditor로 이 폴더 열기' 지원" -ForegroundColor White
    Write-Host "   • Windows '설정 > 앱 > 설치된 앱'에서 MDEditor 관리 및 제거 가능" -ForegroundColor White
    Write-Host "   • 터미널 어디서나 'mdeditor [파일/폴더]' 명령어로 즉시 실행" -ForegroundColor White
    Write-Host "  ==================================================================" -ForegroundColor Green
    Write-Host ""
}

if ($RunAfterInstall) {
    Write-Host "  -> MDEditor를 실행합니다..." -ForegroundColor Cyan
    Start-Process -FilePath $targetExe
}
