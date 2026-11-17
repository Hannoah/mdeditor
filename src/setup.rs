// 설치·제거 마법사. 실행 파일 하나가 앱과 설치 프로그램을 겸한다:
//   MDEditor-Setup.exe (파일 이름에 "setup") 또는 mdeditor --install   → 설치
//   mdeditor --uninstall ("설치된 앱"의 제거 버튼)                      → 제거
//   /S, --silent, --quiet                                              → 창 없이 진행
// 화면만 여기서 그리고, 시스템 등록은 내장 PowerShell 스크립트(install.ps1 / uninstall.ps1)가 한다.
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

use eframe::egui;
use egui::{pos2, vec2, Align, Align2, Color32, FontId, Frame, Margin, RichText, Sense, Stroke};

use crate::theme::{self, img, Pal, GREEN, RED};
use crate::ui::{ghost, primary};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const INSTALL_PS1: &str = include_str!("install.ps1");
const UNINSTALL_PS1: &str = include_str!("uninstall.ps1");
const INSTALL_STEPS: [&str; 6] = [
    "실행 중인 MDEditor를 닫는 중",
    "프로그램 파일 복사 중",
    "파일 형식과 탐색기 메뉴 등록 중",
    "바로가기 만드는 중",
    "설치된 앱 목록에 등록 중",
    "마무리하는 중",
];
const UNINSTALL_STEPS: [&str; 4] = [
    "실행 중인 MDEditor를 닫는 중",
    "파일 연결과 탐색기 메뉴 정리 중",
    "바로가기 정리 중",
    "프로그램 파일 삭제 중",
];

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Install,
    Uninstall,
}

pub fn install_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join("AppData").join("Local"))
        .join("Programs")
        .join("mdeditor")
}

use crate::home;

// 실행 인자·파일 이름으로 설치/제거 모드인지 판별한다. 일반 앱 실행이면 None.
pub fn mode(exe: &Path, args: &[String]) -> Option<Mode> {
    let has = |flag: &str| args.iter().any(|a| a.eq_ignore_ascii_case(flag));
    if has("--uninstall") || has("--uninstall-run") {
        return Some(Mode::Uninstall);
    }
    let named_setup = exe
        .file_stem()
        .is_some_and(|s| s.to_string_lossy().to_lowercase().contains("setup"));
    (has("--install") || named_setup).then_some(Mode::Install)
}

pub fn run(mode: Mode, args: &[String]) -> eframe::Result {
    let has = |flags: &[&str]| {
        args.iter()
            .any(|a| flags.iter().any(|f| a.eq_ignore_ascii_case(f)))
    };
    let quiet = has(&["/s", "-s", "--silent", "--quiet"]);
    let exe = std::env::current_exe().unwrap_or_default();

    // 제거는 임시 폴더의 복사본에서 진행해야 설치 폴더(자기 자신)를 지울 수 있다
    if mode == Mode::Uninstall && !has(&["--uninstall-run"]) && exe.starts_with(install_dir()) {
        let copy = std::env::temp_dir().join("mdeditor-uninstall.exe");
        let mut cmd = Command::new(&copy);
        cmd.arg("--uninstall-run");
        if quiet {
            cmd.arg("--quiet");
        }
        if std::fs::copy(&exe, &copy).is_ok() && cmd.spawn().is_ok() {
            return Ok(());
        }
        // 복사 실패: 제자리에서 진행 (설치 폴더는 uninstall.ps1이 종료 후 지우도록 예약한다)
    }

    let result = if quiet {
        let opts = Options::default();
        if let Err(e) = execute(mode, &opts, |_| {}) {
            eprintln!("{e}");
            std::process::exit(1);
        }
        Ok(())
    } else {
        window(mode)
    };
    if has(&["--uninstall-run"]) {
        // 임시 복사본 뒷정리: 프로세스가 끝난 뒤 자기 자신을 지운다
        let _ = hidden("cmd.exe")
            .raw_arg(format!(
                "/c ping 127.0.0.1 -n 3 >nul & del /q \"{}\"",
                exe.display()
            ))
            .spawn();
    }
    result
}

