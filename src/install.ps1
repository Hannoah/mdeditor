# MDEditor 설치 엔진. 화면은 mdeditor.exe(src/setup.rs)가 그리고, 이 스크립트는 숨김 창에서 실행된다.
# 진행 상황은 표준 출력의 "@@<단계>" 줄로, 실패는 "@@ERR <메시지>" 줄로 알린다.
param(
    [Parameter(Mandatory)][string]$SourceExe,
    [Parameter(Mandatory)][string]$InstallDir,
    [Parameter(Mandatory)][string]$Version,
    [int]$KeepPid = 0,
    [switch]$NoDesktop,
    [switch]$NoAssoc,
    [switch]$NoMenu,
    [switch]$NoPath
)

try { [Console]::OutputEncoding = [System.Text.Encoding]::UTF8 } catch {}
function Step([int]$n) { Write-Output "@@$n" }
function Fail([string]$msg) { Write-Output "@@ERR $msg"; exit 1 }

$targetExe = Join-Path $InstallDir "mdeditor.exe"
$icon = "`"$targetExe`",0"
$open = "`"$targetExe`" `"%1`""
$classes = "HKCU:\Software\Classes"
$exts = @(".md", ".markdown")

# 1. 설치된 MDEditor가 실행 중이면 먼저 정상 종료를 요청한다 (앱이 닫히면서 문서를 저장한다)
#    덮어쓸 설치 폴더의 실행 파일만 닫는다 (개발 빌드·포터블 사본은 건드리지 않는다).
#    "저장되지 않은 변경사항" 창이 떠서 안 닫히면 강제 종료하지 않고 멈춘다 (편집 내용 보호).
Step 1
$procs = @(Get-Process -Name mdeditor -ErrorAction SilentlyContinue |
    Where-Object { $_.Id -ne $KeepPid -and $_.Path -eq $targetExe })
foreach ($p in $procs) { [void]$p.CloseMainWindow() }
foreach ($p in $procs) {
    if (-not $p.WaitForExit(8000)) {
        Fail "실행 중인 MDEditor가 닫히지 않았습니다. 저장하지 않은 문서를 확인하고 MDEditor를 닫은 뒤 다시 설치하세요."
    }
}

# 2. 프로그램 파일
Step 2
try {
    New-Item -Path $InstallDir -ItemType Directory -Force | Out-Null
    if ([System.IO.Path]::GetFullPath($SourceExe) -ne [System.IO.Path]::GetFullPath($targetExe)) {
        for ($attempt = 1; $attempt -le 10; $attempt++) {
            try {
                Copy-Item -LiteralPath $SourceExe -Destination $targetExe -Force -ErrorAction Stop
                break
            } catch {
                if ($attempt -eq 10) { throw }
                Start-Sleep -Milliseconds 150
            }
        }
    }
    # 이전 버전 설치 프로그램이 남긴 파일
    foreach ($old in @("app.ico", "install.bat", "install.ps1", "uninstall.bat", "uninstall.ps1")) {
        Remove-Item -Path (Join-Path $InstallDir $old) -Force -ErrorAction SilentlyContinue
    }
} catch {
    Fail "프로그램 파일을 복사하지 못했습니다: $($_.Exception.Message)"
}

# 3. 파일 형식과 탐색기 메뉴
Step 3
$progId = "$classes\mdeditor.md"
New-Item -Path "$progId\DefaultIcon" -Force | Out-Null
New-Item -Path "$progId\shell\open\command" -Force | Out-Null
Set-ItemProperty -Path $progId -Name "(Default)" -Value "Markdown 문서"
Set-ItemProperty -Path "$progId\DefaultIcon" -Name "(Default)" -Value $icon
Set-ItemProperty -Path "$progId\shell\open\command" -Name "(Default)" -Value $open

# "연결 프로그램" 목록에는 항상 올린다 (기본 앱을 바꾸지는 않는다)
$app = "$classes\Applications\mdeditor.exe"
New-Item -Path "$app\shell\open\command" -Force | Out-Null
New-Item -Path "$app\SupportedTypes" -Force | Out-Null
Set-ItemProperty -Path $app -Name "FriendlyAppName" -Value "MDEditor"
Set-ItemProperty -Path "$app\shell\open\command" -Name "(Default)" -Value $open
foreach ($ext in $exts) {
    Set-ItemProperty -Path "$app\SupportedTypes" -Name $ext -Value ""
    $fileExts = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\$ext"
    New-Item -Path "$fileExts\OpenWithProgids" -Force | Out-Null
    New-ItemProperty -Path "$fileExts\OpenWithProgids" -Name "mdeditor.md" -PropertyType None -Force -ErrorAction SilentlyContinue | Out-Null
}

