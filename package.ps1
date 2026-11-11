# MDEditor Production Packaging & Distribution Builder
# Copyright (c) 2026 MDEditor Team. All rights reserved.

[CmdletBinding()]
param(
    [switch]$SkipTests
)

[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$Host.UI.RawUI.WindowTitle = "MDEditor 프로덕션 패키징 빌더"

$AppName = "MDEditor"
$AppVersion = "0.3.0"
$scriptDir = $PSScriptRoot
if (-not $scriptDir) { $scriptDir = (Get-Location).Path }
$distDir = Join-Path $scriptDir "dist"

Write-Host ""
Write-Host "  ==================================================================" -ForegroundColor Cyan
Write-Host "   📦 MDEditor v$AppVersion Production Release Packaging" -ForegroundColor White
Write-Host "  ==================================================================" -ForegroundColor Cyan
Write-Host ""

# 1. 단위 테스트 검증
if (-not $SkipTests) {
    Write-Host " [1/5] 단위 테스트 실행 중 (cargo test)..." -ForegroundColor Yellow
    Push-Location $scriptDir
    cargo test
    if ($LASTEXITCODE -ne 0) {
        Write-Host " [오류] 단위 테스트를 통과하지 못했습니다. 패키징을 중단합니다." -ForegroundColor Red
        Pop-Location
        exit 1
    }
    Pop-Location
    Write-Host "       [✓] 모든 단위 테스트 통과 완료" -ForegroundColor Green
} else {
    Write-Host " [1/5] 단위 테스트 생략 (-SkipTests)" -ForegroundColor Gray
}

# 2. 릴리즈 메인 바이너리 빌드
Write-Host " [2/5] 릴리즈 바이너리 빌드 중 (cargo build --release)..." -ForegroundColor Yellow
Push-Location $scriptDir
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Host " [오류] 메인 바이너리 빌드에 실패했습니다." -ForegroundColor Red
    Pop-Location
    exit 1
}
Pop-Location
$releaseExe = Join-Path $scriptDir "target\release\mdeditor.exe"
$releaseSizeKb = [Math]::Round((Get-Item $releaseExe).Length / 1KB, 1)
Write-Host "       [✓] mdeditor.exe 빌드 완료 ($releaseSizeKb KB)" -ForegroundColor Green

# 3. 단일 실행 설치 파일(Standalone Installer) 빌드
Write-Host " [3/5] 단일 파일 설치 프로그램 빌드 (installer/Cargo.toml)..." -ForegroundColor Yellow
$installerManifest = Join-Path $scriptDir "installer\Cargo.toml"
cargo build --release --manifest-path $installerManifest
if ($LASTEXITCODE -ne 0) {
    Write-Host " [오류] 인스톨러 바이너리 빌드에 실패했습니다." -ForegroundColor Red
    exit 1
}
$installerExe = Join-Path $scriptDir "installer\target\release\mdeditor-setup.exe"
$installerSizeKb = [Math]::Round((Get-Item $installerExe).Length / 1KB, 1)
Write-Host "       [✓] mdeditor-setup.exe 빌드 완료 ($installerSizeKb KB)" -ForegroundColor Green

# 4. 배포 디렉토리(dist) 구성 및 산출물 배치
Write-Host " [4/5] 프로덕션 배포 패키지(dist) 생성 중..." -ForegroundColor Yellow
if (-not (Test-Path $distDir)) {
    New-Item -Path $distDir -ItemType Directory -Force | Out-Null
}

# 4-1. 단일 설치 파일 배치
$setupTarget = Join-Path $distDir "MDEditor-Setup.exe"
$setupVersionedTarget = Join-Path $distDir "MDEditor-Setup-v$AppVersion.exe"
Copy-Item -Path $installerExe -Destination $setupTarget -Force
Copy-Item -Path $installerExe -Destination $setupVersionedTarget -Force
Write-Host "       [✓] 단일 설치 파일 생성: dist\MDEditor-Setup.exe" -ForegroundColor Green

