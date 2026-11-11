// Windows 리소스: 아이콘 + 버전 정보(탐색기 속성 > 자세히, 작업 관리자에 표시되는 이름).
// 버전은 Cargo.toml 한 곳에서만 관리한다.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=app.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let version = std::env::var("CARGO_PKG_VERSION").unwrap();
    let commas = version.replace('.', ",");
    let ico = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("app.ico")
        .to_string_lossy()
        .replace('\\', "/");
    let rc = format!(
        r#"1 ICON "{ico}"
1 VERSIONINFO
FILEVERSION {commas},0
PRODUCTVERSION {commas},0
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "ProductName", "MDEditor"
      VALUE "FileDescription", "MDEditor"
      VALUE "CompanyName", "MDEditor Team"
      VALUE "LegalCopyright", "Copyright (c) 2026 MDEditor Team"
      VALUE "OriginalFilename", "mdeditor.exe"
      VALUE "FileVersion", "{version}"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0409, 1200
  END
END
"#
    );
    let rc_path = out_dir.join("app.rc");
    let res_path = out_dir.join("app.res");
    std::fs::write(&rc_path, rc).unwrap();

    // GNU / LLVM windres, 없으면 MSVC rc.exe
    let windres = ["windres", "llvm-windres"].iter().any(|cmd| {
        std::process::Command::new(cmd)
            .arg("-i")
            .arg(&rc_path)
            .args(["-O", "coff", "-o"])
            .arg(&res_path)
            .status()
            .is_ok_and(|s| s.success())
    });
    let compiled = windres
        || std::process::Command::new("rc.exe")
            .arg(format!("/fo{}", res_path.display()))
            .arg(&rc_path)
            .status()
            .is_ok_and(|s| s.success());
    if compiled {
        println!("cargo:rustc-link-arg-bins={}", res_path.display());
    } else {
        println!("cargo:warning=리소스 컴파일러(windres/rc.exe)를 찾지 못해 아이콘·버전 정보 없이 빌드합니다");
    }
}
