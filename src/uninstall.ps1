# MDEditor 제거 엔진. install.ps1과 같은 규약("@@<단계>", "@@ERR <메시지>")을 쓴다.
# 사용자의 문서 파일은 건드리지 않는다.
param(
    [Parameter(Mandatory)][string]$InstallDir,
    [switch]$RemoveSettings
)

try { [Console]::OutputEncoding = [System.Text.Encoding]::UTF8 } catch {}
function Step([int]$n) { Write-Output "@@$n" }

$targetExe = Join-Path $InstallDir "mdeditor.exe"
$classes = "HKCU:\Software\Classes"

# 1. 실행 중인 MDEditor 닫기 (정상 종료 요청 → 안 닫히면 강제 종료)
Step 1
$procs = @(Get-Process -Name mdeditor -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $targetExe })
foreach ($p in $procs) { [void]$p.CloseMainWindow() }
foreach ($p in $procs) {
    if (-not $p.WaitForExit(4000)) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
}

# 2. 파일 연결·탐색기 메뉴·설치 정보
Step 2
foreach ($ext in @(".md", ".markdown")) {
    # 다른 프로그램이 가져간 연결은 지우지 않는다
    $key = "$classes\$ext"
    if ((Get-ItemProperty -Path $key -ErrorAction SilentlyContinue)."(default)" -eq "mdeditor.md") {
        Remove-Item -Path $key -Recurse -Force -ErrorAction SilentlyContinue
    }
    $fileExts = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\$ext"
    Remove-ItemProperty -Path "$fileExts\OpenWithProgids" -Name "mdeditor.md" -ErrorAction SilentlyContinue
}
foreach ($key in @(
        "$classes\mdeditor.md",
        "$classes\Applications\mdeditor.exe",
        "$classes\SystemFileAssociations\.md\shell\mdeditor",
        "$classes\SystemFileAssociations\.markdown\shell\mdeditor",
        "$classes\Directory\shell\MDEditor",
        "$classes\Directory\Background\shell\MDEditor",
        "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\MDEditor")) {
    Remove-Item -Path $key -Recurse -Force -ErrorAction SilentlyContinue
}

# 3. 바로가기와 PATH
Step 3
Remove-Item -Path ([System.IO.Path]::Combine($env:APPDATA, "Microsoft\Windows\Start Menu\Programs\MDEditor.lnk")) -Force -ErrorAction SilentlyContinue
Remove-Item -Path ([System.IO.Path]::Combine([System.Environment]::GetFolderPath("Desktop"), "MDEditor.lnk")) -Force -ErrorAction SilentlyContinue
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath) {
    $newPath = @($userPath -split ";" | Where-Object { $_ -and $_.TrimEnd("\") -ne $InstallDir.TrimEnd("\") }) -join ";"
    if ($newPath -ne $userPath) { [Environment]::SetEnvironmentVariable("Path", $newPath, "User") }
}

# 4. 프로그램 파일 (+ 선택 시 앱 설정)
Step 4
Remove-Item -Path $InstallDir -Recurse -Force -ErrorAction SilentlyContinue
if (Test-Path $InstallDir) {
    # 아직 잠겨 있으면(제거 창이 설치 폴더에서 실행된 경우 등) 프로세스가 끝난 뒤 지운다
    Start-Process -FilePath "cmd.exe" -ArgumentList "/c ping 127.0.0.1 -n 4 >nul & rmdir /s /q `"$InstallDir`"" -WindowStyle Hidden
}
if ($RemoveSettings) {
    Remove-Item -Path (Join-Path $env:APPDATA "mdeditor") -Recurse -Force -ErrorAction SilentlyContinue
}

Add-Type -Namespace MDEditor -Name Shell -MemberDefinition '[DllImport("shell32.dll")] public static extern void SHChangeNotify(int e, int f, IntPtr a, IntPtr b);' -ErrorAction SilentlyContinue
try { [MDEditor.Shell]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero) } catch {}
exit 0