fn window(mode: Mode) -> eframe::Result {
    let title = match mode {
        Mode::Install => "MDEditor 설치",
        Mode::Uninstall => "MDEditor 제거",
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size([540.0, 480.0])
            .with_resizable(false)
            .with_maximize_button(false)
            .with_icon(crate::app_icon()),
        centered: true,
        persist_window: false,
        ..Default::default()
    };
    // 앱 설정(%APPDATA%\mdeditor)과 섞이지 않게 다른 이름을 쓰고, 끝나면 흔적을 지운다
    let app_id = "mdeditor-setup";
    let result = eframe::run_native(
        app_id,
        options,
        Box::new(move |cc| Ok(Box::new(Setup::new(cc, mode)))),
    );
    if let Some(appdata) = std::env::var_os("APPDATA") {
        let _ = std::fs::remove_dir_all(PathBuf::from(appdata).join(app_id));
    }
    result
}

// 콘솔 창이 깜빡이지 않게 숨겨서 실행한다
fn hidden(program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    cmd
}

#[derive(Clone)]
struct Options {
    desktop: bool,
    assoc: bool,
    menu: bool,
    path: bool,
    remove_settings: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            desktop: true,
            assoc: true,
            menu: true,
            path: true,
            remove_settings: false,
        }
    }
}

fn execute(mode: Mode, o: &Options, on_step: impl FnMut(usize)) -> Result<(), String> {
    let dir = install_dir();
    let dir = dir.to_string_lossy();
    let res = match mode {
        Mode::Install => {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let pid = std::process::id().to_string();
            let mut args = vec![
                "-SourceExe".to_string(),
                exe.to_string_lossy().into_owned(),
                "-InstallDir".into(),
                dir.into_owned(),
                "-Version".into(),
                VERSION.into(),
                "-KeepPid".into(),
                pid,
            ];
            for (on, flag) in [
                (o.desktop, "-NoDesktop"),
                (o.assoc, "-NoAssoc"),
                (o.menu, "-NoMenu"),
                (o.path, "-NoPath"),
            ] {
                if !on {
                    args.push(flag.into());
                }
            }
            run_script(INSTALL_PS1, &args, on_step)
        }
        Mode::Uninstall => {
            let mut args = vec!["-InstallDir".to_string(), dir.into_owned()];
            if o.remove_settings {
                args.push("-RemoveSettings".into());
            }
            run_script(UNINSTALL_PS1, &args, on_step)
        }
    };
    if res.is_ok() {
        notify_shell_assoc_changed();
    }
    res
}

fn notify_shell_assoc_changed() {
    #[link(name = "shell32")]
    extern "system" {
        fn SHChangeNotify(
            event_id: i32,
            flags: u32,
            item1: *const std::ffi::c_void,
            item2: *const std::ffi::c_void,
        );
    }
    // SHCNE_ASSOCCHANGED = 0x0800_0000, SHCNF_IDLIST = 0
    unsafe {
        SHChangeNotify(0x0800_0000, 0, std::ptr::null(), std::ptr::null());
    }
}

// 스크립트를 임시 파일로 풀어 숨김 PowerShell로 실행한다. "@@n" 줄은 진행 단계, "@@ERR ..."는 실패 사유.
fn run_script(script: &str, args: &[String], mut on_step: impl FnMut(usize)) -> Result<(), String> {
    use std::io::{BufRead, BufReader};
    let path = std::env::temp_dir().join(format!("mdeditor-setup-{}.ps1", std::process::id()));
    let clean = script.trim_start_matches('\u{feff}');
    // BOM이 없으면 Windows PowerShell 5.1이 한글을 ANSI로 읽는다 (이중 BOM 방지)
    std::fs::write(&path, format!("\u{feff}{clean}"))
        .map_err(|e| format!("임시 파일을 만들 수 없습니다: {e}"))?;
    let spawned = hidden("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&path)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let result = (|| {
        let mut child = spawned.map_err(|e| format!("PowerShell을 실행할 수 없습니다: {e}"))?;
        let mut error = None;
        if let Some(out) = child.stdout.take() {
            for line in BufReader::new(out).split(b'\n').map_while(Result::ok) {
                let line = String::from_utf8_lossy(&line);
                let Some(msg) = line.trim().strip_prefix("@@") else {
                    continue;
                };
                if let Some(e) = msg.strip_prefix("ERR ") {
                    error = Some(e.to_string());
                } else if let Ok(n) = msg.parse() {
                    on_step(n);
                }
            }
        }
        let status = child.wait().map_err(|e| e.to_string())?;
        match error {
            Some(e) => Err(e),
            None if status.success() => Ok(()),
            None => Err(format!(
                "작업이 완료되지 않았습니다 (종료 코드 {})",
                status.code().unwrap_or(-1)
            )),
        }
    })();
    let _ = std::fs::remove_file(&path);
    result
}

// ---------- 화면 ----------

