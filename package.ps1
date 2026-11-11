# MDEditor 배포 빌드: 테스트 → 릴리즈 빌드 → 설치 파일 + 포터블 zip + SHA256
# 설치 파일은 mdeditor.exe와 같은 바이너리다. 파일 이름에 "Setup"이 들어 있으면 설치 마법사로 뜬다(src/setup.rs).
[CmdletBinding()]
param(
    [switch]$SkipTests
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Set-Location $PSScriptRoot

$version = (Select-String -Path "Cargo.toml" -Pattern '^version\s*=\s*"(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
Write-Host "MDEditor v$version 패키징" -ForegroundColor Cyan

if (-not $SkipTests) {
    Write-Host "[1/3] 테스트" -ForegroundColor Yellow
    cargo test
    if ($LASTEXITCODE -ne 0) { Write-Host "테스트 실패 - 패키징을 중단합니다." -ForegroundColor Red; exit 1 }
}

Write-Host "[2/3] 릴리즈 빌드" -ForegroundColor Yellow
cargo build --release
if ($LASTEXITCODE -ne 0) { Write-Host "빌드 실패" -ForegroundColor Red; exit 1 }
$exe = "target\release\mdeditor.exe"

Write-Host "[3/3] dist 구성" -ForegroundColor Yellow
$dist = "dist"
if (Test-Path $dist) { Remove-Item "$dist\*" -Recurse -Force }
New-Item -Path $dist -ItemType Directory -Force | Out-Null

$setup = "MDEditor-Setup-v$version.exe"
$zip = "MDEditor-v$version-windows-x64-portable.zip"
Copy-Item $exe (Join-Path $dist $setup)
Compress-Archive -Path $exe, "README.md", "LICENSE" -DestinationPath (Join-Path $dist $zip)

$sums = foreach ($name in @($setup, $zip)) {
    $file = Get-Item (Join-Path $dist $name)
    $hash = (Get-FileHash $file.FullName -Algorithm SHA256).Hash.ToLower()
    Write-Host ("  {0}  ({1:N1} MB)" -f $name, ($file.Length / 1MB)) -ForegroundColor Green
    "$hash  $name"
}
$sums | Out-File -FilePath (Join-Path $dist "SHA256SUMS.txt") -Encoding ascii

Write-Host ""
Write-Host "배포: dist\$setup 하나만 전달하면 됩니다 (더블클릭 → 설치 마법사)." -ForegroundColor Cyan
Write-Host "무인 설치: $setup /S      제거: 설정 > 앱 > 설치된 앱 > MDEditor" -ForegroundColor Gray
