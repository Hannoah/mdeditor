#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// MDEditor 단일 파일 설치 프로그램 (std + Win32 MessageBox만 사용)
// mdeditor.exe·아이콘·제거 스크립트를 내장하고, 시스템 등록은 내장 install.ps1에 맡긴다.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const MDEDITOR_EXE: &[u8] = include_bytes!("../../target/release/mdeditor.exe");
const APP_ICO: &[u8] = include_bytes!("../../app.ico");
const UNINSTALL_BAT: &[u8] = include_bytes!("../../uninstall.bat");
const UNINSTALL_PS1: &[u8] = include_bytes!("../../uninstall.ps1");
const INSTALL_PS1: &[u8] = include_bytes!("../../install.ps1");
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(windows)]
mod win32 {
    #[link(name = "user32")]
    extern "system" {
        pub fn MessageBoxW(
            hwnd: *mut (),
            text: *const u16,
            caption: *const u16,
            u_type: u32,
        ) -> i32;
    }

    pub const MB_YESNO: u32 = 0x00000004;
    pub const MB_ICONQUESTION: u32 = 0x00000020;
    pub const MB_ICONINFORMATION: u32 = 0x00000040;
    pub const MB_ICONERROR: u32 = 0x00000010;
    pub const MB_TOPMOST: u32 = 0x00040000;
    pub const IDYES: i32 = 6;

    pub fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn message_box(caption: &str, text: &str, u_type: u32) -> i32 {
        let w_caption = to_wide(caption);
        let w_text = to_wide(text);
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                w_text.as_ptr(),
                w_caption.as_ptr(),
                u_type | MB_TOPMOST,
            )
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut silent = false;
    let mut no_launch = false;

    for arg in &args {
        match arg.to_lowercase().as_str() {
            "/s" | "-s" | "--silent" | "-quiet" | "--quiet" => silent = true,
            "--no-launch" | "-nolaunch" => no_launch = true,
            _ => {}
        }
    }

    #[cfg(windows)]
    if !silent {
        let prompt = format!(
            "MDEditor v{VERSION}을 이 컴퓨터에 설치하시겠습니까?\n\n\
                      • 설치 위치: %LOCALAPPDATA%\\Programs\\mdeditor\n\
                      • .md 및 .markdown 파일 기본 연결\n\
                      • Windows 탐색기 'MDEditor로 이 폴더 열기' 등록\n\
                      • 시작 메뉴 및 바탕화면 바로가기 생성\n\
                      • Windows 설정 '설치된 앱' 등록 (제어판 1-클릭 제거 지원)"
        );
        let res = win32::message_box(
            "MDEditor 설치 마법사",
            &prompt,
            win32::MB_YESNO | win32::MB_ICONQUESTION,
        );
        if res != win32::IDYES {
            return;
        }
    }

    // 설치 디렉토리 확인 (%LOCALAPPDATA%\Programs\mdeditor)
    let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
        let userprofile =
            std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\Users\\Default".into());
        format!("{userprofile}\\AppData\\Local")
    });

    let install_dir = PathBuf::from(local_app_data)
        .join("Programs")
        .join("mdeditor");
    let target_exe = install_dir.join("mdeditor.exe");

    // 1. 실행 중인 mdeditor 프로세스 종료 (파일 잠금 충돌 방지)
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/F", "/IM", "mdeditor.exe"])
            .output();
        std::thread::sleep(std::time::Duration::from_millis(400));
    }

    // 2. 디렉토리 생성
    if let Err(e) = fs::create_dir_all(&install_dir) {
        #[cfg(windows)]
        if !silent {
            win32::message_box(
                "MDEditor 설치 오류",
                &format!("설치 폴더를 생성할 수 없습니다:\n{e}"),
                win32::MB_ICONERROR,
            );
        }
        std::process::exit(1);
    }

    // 3. 내장 파일 복사
    if let Err(e) = fs::write(&target_exe, MDEDITOR_EXE) {
        #[cfg(windows)]
        if !silent {
            win32::message_box(
                "MDEditor 설치 오류",
                &format!("프로그램 바이너리 복사에 실패했습니다:\n{e}"),
                win32::MB_ICONERROR,
            );
        }
        std::process::exit(1);
    }

    let _ = fs::write(install_dir.join("app.ico"), APP_ICO);
    let _ = fs::write(install_dir.join("uninstall.bat"), UNINSTALL_BAT);
    let _ = fs::write(install_dir.join("uninstall.ps1"), UNINSTALL_PS1);

    // 4. 파일 연결·바로가기·설치된 앱 등록 (내장 install.ps1, 방금 복사한 exe를 원본으로 지정)
    let temp_script =
        std::env::temp_dir().join(format!("mdeditor-setup-{}.ps1", std::process::id()));
    let registered = fs::write(&temp_script, INSTALL_PS1).is_ok()
        && Command::new("powershell.exe")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&temp_script)
            .args(["-Quiet", "-SourceExe"])
            .arg(&target_exe)
            .output()
            .is_ok_and(|o| o.status.success());
    let _ = fs::remove_file(&temp_script);
    #[cfg(windows)]
    if !registered && !silent {
        win32::message_box(
            "MDEditor 설치 경고",
            "프로그램은 설치되었지만 파일 연결·바로가기 등록에 실패했습니다.\n\
             설치 폴더의 mdeditor.exe는 직접 실행할 수 있습니다.",
            win32::MB_ICONERROR,
        );
    }

    // 5. 완료 알림 및 실행 안내
    #[cfg(windows)]
    if !silent {
        let success_msg = format!(
            "✨ MDEditor v{VERSION} 설치가 성공적으로 완료되었습니다!\n\n\
                           • .md 파일을 더블클릭하면 MDEditor로 열립니다.\n\
                           • 폴더 우클릭 -> 'MDEditor로 이 폴더 열기'를 지원합니다.\n\
                           • 언제든 Windows 설정의 '설치된 앱'에서 제거할 수 있습니다.\n\n\
                           지금 MDEditor를 실행하시겠습니까?"
        );
        let res = win32::message_box(
            "MDEditor 설치 완료",
            &success_msg,
            win32::MB_YESNO | win32::MB_ICONINFORMATION,
        );
        if res == win32::IDYES {
            let _ = Command::new(&target_exe).spawn();
        }
        return;
    }

    // silent 모드에서 실행 요청이 있을 때
    if !no_launch {
        let _ = Command::new(&target_exe).spawn();
    }
}