enum Page {
    Ready,
    Working,
    Done,
    Failed(String),
}

enum Msg {
    Step(usize),
    End(Result<(), String>),
}

struct Setup {
    mode: Mode,
    dark: bool,
    page: Page,
    step: usize,
    rx: Option<Receiver<Msg>>,
    opts: Options,
    upgrade: bool, // 이미 설치돼 있음 → "업데이트"로 표시
    launch: bool,
}

impl Setup {
    fn new(cc: &eframe::CreationContext<'_>, mode: Mode) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        theme::install_fonts(&cc.egui_ctx);
        let dark = cc.egui_ctx.system_theme() == Some(egui::Theme::Dark);
        theme::apply(&cc.egui_ctx, dark);
        Self {
            mode,
            dark,
            page: Page::Ready,
            step: 0,
            rx: None,
            opts: Options::default(),
            upgrade: install_dir().join("mdeditor.exe").is_file(),
            launch: true,
        }
    }

    fn steps(&self) -> &'static [&'static str] {
        match self.mode {
            Mode::Install => &INSTALL_STEPS,
            Mode::Uninstall => &UNINSTALL_STEPS,
        }
    }

    fn start(&mut self, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let (mode, opts, ctx) = (self.mode, self.opts.clone(), ctx.clone());
        std::thread::spawn(move || {
            let result = execute(mode, &opts, |n| {
                let _ = tx.send(Msg::Step(n));
                ctx.request_repaint();
            });
            let _ = tx.send(Msg::End(result));
            ctx.request_repaint();
        });
        self.rx = Some(rx);
        self.step = 0;
        self.page = Page::Working;
    }

    fn poll(&mut self) {
        let Some(rx) = &self.rx else {
            return;
        };
        for msg in rx.try_iter() {
            match msg {
                Msg::Step(n) => self.step = n,
                Msg::End(Ok(())) => self.page = Page::Done,
                Msg::End(Err(e)) => self.page = Page::Failed(e),
            }
        }
    }

    fn finish(&mut self, ctx: &egui::Context) {
        if self.mode == Mode::Install && self.launch && matches!(self.page, Page::Done) {
            let _ = Command::new(install_dir().join("mdeditor.exe"))
                .current_dir(home())
                .spawn();
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn body(&mut self, ui: &mut egui::Ui, p: &Pal) {
        let install = self.mode == Mode::Install;
        match &self.page {
            Page::Ready if install => {
                let lead = if self.upgrade {
                    "이미 설치된 MDEditor를 이 버전으로 업데이트합니다. 문서와 설정은 그대로 유지됩니다."
                } else {
                    "브라우저 없이 가볍게 동작하는 Markdown 편집기를 설치합니다. 관리자 권한이 필요 없습니다."
                };
                ui.label(RichText::new(lead).size(13.5).color(p.muted));
                ui.add_space(14.0);
                let o = &mut self.opts;
                check_row(
                    ui,
                    p,
                    &mut o.assoc,
                    ".md 파일을 MDEditor로 열기",
                    "탐색기에서 더블클릭하면 바로 열립니다",
                );
                check_row(
                    ui,
                    p,
                    &mut o.menu,
                    "탐색기 우클릭 메뉴에 추가",
                    "'MDEditor로 편집', 'MDEditor로 이 폴더 열기'",
                );
                check_row(
                    ui,
                    p,
                    &mut o.desktop,
                    "바탕화면 바로가기 만들기",
                    "시작 메뉴에는 항상 등록됩니다",
                );
                check_row(
                    ui,
                    p,
                    &mut o.path,
                    "터미널 명령어로 등록 (PATH)",
                    "어디서나 'mdeditor 파일.md'로 실행",
                );
                ui.add_space(10.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(format!("설치 위치  {}", install_dir().display()))
                            .size(11.5)
                            .color(p.faint),
                    )
                    .truncate(),
                );
            }
            Page::Ready => {
                ui.label(
                    RichText::new("이 컴퓨터에서 MDEditor를 제거합니다.\n작성한 문서 파일은 삭제되지 않습니다.")
                        .size(13.5)
                        .color(p.muted),
                );
                ui.add_space(14.0);
                check_row(
                    ui,
                    p,
                    &mut self.opts.remove_settings,
                    "앱 설정도 함께 삭제",
                    "테마, 마지막으로 연 폴더, 창 크기",
                );
            }
            Page::Working => {
                let steps = self.steps();
                let active_idx = self.step.saturating_sub(1).min(steps.len() - 1);
                let label = steps[active_idx];
                let target = (self.step.max(1) as f32 / steps.len() as f32).clamp(0.08, 0.96);
                let shown =
                    ui.ctx()
                        .animate_value_with_time(egui::Id::unique("progress"), target, 0.35);
                let pct = (shown * 100.0).round().clamp(5.0, 99.0) as u32;

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{label}…"))
                            .font(FontId::new(13.5, theme::bold()))
                            .color(p.text),
                    );
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("{pct}%"))
                                .family(egui::FontFamily::Monospace)
                                .size(13.0)
                                .strong()
                                .color(p.accent),
                        );
                    });
                });
                ui.add_space(10.0);
                let (track, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 8.0), Sense::hover());
                ui.painter().rect_filled(track, 4.0, p.hover);
                let mut bar = track;
                bar.set_width(track.width() * shown.max(0.04));
                ui.painter().rect_filled(bar, 4.0, p.accent);
                ui.add_space(16.0);

                Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(10)
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 6.0;
                        for (i, step_label) in steps.iter().enumerate() {
                            let done_step = i < active_idx;
                            let cur_step = i == active_idx;
                            let (row_rect, _) = ui.allocate_exact_size(
                                vec2(ui.available_width(), 24.0),
                                Sense::hover(),
                            );
                            let icon_c = pos2(row_rect.left() + 10.0, row_rect.center().y);
                            if done_step {
                                ui.painter()
                                    .circle_filled(icon_c, 8.0, GREEN.gamma_multiply(0.18));
                                check_mark(ui, icon_c, 3.8, Stroke::new(1.8, GREEN));
                            } else if cur_step {
                                ui.painter().circle_filled(
                                    icon_c,
                                    8.0,
                                    p.accent.gamma_multiply(0.18),
                                );
                                ui.painter().circle_filled(icon_c, 3.5, p.accent);
                            } else {
                                ui.painter().circle_filled(icon_c, 3.0, p.faint);
                            }
                            let text_color = if done_step {
                                p.muted
                            } else if cur_step {
                                p.text
                            } else {
                                p.faint
                            };
                            let font = if cur_step {
                                FontId::new(12.5, theme::bold())
                            } else {
                                FontId::proportional(12.5)
                            };
                            ui.painter().text(
                                pos2(row_rect.left() + 26.0, row_rect.center().y),
                                Align2::LEFT_CENTER,
                                step_label,
                                font,
                                text_color,
                            );
                            let (badge_txt, badge_col) = if done_step {
                                ("완료", GREEN)
                            } else if cur_step {
                                ("진행 중...", p.accent)
                            } else {
                                ("대기", p.faint)
                            };
                            ui.painter().text(
                                pos2(row_rect.right() - 4.0, row_rect.center().y),
                                Align2::RIGHT_CENTER,
                                badge_txt,
                                FontId::proportional(11.5),
                                badge_col,
                            );
                        }
                    });
            }
            Page::Done => {
                let (title, detail) = if !install {
                    (
                        "MDEditor가 제거되었습니다",
                        "그동안 사용해 주셔서 감사합니다. 문서 파일은 그대로 남아 있습니다.",
                    )
                } else if self.upgrade {
                    (
                        "업데이트가 완료되었습니다!",
                        "최신 버전의 MDEditor를 바로 사용하실 수 있습니다.",
                    )
                } else {
                    (
                        "설치가 완료되었습니다!",
                        "이제 바로 실행하거나 탐색기에서 .md 파일을 열어 보세요.",
                    )
                };
                // 상단 완료 배너 카드
                Frame::new()
                    .fill(GREEN.gamma_multiply(0.10))
                    .stroke(Stroke::new(1.0, GREEN.gamma_multiply(0.4)))
                    .corner_radius(10)
                    .inner_margin(Margin::symmetric(16, 14))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
                            ui.painter().circle_filled(
                                r.center(),
                                20.0,
                                GREEN.gamma_multiply(0.20),
                            );
                            check_mark(ui, r.center(), 7.5, Stroke::new(2.5, GREEN));
                            ui.add_space(10.0);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 3.0;
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(if install {
                                            format!("MDEditor v{VERSION} {title}")
                                        } else {
                                            title.to_string()
                                        })
                                        .font(FontId::new(15.0, theme::bold()))
                                        .color(p.text),
                                    );
                                });
                                ui.label(RichText::new(detail).size(12.5).color(p.muted));
                            });
                            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                pill_badge(ui, "100% 완료", GREEN);
                            });
                        });
                    });
                ui.add_space(14.0);

                // 설치 / 제거 결과 요약 카드
                Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(10)
                    .inner_margin(Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 8.0;
                        if install {
                            let dir = install_dir();
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("설치 위치").size(12.0).color(p.muted));
                                ui.add_space(8.0);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(dir.display().to_string())
                                            .family(egui::FontFamily::Monospace)
                                            .size(11.5)
                                            .color(p.text),
                                    )
                                    .truncate(),
                                );
                                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                RichText::new("폴더 열기")
                                                    .size(11.5)
                                                    .color(p.accent),
                                            )
                                            .fill(p.accent.gamma_multiply(0.10))
                                            .stroke(Stroke::new(1.0, p.accent.gamma_multiply(0.35)))
                                            .corner_radius(5)
                                            .min_size(vec2(64.0, 24.0)),
                                        )
                                        .clicked()
                                    {
                                        let _ = Command::new("explorer.exe").arg(&dir).spawn();
                                    }
                                });
                            });
                            summary_row(
                                ui,
                                p,
                                "파일 연결",
                                ".md / .markdown 기본 연결",
                                if self.opts.assoc {
                                    "✓ 적용됨"
                                } else {
                                    "건너뜀"
                                },
                                if self.opts.assoc { GREEN } else { p.faint },
                            );
                            summary_row(
                                ui,
                                p,
                                "탐색기 메뉴",
                                "우클릭 컨텍스트 메뉴",
                                if self.opts.menu {
                                    "✓ 적용됨"
                                } else {
                                    "건너뜀"
                                },
                                if self.opts.menu { GREEN } else { p.faint },
                            );
                            let mut shortcuts = vec!["시작 메뉴"];
                            if self.opts.desktop {
                                shortcuts.push("바탕화면");
                            }
                            if self.opts.path {
                                shortcuts.push("터미널 PATH");
                            }
                            summary_row(
                                ui,
                                p,
                                "바로가기 / PATH",
                                &shortcuts.join(" · "),
                                "✓ 등록됨",
                                GREEN,
                            );
                        } else {
                            summary_row(
                                ui,
                                p,
                                "프로그램 제거",
                                "실행 파일 및 레지스트리 정리",
                                "✓ 완료",
                                GREEN,
                            );
                            summary_row(
                                ui,
                                p,
                                "사용자 문서",
                                "작성한 Markdown 파일",
                                "✓ 그대로 보존됨",
                                GREEN,
                            );
                            summary_row(
                                ui,
                                p,
                                "앱 설정",
                                "테마 · 최근 폴더 · 창 크기",
                                if self.opts.remove_settings {
                                    "삭제됨"
                                } else {
                                    "보존됨"
                                },
                                if self.opts.remove_settings {
                                    p.muted
                                } else {
                                    GREEN
                                },
                            );
                        }
                    });
            }
            Page::Failed(e) => {
                ui.add_space(12.0);
                ui.label(
                    RichText::new(if install {
                        "설치하지 못했습니다"
                    } else {
                        "제거하지 못했습니다"
                    })
                    .font(FontId::new(17.0, theme::bold()))
                    .color(RED),
                );
                ui.add_space(8.0);
                Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(8)
                    .inner_margin(Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(e).size(12.5).color(p.text));
                    });
                ui.add_space(8.0);
                ui.label(
                    RichText::new("MDEditor 창이 열려 있다면 닫고 다시 시도해 주세요.")
                        .size(12.0)
                        .color(p.faint),
                );
            }
        }
    }

    fn buttons(&mut self, ui: &mut egui::Ui, p: &Pal) {
        let ctx = ui.ctx().clone();
        let install = self.mode == Mode::Install;
        match self.page {
            Page::Ready => {
                let go = if !install {
                    ui.add(
                        egui::Button::new(
                            RichText::new("제거")
                                .size(13.0)
                                .color(Color32::WHITE)
                                .strong(),
                        )
                        .fill(RED)
                        .corner_radius(6)
                        .min_size(vec2(88.0, 32.0)),
                    )
                } else {
                    ui.add_sized(vec2(88.0, 32.0), |ui: &mut egui::Ui| {
                        primary(
                            ui,
                            p,
                            if self.upgrade {
                                "업데이트"
                            } else {
                                "설치"
                            },
                        )
                    })
                };
                // Enter는 설치에만: 제거는 실수로 시작되지 않게 버튼을 직접 눌러야 한다
                if go.clicked() || (install && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    self.start(&ctx);
                }
                if ghost(ui, p, "취소", None).clicked() {
                    self.finish(&ctx);
                }
            }
            Page::Working => {
                ui.with_layout(egui::Layout::left_to_right(Align::Center), |ui| {
                    ui.label(
                        RichText::new(if install {
                            "설치 진행 중입니다. 잠시만 기다려 주세요…"
                        } else {
                            "제거 진행 중입니다. 잠시만 기다려 주세요…"
                        })
                        .size(12.0)
                        .color(p.muted),
                    );
                });
            }
            Page::Done => {
                if install {
                    let launch_btn = ui.add(
                        egui::Button::new(
                            RichText::new("지금 MDEditor 실행 (Enter)")
                                .size(13.0)
                                .color(Color32::WHITE)
                                .strong(),
                        )
                        .fill(GREEN)
                        .stroke(Stroke::NONE)
                        .corner_radius(6)
                        .min_size(vec2(180.0, 34.0)),
                    );
                    if launch_btn.clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.launch = true;
                        self.finish(&ctx);
                    }
                    if ghost(ui, p, "닫기", None).clicked() {
                        self.launch = false;
                        self.finish(&ctx);
                    }
                } else {
                    let done = ui.add_sized(vec2(140.0, 34.0), |ui: &mut egui::Ui| {
                        primary(ui, p, "확인 및 닫기 (Enter)")
                    });
                    if done.clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.finish(&ctx);
                    }
                }
            }
            Page::Failed(_) => {
                if primary(ui, p, "다시 시도").clicked() {
                    self.start(&ctx);
                }
                if ghost(ui, p, "닫기", None).clicked() {
                    self.finish(&ctx);
                }
            }
        }
    }
}

