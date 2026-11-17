#!/bin/bash
# MDEditor macOS 배포 빌드: 테스트 → 릴리즈 빌드 → MDEditor.app + zip + SHA256
# 사용: ./package-mac.sh [--skip-tests]
set -euo pipefail
cd "$(dirname "$0")"

version=$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)
arch=universal # Apple Silicon(arm64) + Intel(x86_64) 한 바이너리
echo "MDEditor v$version 패키징 (macOS $arch)"

if [[ "${1:-}" != "--skip-tests" ]]; then
    echo "[1/3] 테스트"
    cargo test
fi

echo "[2/3] 릴리즈 빌드 (arm64 + x86_64)"
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

echo "[3/3] dist 구성"
dist=dist
app="$dist/MDEditor.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create -output "$app/Contents/MacOS/mdeditor" \
    target/aarch64-apple-darwin/release/mdeditor \
    target/x86_64-apple-darwin/release/mdeditor

# app.ico(256px) → AppIcon.icns
iconset=$(mktemp -d)/AppIcon.iconset
mkdir -p "$iconset"
sips -s format png app.ico --out "$iconset/base.png" >/dev/null
for s in 16 32 128 256; do
    sips -z $s $s "$iconset/base.png" --out "$iconset/icon_${s}x${s}.png" >/dev/null
    sips -z $((s * 2)) $((s * 2)) "$iconset/base.png" --out "$iconset/icon_${s}x${s}@2x.png" >/dev/null
done
rm "$iconset/base.png"
iconutil -c icns "$iconset" -o "$app/Contents/Resources/AppIcon.icns"

cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>MDEditor</string>
    <key>CFBundleDisplayName</key><string>MDEditor</string>
    <key>CFBundleIdentifier</key><string>com.mdeditor.app</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleExecutable</key><string>mdeditor</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>LSMinimumSystemVersion</key><string>10.13</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSHumanReadableCopyright</key><string>Copyright (c) 2026 MDEditor Team</string>
</dict>
</plist>
EOF

# ad-hoc 서명 (개발자 인증서 없음: 다른 Mac에서는 처음 실행 시 우클릭 > 열기 필요)
codesign --force --deep -s - "$app"

zip="MDEditor-v$version-macos-$arch.zip"
rm -f "$dist/$zip"
ditto -c -k --keepParent "$app" "$dist/$zip"
# 설치 파일(.dmg): 열면 MDEditor.app을 Applications 바로가기로 끌어다 놓아 설치한다
dmg="MDEditor-Setup-v$version-macos-$arch.dmg"
stage=$(mktemp -d)
cp -R "$app" "$stage/"
ln -s /Applications "$stage/Applications"
rm -f "$dist/$dmg"
hdiutil create -volname "MDEditor $version" -srcfolder "$stage" -format UDZO -ov "$dist/$dmg" >/dev/null
rm -rf "$stage"

(cd "$dist" && shasum -a 256 "$dmg" "$zip" > "SHA256SUMS-macos.txt")

echo ""
echo "완료: $app"
echo "설치 파일: $dist/$dmg   (열고 MDEditor를 Applications로 끌어다 놓기)"
echo "포터블: $dist/$zip"
