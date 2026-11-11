// 설치·제거 마법사. 실행 파일 하나가 앱과 설치 프로그램을 겸한다:
//   MDEditor-Setup.exe (파일 이름에 "setup") 또는 mdeditor --install   → 설치
//   mdeditor --uninstall ("설치된 앱"의 제거 버튼)                      → 제거
//   /S, --silent, --quiet                                              → 창 없이 진행
// 화면만 여기서 그리고, 시스템 등록은 내장 PowerShell 스크립트(install.ps1 / uninstall.ps1)가 한다.
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver};

use eframe::egui;
use egui::{vec2, Align, Color32, FontId, Frame, Margin, RichText, Sense, Stroke};

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

pub fn home() -> PathBuf {
    std::env::var_os("USERPROFILE").map_or_else(|| PathBuf::from("."), PathBuf::from)
}

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
            .with_inner_size([520.0, 450.0])
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
    match mode {
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
    }
}

// 스크립트를 임시 파일로 풀어 숨김 PowerShell로 실행한다. "@@n" 줄은 진행 단계, "@@ERR ..."는 실패 사유.
fn run_script(script: &str, args: &[String], mut on_step: impl FnMut(usize)) -> Result<(), String> {
    use std::io::{BufRead, BufReader};
    let path = std::env::temp_dir().join(format!("mdeditor-setup-{}.ps1", std::process::id()));
    // BOM이 없으면 Windows PowerShell 5.1이 한글을 ANSI로 읽는다
    std::fs::write(&path, format!("\u{feff}{script}"))
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
                let label = steps[self.step.clamp(1, steps.len()) - 1];
                ui.add_space(36.0);
                ui.label(RichText::new(format!("{label}…")).size(13.5).color(p.text));
                ui.add_space(10.0);
                let (track, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
                let target = self.step as f32 / (steps.len() + 1) as f32;
                let shown =
                    ui.ctx()
                        .animate_value_with_time(egui::Id::unique("progress"), target, 0.4);
                ui.painter().rect_filled(track, 3.0, p.hover);
                let mut bar = track;
                bar.set_width(track.width() * shown.max(0.03));
                ui.painter().rect_filled(bar, 3.0, p.accent);
                ui.add_space(10.0);
                ui.label(
                    RichText::new("잠시만 기다려 주세요. 몇 초면 끝납니다.")
                        .size(12.0)
                        .color(p.faint),
                );
            }
            Page::Done => {
                ui.add_space(20.0);
                let (r, _) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::hover());
                ui.painter()
                    .circle_filled(r.center(), 22.0, GREEN.gamma_multiply(0.16));
                check_mark(ui, r.center(), 8.0, Stroke::new(2.5, GREEN));
                ui.add_space(14.0);
                let (title, detail) = if install {
                    (
                        "설치가 완료되었습니다",
                        "시작 메뉴에서 MDEditor를 찾거나 .md 파일을 열어 보세요.",
                    )
                } else {
                    (
                        "MDEditor가 제거되었습니다",
                        "그동안 사용해 주셔서 감사합니다. 문서 파일은 그대로 남아 있습니다.",
                    )
                };
                ui.label(
                    RichText::new(title)
                        .font(FontId::new(17.0, theme::bold()))
                        .color(p.text),
                );
                ui.add_space(4.0);
                ui.label(RichText::new(detail).size(13.0).color(p.muted));
                if install {
                    ui.add_space(18.0);
                    check_row(ui, p, &mut self.launch, "지금 MDEditor 실행", "");
                }
            }
            Page::Failed(e) => {
                ui.add_space(20.0);
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
            Page::Working => {}
            Page::Done => {
                let done =
                    ui.add_sized(vec2(88.0, 32.0), |ui: &mut egui::Ui| primary(ui, p, "완료"));
                if done.clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.finish(&ctx);
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
        // 작업 도중에는 창을 닫지 못하게 한다 (반쯤 설치된 상태 방지)
        if matches!(self.page, Page::Working) && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        let p = theme::pal(self.dark);
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
                    vec2(ui.available_width(), 32.0),
                    egui::Layout::right_to_left(Align::Center),
                    |ui| self.buttons(ui, &p),
                );
            });
        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.bg).inner_margin(Margin::same(28)))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::hover());
                    ui.painter().rect_filled(r, 11.0, p.accent);
                    img(theme::BRAND, 24.0, Color32::WHITE).paint_at(
                        ui,
                        egui::Rect::from_center_size(r.center(), vec2(24.0, 24.0)),
                    );
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new("MDEditor")
                                .font(FontId::new(20.0, theme::bold()))
                                .color(p.text),
                        );
                        ui.label(
                            RichText::new(format!("버전 {VERSION}"))
                                .size(12.0)
                                .color(p.faint),
                        );
                    });
                });
                ui.add_space(22.0);
                self.body(ui, &p);
            });
    }

    fn persist_egui_memory(&self) -> bool {
        false
    }
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
        let mut steps = Vec::new();
        let ok = run_script(
            "param([string]$Name)\ntry { [Console]::OutputEncoding = [Text.Encoding]::UTF8 } catch {}\nWrite-Output '@@1'\nWrite-Output \"잡음 $Name\"\nWrite-Output '@@2'\nexit 0",
            &["-Name".into(), "a b".into()],
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
}