impl eframe::App for Setup {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll();
        if matches!(self.page, Page::Working) {
            // 작업 스레드 완료/단계 전환이 마우스 입력 없이도 즉시 반영되게 한다
            ctx.request_repaint_after(Duration::from_millis(40));
            if ctx.input(|i| i.viewport().close_requested()) {
                // 작업 도중에는 창을 닫지 못하게 한다 (반쯤 설치된 상태 방지)
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }
        let p = theme::pal(self.dark);
        let install = self.mode == Mode::Install;
        ui.painter().rect_filled(ui.max_rect(), 0.0, p.bg); // 패널 구분선 자리까지 배경색으로
        egui::Panel::bottom("buttons")
            .frame(
                Frame::new()
                    .fill(p.surface)
                    .inner_margin(Margin::symmetric(24, 14)),
            )
            .show(ui, |ui| {
                ui.spacing_mut().button_padding = vec2(16.0, 6.0);
                ui.allocate_ui_with_layout(
                    vec2(ui.available_width(), 34.0),
                    egui::Layout::right_to_left(Align::Center),
                    |ui| self.buttons(ui, &p),
                );
            });
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(p.bg)
                    .inner_margin(Margin::symmetric(28, 22)),
            )
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(42.0, 42.0), Sense::hover());
                    ui.painter().rect_filled(r, 11.0, p.accent);
                    img(theme::BRAND, 22.0, Color32::WHITE).paint_at(
                        ui,
                        egui::Rect::from_center_size(r.center(), vec2(22.0, 22.0)),
                    );
                    ui.add_space(10.0);
                    ui.vertical(|ui| {
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new("MDEditor")
                                .font(FontId::new(19.0, theme::bold()))
                                .color(p.text),
                        );
                        ui.label(
                            RichText::new(format!("버전 {VERSION}"))
                                .size(11.5)
                                .color(p.faint),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        let (status_txt, status_col) = match &self.page {
                            Page::Ready if !install => ("제거 준비", RED),
                            Page::Ready if self.upgrade => ("업데이트 준비", p.accent),
                            Page::Ready => ("설치 준비", p.accent),
                            Page::Working => ("진행 중…", p.accent),
                            Page::Done if !install => ("제거 완료", GREEN),
                            Page::Done if self.upgrade => ("업데이트 완료", GREEN),
                            Page::Done => ("설치 완료", GREEN),
                            Page::Failed(_) => ("오류 발생", RED),
                        };
                        pill_badge(ui, status_txt, status_col);
                    });
                });
                ui.add_space(14.0);
                let active_stage = match &self.page {
                    Page::Ready => 0,
                    Page::Working => 1,
                    Page::Done | Page::Failed(_) => 2,
                };
                let failed = matches!(self.page, Page::Failed(_));
                stepper(
                    ui,
                    &p,
                    [
                        "옵션 선택",
                        if install {
                            "설치 진행"
                        } else {
                            "제거 진행"
                        },
                        "완료",
                    ],
                    active_stage,
                    failed,
                );
                ui.add_space(18.0);
                self.body(ui, &p);
            });
    }

    fn persist_egui_memory(&self) -> bool {
        false
    }
}