# 4-2. 포터블(Portable) zip 패키지 생성
$stagingDir = Join-Path $distDir "staging"
if (Test-Path $stagingDir) { Remove-Item -Path $stagingDir -Recurse -Force }
New-Item -Path $stagingDir -ItemType Directory -Force | Out-Null

Copy-Item -Path $releaseExe -Destination (Join-Path $stagingDir "mdeditor.exe") -Force
Copy-Item -Path (Join-Path $scriptDir "app.ico") -Destination (Join-Path $stagingDir "app.ico") -Force
Copy-Item -Path (Join-Path $scriptDir "install.bat") -Destination (Join-Path $stagingDir "install.bat") -Force
Copy-Item -Path (Join-Path $scriptDir "install.ps1") -Destination (Join-Path $stagingDir "install.ps1") -Force
Copy-Item -Path (Join-Path $scriptDir "uninstall.bat") -Destination (Join-Path $stagingDir "uninstall.bat") -Force
Copy-Item -Path (Join-Path $scriptDir "uninstall.ps1") -Destination (Join-Path $stagingDir "uninstall.ps1") -Force
Copy-Item -Path (Join-Path $scriptDir "README.md") -Destination (Join-Path $stagingDir "README.md") -Force
Copy-Item -Path (Join-Path $scriptDir "LICENSE") -Destination (Join-Path $stagingDir "LICENSE") -Force

$zipTarget = Join-Path $distDir "MDEditor-v$AppVersion-windows-x64.zip"
if (Test-Path $zipTarget) { Remove-Item $zipTarget -Force }
Compress-Archive -Path "$stagingDir\*" -DestinationPath $zipTarget -Force
Remove-Item -Path $stagingDir -Recurse -Force
$zipSizeKb = [Math]::Round((Get-Item $zipTarget).Length / 1KB, 1)
Write-Host "       [✓] 포터블 압축 패키지 생성: dist\MDEditor-v$AppVersion-windows-x64.zip ($zipSizeKb KB)" -ForegroundColor Green

# 5. 무결성 검증용 SHA256 체크섬 파일 생성
Write-Host " [5/5] 무결성 체크섬(SHA256SUMS.txt) 생성 중..." -ForegroundColor Yellow
$checksumFile = Join-Path $distDir "SHA256SUMS.txt"
$hashLines = @()

$artifacts = @(
    "MDEditor-Setup.exe",
    "MDEditor-Setup-v$AppVersion.exe",
    "MDEditor-v$AppVersion-windows-x64.zip"
)

foreach ($artifact in $artifacts) {
    $filePath = Join-Path $distDir $artifact
    if (Test-Path $filePath) {
        $hash = (Get-FileHash $filePath -Algorithm SHA256).Hash.ToLower()
        $hashLines += "$hash  $artifact"
    }
}
$hashLines | Out-File -FilePath $checksumFile -Encoding ascii
Write-Host "       [✓] SHA256SUMS.txt 생성 완료" -ForegroundColor Green

Write-Host ""
Write-Host "  ==================================================================" -ForegroundColor Green
Write-Host "   🎉 프로덕션 배포 패키지 구성이 완료되었습니다!" -ForegroundColor Green
Write-Host "  ==================================================================" -ForegroundColor Green
Write-Host ""
Write-Host "   [배포 파일 목록]" -ForegroundColor White

foreach ($artifact in $artifacts) {
    $filePath = Join-Path $distDir $artifact
    if (Test-Path $filePath) {
        $size = [Math]::Round((Get-Item $filePath).Length / 1KB, 1)
        $hash = (Get-FileHash $filePath -Algorithm SHA256).Hash.Substring(0, 16)
        Write-Host "   • $artifact ($size KB) - SHA256: ${hash}..." -ForegroundColor Cyan
    }
}
Write-Host ""
Write-Host "   💡 배포 시에는 'dist\MDEditor-Setup.exe' 단일 파일 하나만 배포하시면 됩니다." -ForegroundColor Yellow
Write-Host "      (더블클릭 시 독립 실행되어 자동 설치, 바로가기 및 파일 연결이 완료됩니다.)" -ForegroundColor Gray
Write-Host "  ==================================================================" -ForegroundColor Green
Write-Host ""
