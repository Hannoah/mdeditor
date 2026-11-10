fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let out_dir = std::env::var("OUT_DIR").unwrap();
        let res_path = std::path::Path::new(&out_dir).join("setup.res");

        let mut compiled = false;
        // GNU / LLVM windres
        for cmd in ["windres", "llvm-windres"] {
            let status = std::process::Command::new(cmd)
                .args(["-i", "../app.rc", "-O", "coff", "-o"])
                .arg(&res_path)
                .status();
            if let Ok(s) = status {
                if s.success() {
                    println!("cargo:rustc-link-arg={}", res_path.display());
                    compiled = true;
                    break;
                }
            }
        }

        // MSVC rc.exe fallback
        if !compiled {
            let status = std::process::Command::new("rc.exe")
                .arg(format!("/fo{}", res_path.display()))
                .arg("../app.rc")
                .status();
            if let Ok(s) = status {
                if s.success() {
                    println!("cargo:rustc-link-arg={}", res_path.display());
                }
            }
        }

        println!("cargo:rerun-if-changed=../app.rc");
        println!("cargo:rerun-if-changed=../app.ico");
    }
}