fn pill_badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    Frame::new()
        .fill(color.gamma_multiply(0.14))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.35)))
        .corner_radius(255)
        .inner_margin(Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.5).strong().color(color));
        });
}

fn stepper(ui: &mut egui::Ui, p: &Pal, labels: [&str; 3], active_stage: usize, failed: bool) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
    let seg_w = rect.width() / 3.0;
    for (i, label) in labels.iter().enumerate() {
        let done = i < active_stage || (active_stage == 2 && !failed && i == 2);
        let current = i == active_stage && !(active_stage == 2 && !failed);
        let color = if i == active_stage && failed {
            RED
        } else if done {
            GREEN
        } else if current {
            p.accent
        } else {
            p.faint
        };
        let x0 = rect.left() + seg_w * (i as f32);
        let cy = rect.center().y;
        let cx = x0 + 10.0;
        if done {
            ui.painter().circle_filled(pos2(cx, cy), 9.0, GREEN);
            check_mark(ui, pos2(cx, cy), 4.0, Stroke::new(1.8, Color32::WHITE));
        } else if current {
            ui.painter().circle_filled(pos2(cx, cy), 9.0, color);
            ui.painter().text(
                pos2(cx, cy),
                Align2::CENTER_CENTER,
                format!("{}", i + 1),
                FontId::proportional(10.5),
                Color32::WHITE,
            );
        } else {
            ui.painter().circle(
                pos2(cx, cy),
                9.0,
                p.surface,
                Stroke::new(1.0, p.border_hover),
            );
            ui.painter().text(
                pos2(cx, cy),
                Align2::CENTER_CENTER,
                format!("{}", i + 1),
                FontId::proportional(10.5),
                p.faint,
            );
        }
        let galley = ui.ctx().fonts_mut(|f| {
            f.layout_no_wrap(
                (*label).to_string(),
                if current || (active_stage == 2 && i == 2) {
                    FontId::new(12.0, theme::bold())
                } else {
                    FontId::proportional(12.0)
                },
                if current || done { p.text } else { p.muted },
            )
        });
        let text_x = cx + 15.0;
        let text_w = galley.size().x;
        ui.painter()
            .galley(pos2(text_x, cy - galley.size().y * 0.5), galley, p.text);
        if i < 2 {
            let line_start = text_x + text_w + 10.0;
            let line_end = rect.left() + seg_w * ((i + 1) as f32) - 8.0;
            if line_end > line_start {
                let line_col = if i < active_stage {
                    GREEN.gamma_multiply(0.6)
                } else {
                    p.border
                };
                ui.painter()
                    .hline(line_start..=line_end, cy, Stroke::new(1.5, line_col));
            }
        }
    }
}