if (-not $NoAssoc) {
    foreach ($ext in $exts) {
        New-Item -Path "$classes\$ext" -Force | Out-Null
        Set-ItemProperty -Path "$classes\$ext" -Name "(Default)" -Value "mdeditor.md"
        Set-ItemProperty -Path "$classes\$ext" -Name "Content Type" -Value "text/markdown"
        Set-ItemProperty -Path "$classes\$ext" -Name "PerceivedType" -Value "document"
        Remove-Item -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\$ext\UserChoice" -Force -ErrorAction SilentlyContinue
    }
}

$menus = @(
    "$classes\SystemFileAssociations\.md\shell\mdeditor",
    "$classes\SystemFileAssociations\.markdown\shell\mdeditor",
    "$classes\Directory\shell\MDEditor",
    "$classes\Directory\Background\shell\MDEditor"
)
if ($NoMenu) {
    foreach ($m in $menus) { Remove-Item -Path $m -Recurse -Force -ErrorAction SilentlyContinue }
} else {
    foreach ($m in $menus) {
        $isDir = $m -like "*\Directory\*"
        New-Item -Path "$m\command" -Force | Out-Null
        Set-ItemProperty -Path $m -Name "(Default)" -Value $(if ($isDir) { "MDEditor로 이 폴더 열기" } else { "MDEditor로 편집" })
        Set-ItemProperty -Path $m -Name "Icon" -Value $icon
        Set-ItemProperty -Path "$m\command" -Name "(Default)" -Value $(if ($isDir) { "`"$targetExe`" `"%V`"" } else { $open })
    }
}

# 4. 바로가기와 PATH
Step 4
$wsh = New-Object -ComObject WScript.Shell
$links = @([System.IO.Path]::Combine($env:APPDATA, "Microsoft\Windows\Start Menu\Programs\MDEditor.lnk"))
$desktopLnk = [System.IO.Path]::Combine([System.Environment]::GetFolderPath("Desktop"), "MDEditor.lnk")
if ($NoDesktop) {
    Remove-Item -Path $desktopLnk -Force -ErrorAction SilentlyContinue
} else {
    $links += $desktopLnk
}
foreach ($path in $links) {
    $lnk = $wsh.CreateShortcut($path)
    $lnk.TargetPath = $targetExe
    $lnk.IconLocation = "$targetExe,0"
    $lnk.WorkingDirectory = [System.Environment]::GetFolderPath("UserProfile")
    $lnk.Description = "MDEditor - Markdown 편집기"
    $lnk.Save()
}

# PATH는 바뀔 때만 쓴다 (환경 변수 변경은 모든 창에 알림을 보내느라 몇 초씩 걸린다)
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
$parts = @($userPath -split ";" | Where-Object { $_ })
$others = @($parts | Where-Object { $_.TrimEnd("\") -ne $InstallDir.TrimEnd("\") })
$listed = $others.Count -ne $parts.Count
if ($NoPath -and $listed) {
    [Environment]::SetEnvironmentVariable("Path", ($others -join ";"), "User")
} elseif (-not $NoPath -and -not $listed) {
    [Environment]::SetEnvironmentVariable("Path", (($parts + $InstallDir) -join ";"), "User")
}

# 5. Windows 설정 > 앱 > 설치된 앱
Step 5
$uninstall = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\MDEditor"
New-Item -Path $uninstall -Force | Out-Null
Set-ItemProperty -Path $uninstall -Name "DisplayName" -Value "MDEditor"
Set-ItemProperty -Path $uninstall -Name "DisplayVersion" -Value $Version
Set-ItemProperty -Path $uninstall -Name "Publisher" -Value "MDEditor Team"
Set-ItemProperty -Path $uninstall -Name "DisplayIcon" -Value $icon
Set-ItemProperty -Path $uninstall -Name "InstallLocation" -Value $InstallDir
Set-ItemProperty -Path $uninstall -Name "UninstallString" -Value "`"$targetExe`" --uninstall"
Set-ItemProperty -Path $uninstall -Name "QuietUninstallString" -Value "`"$targetExe`" --uninstall --quiet"
Set-ItemProperty -Path $uninstall -Name "EstimatedSize" -Value ([int]((Get-Item -LiteralPath $targetExe).Length / 1KB)) -Type DWord
Set-ItemProperty -Path $uninstall -Name "NoModify" -Value 1 -Type DWord
Set-ItemProperty -Path $uninstall -Name "NoRepair" -Value 1 -Type DWord

# 6. 탐색기에 아이콘·연결 변경 알림 (SHChangeNotify는 setup.rs가 직접 호출)
Step 6

if (-not (Test-Path -LiteralPath $targetExe) -or -not (Test-Path -LiteralPath $uninstall)) { Fail "설치 결과를 확인하지 못했습니다." }
exit 0