fn summary_row(
    ui: &mut egui::Ui,
    p: &Pal,
    key: &str,
    desc: &str,
    status: &str,
    status_color: Color32,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(key).size(12.0).color(p.muted));
        ui.add_space(8.0);
        ui.label(RichText::new(desc).size(12.0).color(p.text));
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(status)
                    .size(12.0)
                    .strong()
                    .color(status_color),
            );
        });
    });
}

fn check_mark(ui: &egui::Ui, c: egui::Pos2, s: f32, stroke: Stroke) {
    ui.painter().line(
        vec![
            c + vec2(-s, 0.0),
            c + vec2(-s * 0.3, s * 0.7),
            c + vec2(s, -s * 0.7),
        ],
        stroke,
    );
}

// 체크박스 + 제목 + 설명 한 줄. 행 전체가 클릭된다.
fn check_row(ui: &mut egui::Ui, p: &Pal, on: &mut bool, title: &str, desc: &str) {
    let h = if desc.is_empty() { 30.0 } else { 46.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click());
    if resp.clicked() {
        *on = !*on;
    }
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect.expand2(vec2(8.0, 0.0)), 8.0, p.hover);
    }
    let x = rect.left() + 9.0;
    let y = if desc.is_empty() {
        rect.center().y
    } else {
        rect.top() + 15.0
    };
    let b = egui::Rect::from_center_size(egui::pos2(x, y), vec2(18.0, 18.0));
    if *on {
        ui.painter().rect_filled(b, 5.0, p.accent);
        check_mark(ui, b.center(), 4.5, Stroke::new(1.8, Color32::WHITE));
    } else {
        ui.painter().rect(
            b,
            5.0,
            p.elevated,
            Stroke::new(1.5, p.border_hover),
            egui::StrokeKind::Inside,
        );
    }
    ui.painter().text(
        egui::pos2(x + 19.0, y),
        egui::Align2::LEFT_CENTER,
        title,
        FontId::proportional(13.5),
        p.text,
    );
    if !desc.is_empty() {
        ui.painter().text(
            egui::pos2(x + 19.0, y + 18.0),
            egui::Align2::LEFT_CENTER,
            desc,
            FontId::proportional(11.5),
            p.faint,
        );
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_from_exe_name_and_flags() {
        let a = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert!(mode(Path::new("mdeditor.exe"), &a(&["memo.md"])).is_none());
        assert!(mode(Path::new("MDEditor-Setup (1).exe"), &[]) == Some(Mode::Install));
        assert!(mode(Path::new("mdeditor.exe"), &a(&["--install"])) == Some(Mode::Install));
        assert!(mode(Path::new("mdeditor.exe"), &a(&["--uninstall"])) == Some(Mode::Uninstall));
    }

    // 숨김 PowerShell 실행, BOM(한글), 진행/오류 줄 해석까지 실제로 돌려 본다 (시스템은 건드리지 않음)
    #[test]
    fn script_reports_steps_and_errors() {
        assert!(!INSTALL_PS1.starts_with('\u{feff}'));
        assert!(!UNINSTALL_PS1.starts_with('\u{feff}'));

        let mut steps = Vec::new();
        let ok = run_script(
            "\u{feff}# leading comment\nparam([Parameter(Mandatory)][string]$SourceExe, [Parameter(Mandatory)][string]$InstallDir, [int]$KeepPid = 0)\ntry { [Console]::OutputEncoding = [Text.Encoding]::UTF8 } catch {}\nif ($SourceExe -eq '0') { Write-Output '@@ERR SourceExe=0'; exit 1 }\nWrite-Output '@@1'\nWrite-Output \"잡음 $SourceExe $InstallDir\"\nWrite-Output '@@2'\nexit 0",
            &[
                "-SourceExe".into(),
                "C:\\Program Files\\Test [1]\\mdeditor.exe".into(),
                "-InstallDir".into(),
                "C:\\Users\\a b\\Local".into(),
                "-KeepPid".into(),
                "42".into(),
            ],
            |n| steps.push(n),
        );
        assert_eq!((ok, steps), (Ok(()), vec![1, 2]));
        let err = run_script(
            "try { [Console]::OutputEncoding = [Text.Encoding]::UTF8 } catch {}\nWrite-Output '@@ERR 한글 오류'\nexit 1",
            &[],
            |_| {},
        );
        assert_eq!(err, Err("한글 오류".to_string()));
    }

    #[test]
    fn setup_wizard_pages_render_clear_status_and_actions() {
        use egui_kittest::kittest::Queryable;
        let mut h = egui_kittest::Harness::builder()
            .with_size(vec2(540.0, 480.0))
            .with_accessibility_check(false)
            .build_eframe(|cc| Setup::new(cc, Mode::Install));
        h.run_steps(2);
        assert!(h.query_by_label("취소").is_some());

        // 진행 중 화면: 단계 체크리스트 + 진행 안내 문구
        h.state_mut().page = Page::Working;
        h.state_mut().step = 3;
        h.run_steps(2);
        assert!(h
            .query_by_label("설치 진행 중입니다. 잠시만 기다려 주세요…")
            .is_some());

        // 설치 완료 화면: 100% 완료 배지, 요약 카드, 명확한 실행/닫기 버튼
        h.state_mut().page = Page::Done;
        h.run_steps(2);
        assert!(h.query_by_label("100% 완료").is_some());
        assert!(h.query_by_label("지금 MDEditor 실행 (Enter)").is_some());
        assert!(h.query_by_label("닫기").is_some());
        assert!(h.query_by_label("폴더 열기").is_some());

        // 제거 완료 화면: 확인 및 닫기 버튼
        h.state_mut().mode = Mode::Uninstall;
        h.run_steps(2);
        assert!(h.query_by_label("확인 및 닫기 (Enter)").is_some());

        // 실패 화면
        h.state_mut().page = Page::Failed("테스트 오류".into());
        h.run_steps(2);
        assert!(h.query_by_label("다시 시도").is_some());
    }
}
