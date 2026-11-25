#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// MDEditor — 순수 로컬 네이티브 Markdown 편집기 (eframe/egui). 브라우저·웹뷰·로컬 서버 없음.
// 실행: mdeditor [폴더|파일]
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui;
use egui::text::{CCursor, CCursorRange};
use egui::{Key, Modifiers};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use textops::*;

mod mermaid;
mod sas;
#[cfg(windows)] // 설치·제거 마법사는 Windows 전용 (PowerShell 엔진)
mod setup;
mod spell;
mod textops;
mod theme;
mod ui;

const AUTO_SAVE_DELAY: Duration = Duration::from_secs(2);
const LIVE_SPELL_DEBOUNCE: Duration = Duration::from_millis(300);
const UNDO_LIMIT: usize = 200; // 단계마다 변경 구간(Patch)만 기록해 메모리 점유를 최소화
const MAX_FILES: usize = 5000; // 거대한 폴더(예: 사용자 홈) 스캔 상한
const SCRATCH_KEY: &str = ""; // 첫 번째 임시 메모 문서 키 (실제 파일 경로는 비어 있을 수 없다)
const SCRATCH_PREFIX: &str = ":scratch:"; // 추가 임시 메모 탭 키 접두어

fn is_scratch_key(key: &str) -> bool {
    key == SCRATCH_KEY || key.starts_with(SCRATCH_PREFIX)
}

fn tab_base_name(key: &str) -> std::borrow::Cow<'_, str> {
    if key == SCRATCH_KEY {
        std::borrow::Cow::Borrowed("새 문서")
    } else if let Some(n) = key.strip_prefix(SCRATCH_PREFIX) {
        std::borrow::Cow::Owned(format!("새 문서 {n}"))
    } else {
        std::borrow::Cow::Borrowed(key.rsplit(['/', '\\']).next().unwrap_or(key))
    }
}

// 패닉(release는 abort)으로 죽기 직전에 저장 안 된 문서를 *.recovered.md 로 남기기 위한 스냅샷 (경로, 내용)
static RECOVERY: std::sync::Mutex<Vec<(PathBuf, String)>> = std::sync::Mutex::new(Vec::new());

// 인쇄용 임시 HTML. 링크 허용 목록(handle_links)이 이 주소 하나만 file://로 통과시킨다.
fn print_path() -> PathBuf {
    std::env::temp_dir().join(format!("mdeditor-print-{}.html", std::process::id()))
}

fn print_url() -> String {
    format!(
        "file:///{}",
        pct_encode(to_slash(&print_path()).trim_start_matches('/'))
    )
}

// OS로 넘기는 외부 URL은 안전한 웹·메일 스킴과 인쇄용 임시 HTML만 허용 (file://, smb://, ms-msdt:// 등 차단)
fn external_url_allowed(url: &str) -> bool {
    let lower = url.trim_start().to_ascii_lowercase();
    url == print_url()
        || lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
}

// 인쇄용 임시 HTML(mdeditor-print-<pid>.html)은 브라우저가 읽고 나면 필요 없다: 시작할 때 10분 지난 것을 지운다
fn clean_old_print_files() {
    let Ok(dir) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for e in dir.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("mdeditor-print-")
            && name.ends_with(".html")
            && e.metadata()
                .and_then(|m| m.modified())
                .is_ok_and(|t| t.elapsed().is_ok_and(|d| d > Duration::from_secs(600)))
        {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

fn install_panic_recovery() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Ok(snap) = RECOVERY.try_lock() {
            for (path, text) in snap.iter() {
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir); // 인자로 실행하면 문서\MDEditor가 없을 수 있다
                }
                let _ = std::fs::write(path, text);
            }
        }
        prev(info);
    }));
}

fn main() -> eframe::Result {
    install_panic_recovery();
    clean_old_print_files();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let exe = std::env::current_exe().unwrap_or_default();
    #[cfg(windows)]
    if let Some(mode) = setup::mode(&exe, &args) {
        return setup::run(mode, &args);
    }
    let arg = args.iter().find(|a| !a.starts_with('-')).map(PathBuf::from);
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    // 시작 메뉴·탐색기에서 인자 없이 실행하면(작업 디렉토리가 홈 또는 실행 파일 폴더)
    // 홈 폴더 전체를 뒤지는 대신 문서\MDEditor를 기본 작업 폴더로 쓴다. 터미널에서 실행하면 그 폴더.
    // macOS Finder/Dock에서 .app을 실행하면 작업 디렉토리가 "/"다
    let launched_bare =
        cwd == home() || cwd == Path::new("/") || exe.parent() == Some(cwd.as_path());
    let cwd = if arg.is_none() && launched_bare {
        let dir = home().join("Documents").join("MDEditor");
        let _ = std::fs::create_dir_all(&dir);
        dir
    } else {
        cwd
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MDEditor")
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([640.0, 420.0])
            .with_icon(app_icon())
            .with_drag_and_drop(true),
        #[cfg(not(target_os = "macos"))]
        glow_options: eframe::egui_glow::GlowConfiguration {
            vsync: false,
            ..Default::default()
        },
        // 기본값 HighPerformance는 듀얼 GPU Mac에서 외장 GPU를 깨워 배터리·발열을 늘린다. 편집기는 내장 GPU로 충분.
        #[cfg(target_os = "macos")]
        wgpu_options: {
            // vsync 끔(Immediate): ProMotion 화면에서 입력 직후 다음 vsync까지 기다리지 않는다.
            // 측정(백스페이스 연타): 입력→다음 프레임 p99 9ms → 5ms, vsync 켬에서 60ms 멈춤 1회 관찰
            let mut w =
                eframe::WgpuConfiguration::default().with_surface_config(eframe::SurfaceConfig {
                    present_mode: eframe::wgpu::PresentMode::AutoNoVsync,
                    ..eframe::SurfaceConfig::LOW_LATENCY
                });
            if let eframe::egui_wgpu::WgpuSetup::CreateNew(c) = &mut w.wgpu_setup {
                c.power_preference = eframe::wgpu::PowerPreference::LowPower;
            }
            w
        },
        system_font_fallback: false,
        dithering: false,
        ..Default::default()
    };
    eframe::run_native(
        "mdeditor",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, arg, &cwd)))),
    )
}

fn home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
}

fn app_icon() -> egui::IconData {
    image::load_from_memory_with_format(include_bytes!("../app.ico"), image::ImageFormat::Ico)
        .map(|img| {
            let img = img.into_rgba8();
            egui::IconData {
                width: img.width(),
                height: img.height(),
                rgba: img.into_raw(),
            }
        })
        .unwrap_or_default()
}

// ---------- 경로 / 파일 유틸 ----------

// canonicalize + Windows 확장 경로 접두어(\\?\C:\) 제거 → 표시·file:// URI에 그대로 쓸 수 있게
fn canon(p: &Path) -> Option<PathBuf> {
    let c = std::fs::canonicalize(p).ok()?;
    let s = c.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => Some(PathBuf::from(rest)),
        _ => Some(c),
    }
}

// 실행 인자(파일 또는 디렉토리)에서 작업 루트와 초기 로드 파일을 결정한다
fn resolve_target(arg: Option<&Path>, cwd: &Path) -> (PathBuf, Option<String>) {
    let Some(p) = arg else {
        return (canon(cwd).unwrap_or_else(|| cwd.to_path_buf()), None);
    };
    let abs = canon(p).unwrap_or_else(|| cwd.join(p));
    if abs.is_dir() {
        return (abs, None);
    }
    let parent = abs.parent().unwrap_or(cwd);
    let root = canon(parent).unwrap_or_else(|| parent.to_path_buf());
    let name = abs
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| is_markdown_name(n))
        .map(str::to_string);
    (root, name)
}

fn is_markdown_name(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, ext)| {
        ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown")
    })
}

fn is_image_name(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, ext)| {
        ext.eq_ignore_ascii_case("png")
            || ext.eq_ignore_ascii_case("jpg")
            || ext.eq_ignore_ascii_case("jpeg")
            || ext.eq_ignore_ascii_case("gif")
            || ext.eq_ignore_ascii_case("webp")
            || ext.eq_ignore_ascii_case("bmp")
    })
}

fn to_slash(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

fn scan_markdown_files(root: &Path) -> Vec<String> {
    let mut names = Vec::new();
    collect_markdown_files(root, root, 0, &mut names);
    names.sort_by_key(|n| n.to_lowercase());
    names
}

fn collect_markdown_files(dir: &Path, base: &Path, depth: usize, out: &mut Vec<String>) {
    if depth > 4 || out.len() >= MAX_FILES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let (Ok(ft), Some(name)) = (entry.file_type(), path.file_name().and_then(|n| n.to_str()))
        else {
            continue;
        };
        if name.starts_with('.')
            || matches!(
                name,
                "target" | "node_modules" | "dist" | "build" | "vendor" | "venv" | "AppData"
            )
        {
            continue;
        }
        if ft.is_file() && is_markdown_name(name) {
            if let Ok(rel) = path.strip_prefix(base) {
                out.push(to_slash(rel));
            }
        } else if ft.is_dir() {
            collect_markdown_files(&path, base, depth + 1, out);
        }
    }
}

// 사용자가 입력한 문서 이름 검증 (작업 폴더 밖 탈출·Windows 금지 이름 차단) + .md 보정
fn validate_name(input: &str) -> Result<String, &'static str> {
    let name = input.trim().replace('\\', "/");
    if name.is_empty() {
        return Err("이름을 입력하세요");
    }
    if name.starts_with('/') || name.contains(':') {
        return Err("작업 폴더 안의 상대 경로만 사용할 수 있습니다");
    }
    if name
        .chars()
        .any(|c| c.is_control() || "<>\"|?*".contains(c))
    {
        return Err("사용할 수 없는 문자가 있습니다 (< > \" | ? *)");
    }
    for part in name.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Err("잘못된 경로입니다");
        }
        if part.ends_with('.') || part.ends_with(' ') {
            return Err("이름은 점이나 공백으로 끝날 수 없습니다");
        }
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.as_bytes()[3].is_ascii_digit());
        if reserved {
            return Err("Windows 예약 이름은 사용할 수 없습니다");
        }
    }
    Ok(if is_markdown_name(&name) {
        name
    } else {
        format!("{name}.md")
    })
}

// 이미지 파일명 정리: 공백·특수문자는 '-'로 (마크다운 링크가 깨지지 않게)
fn sanitize_file_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '-'
            }
        })
        .collect();
    if s.trim_matches(|c| c == '.' || c == '-').is_empty() {
        "image.png".to_string()
    } else {
        s
    }
}

// 같은 이름이 있으면 name-1.ext, name-2.ext ... 로 비어 있는 경로를 고른다
fn unique_path(dir: &Path, file_name: &str) -> PathBuf {
    let first = dir.join(file_name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = match file_name.rfind('.') {
        Some(i) if i > 0 => (&file_name[..i], &file_name[i..]),
        _ => (file_name, ""),
    };
    (1..)
        .map(|n| dir.join(format!("{stem}-{n}{ext}")))
        .find(|p| !p.exists())
        .expect("unbounded range")
}

// 임시 파일에 쓴 뒤 rename → 저장 도중 크래시해도 원본이 잘리지 않는다
// 심볼릭 링크는 링크가 가리키는 실제 파일에 쓰고, 백신·동기화 프로그램이 잠깐 잡고 있는 파일은 짧게 재시도한다
fn save_atomic(path: &Path, body: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let real = if path
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        std::fs::canonicalize(path)?
    } else {
        path.to_path_buf()
    };
    let path = real.as_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}.tmp", std::process::id()));
    let tmp = PathBuf::from(tmp);
    {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(body)?;
        file.sync_all()?;
    }
    let mut res = std::fs::rename(&tmp, path);
    for _ in 0..4 {
        if res.is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(60));
        res = std::fs::rename(&tmp, path);
    }
    res.inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

// 퍼센트 인코딩/디코딩 (링크 경로·file:// URI용). 영숫자와 -._~/: 만 그대로 둔다.
fn pct_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/:".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |x: u8| (x as char).to_digit(16);
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

// ---------- 문서 ----------

// 전체 문자열 스냅샷 대신 바뀐 구간만 보관하는 최소 역패치(Patch)
#[derive(Clone)]
struct Patch {
    start: usize,
    end: usize,
    with: String,
}

impl Patch {
    fn between(old: &str, new: &str) -> Self {
        let (o, n) = (old.as_bytes(), new.as_bytes());
        let mut prefix = o.iter().zip(n).take_while(|(x, y)| x == y).count();
        while prefix > 0 && (!old.is_char_boundary(prefix) || !new.is_char_boundary(prefix)) {
            prefix -= 1;
        }
        let max = o.len().min(n.len()) - prefix;
        let mut suffix = o
            .iter()
            .rev()
            .zip(n.iter().rev())
            .take(max)
            .take_while(|(x, y)| x == y)
            .count();
        while suffix > 0
            && (!old.is_char_boundary(o.len() - suffix) || !new.is_char_boundary(n.len() - suffix))
        {
            suffix -= 1;
        }
        Self {
            start: prefix,
            end: n.len() - suffix,
            with: old[prefix..o.len() - suffix].to_string(),
        }
    }

    fn apply(&self, text: &mut String) {
        text.replace_range(self.start..self.end, &self.with);
    }
}

struct Doc {
    text: String,
    saved: String,
    crlf: bool,
    bom: bool,
    edited_at: Option<Instant>,
    dirty_flag: bool,
    scratch: bool,
    chars: usize,
    lines: usize,
    blocks: Vec<Range<usize>>,
    outline: Vec<(usize, String, usize, usize, usize)>,
    rev: u64,
    mtime: Option<std::time::SystemTime>, // 마지막으로 읽거나 쓴 시점의 디스크 수정 시각
    prev: String,                         // 직전 refresh_meta 때의 텍스트 (실행 취소 기록용)
    undo: Vec<Patch>,
    redo: Vec<Patch>,
    typed_at: Option<Instant>,
    // 탭 전환 시 복원되는 문서별 편집 위치/선택 상태
    view: TabView,
    // 접은 제목 줄 원문 (보기 상태일 뿐 파일에는 저장하지 않는다)
    folds: HashSet<String>,
}

// 탭 전환 때 저장·복원하는 편집 위치/선택 상태
#[derive(Default)]
struct TabView {
    active: Option<usize>,
    blank: usize,
    block_sel: Option<Range<usize>>,
    block_sel_anchor: Option<usize>,
    block_sel_head: Option<usize>,
    last_sel: Option<Sel>,
}

fn modified(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

// 화면에 보이는 내용(블록)이 같은지. 빈 블록을 만들며 넣었다 뺀 줄바꿈 차이는 무시한다.
fn same_blocks(a: &str, b: &str) -> bool {
    let (x, y) = (split_blocks(a), split_blocks(b));
    x.len() == y.len() && x.iter().zip(&y).all(|(p, q)| a[p.clone()] == b[q.clone()])
}

// old → new 로 바뀐 구간의 끝(new 기준 바이트). 실행 취소 뒤 커서를 둘 자리.
fn change_end(old: &str, new: &str) -> usize {
    let (o, n) = (old.as_bytes(), new.as_bytes());
    let prefix = o.iter().zip(n).take_while(|(x, y)| x == y).count();
    let max = o.len().min(n.len()) - prefix;
    let suffix = o
        .iter()
        .rev()
        .zip(n.iter().rev())
        .take(max)
        .take_while(|(x, y)| x == y)
        .count();
    let mut at = n.len() - suffix;
    while !new.is_char_boundary(at) {
        at += 1;
    }
    at
}

impl Doc {
    fn load(path: &Path) -> std::io::Result<Doc> {
        let raw = String::from_utf8(std::fs::read(path)?).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "UTF-8 텍스트 파일이 아닙니다",
            )
        })?;
        let bom = raw.starts_with('\u{feff}');
        let raw = raw.trim_start_matches('\u{feff}');
        let crlf = raw.contains("\r\n");
        let text = raw.replace("\r\n", "\n");
        let mut doc = Doc {
            saved: text.clone(),
            prev: text.clone(),
            mtime: modified(path),
            undo: Vec::new(),
            redo: Vec::new(),
            typed_at: None,
            text,
            crlf,
            bom,
            edited_at: None,
            dirty_flag: false,
            scratch: false,
            chars: 0,
            lines: 1,
            blocks: Vec::new(),
            outline: Vec::new(),
            rev: 0,
            view: TabView::default(),
            folds: HashSet::new(),
        };
        doc.refresh_meta();
        if doc.text.is_empty() {
            doc.view.active = Some(0);
            doc.view.last_sel = Some((0, 0));
        }
        Ok(doc)
    }

    fn scratch(text: String) -> Doc {
        let empty = text.is_empty();
        let mut doc = Doc {
            saved: text.clone(),
            prev: text.clone(),
            mtime: None,
            undo: Vec::new(),
            redo: Vec::new(),
            typed_at: None,
            text,
            crlf: false,
            bom: false,
            edited_at: None,
            dirty_flag: false,
            scratch: true,
            chars: 0,
            lines: 1,
            blocks: Vec::new(),
            outline: Vec::new(),
            rev: 0,
            view: TabView {
                active: empty.then_some(0),
                last_sel: empty.then_some((0, 0)),
                ..TabView::default()
            },
            folds: HashSet::new(),
        };
        doc.refresh_meta();
        doc
    }

    // 실행 취소 기록. 모든 편집이 refresh_meta를 거치므로 여기 한 곳에서 직전 텍스트와 비교한다.
    // 끊기지 않고 이어 치는 입력(0.7초 이내, 몇 글자 차이, 줄 수 그대로)은 한 단계로 묶는다.
    fn record_history(&mut self, same_lines: bool) {
        if self.text == self.prev {
            return;
        }
        let now = Instant::now();
        let typing = same_lines
            && self
                .typed_at
                .is_some_and(|t| now.duration_since(t) < Duration::from_millis(700))
            && self.text.len().abs_diff(self.prev.len()) <= 4;
        let patch = Patch::between(&self.prev, &self.text);
        if typing {
            if let Some(last) = self.undo.last_mut() {
                let p_end_in_prev = patch.start + patch.with.len();
                let o_start = patch.start.min(last.start);
                let o_end = p_end_in_prev.max(last.end);
                let mut merged = String::with_capacity(
                    (last.start - o_start) + last.with.len() + (o_end - last.end),
                );
                merged.push_str(&self.prev[o_start..last.start]);
                merged.push_str(&last.with);
                merged.push_str(&self.prev[last.end..o_end]);
                last.start = o_start;
                last.end = o_end + patch.end - p_end_in_prev;
                last.with = merged;
            } else {
                self.undo.push(patch);
            }
        } else {
            if self.undo.len() >= UNDO_LIMIT {
                self.undo.remove(0);
            }
            self.undo.push(patch);
        }
        self.prev.clear();
        self.prev.push_str(&self.text);
        self.redo.clear();
        self.typed_at = Some(now);
    }

    // 실행 취소(undo=true) / 다시 실행. 바뀐 구간의 끝 바이트 위치를 돌려준다.
    fn step_history(&mut self, undo: bool) -> Option<usize> {
        let (from, to) = if undo {
            (&mut self.undo, &mut self.redo)
        } else {
            (&mut self.redo, &mut self.undo)
        };
        let first = from.pop()?;
        let old = self.text.clone();
        first.apply(&mut self.text);
        while same_blocks(&self.text, &old) {
            match from.pop() {
                Some(p) => p.apply(&mut self.text),
                None => break,
            }
        }
        let caret = change_end(&old, &self.text);
        to.push(Patch::between(&old, &self.text));
        self.prev.clear();
        self.prev.push_str(&self.text);
        self.typed_at = None;
        self.edited_at = Some(Instant::now());
        self.refresh_meta();
        Some(caret)
    }

    fn refresh_meta(&mut self) {
        let lines = self.text.bytes().filter(|&b| b == b'\n').count() + 1;
        self.record_history(lines == self.lines);
        self.lines = lines;
        self.dirty_flag = self.text.len() != self.saved.len() || self.text != self.saved;
        self.chars = char_len(&self.text);
        split_blocks_into(&self.text, &mut self.blocks);
        outline_with_bytes_into(&self.text, &mut self.outline);
        self.rev = self.rev.wrapping_add(1);
    }

    // 활성 블록 1개만 바뀐 경우 전체 문서를 다시 스캔·복제하지 않고 증분 갱신한다.
    // 반환값: 바뀐 블록의 글자 수(char_len(new_block)).
    fn apply_block_edit(&mut self, r: Range<usize>, new_block: &str) -> usize {
        let old = &self.text[r.clone()];
        let new_chars = char_len(new_block);
        if old == new_block {
            return new_chars;
        }
        let old_nl = old.bytes().filter(|&b| b == b'\n').count();
        let new_nl = new_block.bytes().filter(|&b| b == b'\n').count();
        let old_chars = char_len(old);
        let now = Instant::now();
        let typing = old_nl == new_nl
            && self
                .typed_at
                .is_some_and(|t| now.duration_since(t) < Duration::from_millis(700))
            && new_block.len().abs_diff(old.len()) <= 4;
        let sub = Patch::between(old, new_block);
        let patch = Patch {
            start: r.start + sub.start,
            end: r.start + sub.end,
            with: sub.with,
        };
        let old_doc_len = self.text.len();
        let synced_prev = self.prev.len() == old_doc_len;
        if typing && synced_prev {
            if let Some(last) = self.undo.last_mut() {
                let p_end_in_prev = patch.start + patch.with.len();
                let o_start = patch.start.min(last.start);
                let o_end = p_end_in_prev.max(last.end);
                let mut merged = String::with_capacity(
                    (last.start - o_start) + last.with.len() + (o_end - last.end),
                );
                merged.push_str(&self.text[o_start..last.start]);
                merged.push_str(&last.with);
                merged.push_str(&self.text[last.end..o_end]);
                last.start = o_start;
                last.end = o_end + patch.end - p_end_in_prev;
                last.with = merged;
            } else {
                self.undo.push(patch);
            }
        } else {
            if self.undo.len() >= UNDO_LIMIT {
                self.undo.remove(0);
            }
            self.undo.push(patch);
        }
        self.redo.clear();
        self.typed_at = Some(now);

        let is_self_contained = |s: &str| -> bool {
            if s.is_empty() || s.contains('\r') {
                return false;
            }
            let mut started = false;
            let mut open_fence: Option<&str> = None;
            let mut is_heading = false;
            for line in s.split('\n') {
                let t = line.trim_start();
                if let Some(close) = open_fence {
                    let closes = if close == "```" || close == "~~~" {
                        t.starts_with(close)
                    } else {
                        t.trim_end() == close
                    };
                    if closes {
                        open_fence = None;
                    }
                    continue;
                }
                if line.is_empty() || is_heading {
                    return false;
                }
                if !started {
                    started = true;
                    open_fence = if t.starts_with("```") {
                        Some("```")
                    } else if t.starts_with("~~~") {
                        Some("~~~")
                    } else if t.trim_end() == "$$" {
                        Some("$$")
                    } else if t.starts_with(":::toggle") {
                        Some(":::")
                    } else {
                        None
                    };
                    if open_fence.is_none() && heading_level(t).is_some() {
                        is_heading = true;
                    }
                } else if t.starts_with("```")
                    || t.starts_with("~~~")
                    || t.trim_end() == "$$"
                    || t.starts_with(":::toggle")
                    || heading_level(t).is_some()
                {
                    return false;
                }
            }
            started && open_fence.is_none()
        };
        let is_closed_code = |s: &str| -> bool {
            let t = s.trim_start();
            let fence = if t.starts_with("```") {
                "```"
            } else if t.starts_with("~~~") {
                "~~~"
            } else {
                return false;
            };
            t.find('\n')
                .and_then(|nl| t[nl + 1..].lines().last())
                .is_some_and(|last| last.trim_start().starts_with(fence))
        };
        let can_in_place = is_self_contained(old) && is_self_contained(new_block);
        let in_place_idx = if can_in_place {
            self.blocks
                .binary_search_by_key(&r.start, |b| b.start)
                .ok()
                .filter(|&i| self.blocks[i] == r)
        } else {
            None
        };
        let outline_unchanged = in_place_idx.is_some()
            && ((!old.contains('#') && !new_block.contains('#'))
                || (is_closed_code(old) && is_closed_code(new_block)));
        let line_delta = new_nl as isize - old_nl as isize;
        let byte_delta = new_block.len() as isize - old.len() as isize;
        let char_delta = new_chars as isize - old_chars as isize;

        self.text.replace_range(r.clone(), new_block);
        if synced_prev {
            self.prev.replace_range(r.clone(), new_block);
        } else {
            self.prev.clear();
            self.prev.push_str(&self.text);
        }
        self.lines = (self.lines + new_nl).saturating_sub(old_nl);
        self.chars = (self.chars + new_chars).saturating_sub(old_chars);
        self.dirty_flag = self.text.len() != self.saved.len() || self.text != self.saved;

        if let Some(idx) = in_place_idx {
            self.blocks[idx].end = (r.end as isize + byte_delta) as usize;
            for b in &mut self.blocks[idx + 1..] {
                b.start = (b.start as isize + byte_delta) as usize;
                b.end = (b.end as isize + byte_delta) as usize;
            }
            if outline_unchanged {
                for item in &mut self.outline {
                    if item.4 > r.start {
                        item.2 = (item.2 as isize + line_delta) as usize;
                        item.3 = (item.3 as isize + char_delta) as usize;
                        item.4 = (item.4 as isize + byte_delta) as usize;
                    }
                }
            } else {
                outline_with_bytes_into(&self.text, &mut self.outline);
            }
        } else {
            split_blocks_into(&self.text, &mut self.blocks);
            outline_with_bytes_into(&self.text, &mut self.outline);
        }
        self.rev = self.rev.wrapping_add(1);
        new_chars
    }

    fn dirty(&self) -> bool {
        self.dirty_flag || self.text.len() != self.saved.len()
    }

    // 원래 줄바꿈(CRLF)과 BOM을 보존해서 디스크 형식으로 되돌린다
    fn encoded(&self) -> String {
        let body = if self.crlf {
            self.text.replace('\n', "\r\n")
        } else {
            self.text.clone()
        };
        if self.bom {
            format!("\u{feff}{body}")
        } else {
            body
        }
    }
}

// ---------- 앱 ----------

enum Dialog {
    Files {
        filter: String,
        sel: usize,
    },
    NewFile(String),
    SaveScratch(String),
    Rename {
        from: String,
        to: String,
    },
    Delete(String),
    Unsaved(After),
    Conflict(String), // 저장하려는 파일이 그 사이 디스크에서 바뀜
    Image {
        url_tab: bool,
        url: String,
        alt: String,
    },
    // 링크 넣기·고치기 (Ctrl+K). range: 바꿀 문서 문자 구간, existing: 이미 있는 링크를 고치는 중
    Link {
        range: Sel,
        label: String,
        url: String,
        existing: bool,
    },
    Spellcheck {
        selected: usize,
        cached_key: Option<String>,
        cached_rev: u64,
        cached_spell_rev: u64,
        issues: Vec<spell::SpellIssue>,
        checking: bool, // 작업 스레드가 검사 중 (issues는 직전 결과)
    },
    Help,
}

enum After {
    Close,
    CloseTab(String),
    Switch(PathBuf, Option<String>),
}

#[derive(Default)]
struct Find {
    open: bool,
    replace: bool,
    query: String,
    with: String,
    case: bool,
    word: bool,
    regex: bool,
    focus: bool,
    cache_key: (u64, String, FindOpts),
    cached: Vec<Sel>,
    err: Option<String>, // 잘못된 정규식
}

impl Find {
    fn opts(&self) -> FindOpts {
        FindOpts {
            case: self.case,
            word: self.word,
            regex: self.regex,
        }
    }

    fn matches(&mut self, doc: &Doc) -> &[Sel] {
        let o = self.opts();
        if self.cache_key.0 != doc.rev || self.cache_key.1 != self.query || self.cache_key.2 != o {
            let r = find_replacements(&doc.text, &self.query, o, None);
            self.cached = r
                .as_ref()
                .map_or_else(|_| Vec::new(), |v| v.iter().map(|x| x.0).collect());
            self.err = r.err();
            self.cache_key = (doc.rev, self.query.clone(), o);
        }
        &self.cached
    }
}

#[derive(Clone, Copy)]
enum Fmt {
    Wrap(&'static str, &'static str, &'static str),
    Block(BlockKind),
    Clear,
    BrushCopy,  // 서식 복사
    BrushPaste, // 서식 붙여넣기
}

// 슬래시(/) 블록 메뉴 (웹 버전과 같은 목록)
struct SlashItem {
    icon: &'static str,
    title: &'static str,
    desc: &'static str,
    tpl: &'static str,
    caret: usize, // 템플릿 안 커서 위치(바이트), END = 끝
    color: [u8; 3],
    advanced: bool,
}

const END: usize = usize::MAX;

#[rustfmt::skip]
const SLASH: &[SlashItem] = &[
    SlashItem { icon: "¶", title: "텍스트", desc: "일반 문단 작성", tpl: "", caret: END, color: [100, 116, 139], advanced: false },
    SlashItem { icon: "H1", title: "제목 1", desc: "가장 큰 섹션 제목 (#)", tpl: "# ", caret: END, color: [35, 131, 226], advanced: false },
    SlashItem { icon: "H2", title: "제목 2", desc: "중간 크기 제목 (##)", tpl: "## ", caret: END, color: [35, 131, 226], advanced: false },
    SlashItem { icon: "H3", title: "제목 3", desc: "작은 소제목 (###)", tpl: "### ", caret: END, color: [35, 131, 226], advanced: false },
    SlashItem { icon: "▣", title: "할 일 목록", desc: "인터랙티브 체크박스 (- [ ])", tpl: "- [ ] ", caret: END, color: [16, 185, 129], advanced: false },
    SlashItem { icon: "•", title: "글머리 기호 목록", desc: "원형 불릿 목록 (-)", tpl: "- ", caret: END, color: [245, 158, 11], advanced: false },
    SlashItem { icon: "1.", title: "번호 매기기 목록", desc: "순서가 있는 번호 목록 (1.)", tpl: "1. ", caret: END, color: [245, 158, 11], advanced: false },
    SlashItem { icon: "”", title: "인용구", desc: "왼쪽 강조선 인용 블록 (>)", tpl: "> ", caret: END, color: [139, 92, 246], advanced: false },
    SlashItem { icon: "—", title: "구분선", desc: "콘텐츠 구분선 (---)", tpl: "---", caret: END, color: [100, 116, 139], advanced: false },
    SlashItem { icon: "※", title: "콜아웃 블록", desc: "Notion / GitHub Alert 박스", tpl: "> [!NOTE]\n> ", caret: END, color: [234, 179, 8], advanced: true },
    SlashItem { icon: "<>", title: "코드 블록", desc: "구문 강조 코드 블록 (```)", tpl: "```\n\n```", caret: 4, color: [244, 63, 94], advanced: true },
    SlashItem { icon: "▦", title: "표 (Table)", desc: "2x3 마크다운 데이터 표", tpl: "| 제목 | 내용 |\n| --- | --- |\n|  |  |", caret: 2, color: [6, 182, 212], advanced: true },
    SlashItem { icon: "▶", title: "접기 (토글)", desc: "상세 내용을 숨기고 펼치는 블록 (:::toggle)", tpl: ":::toggle 제목\n내용\n:::", caret: 10, color: [168, 85, 247], advanced: true },
    SlashItem { icon: "fx", title: "수학 공식", desc: "LaTeX 수식 ($$)", tpl: "$$\n\n$$", caret: 3, color: [16, 185, 129], advanced: true },
    SlashItem { icon: "◈", title: "Mermaid 다이어그램", desc: "순서도 및 아키텍처 다이어그램", tpl: "```mermaid\ngraph TD\n  A --> B\n```", caret: 11, color: [99, 102, 241], advanced: true },
    SlashItem { icon: "▨", title: "이미지", desc: "로컬 또는 웹 이미지 삽입 (![설명](경로))", tpl: "", caret: END, color: [20, 184, 166], advanced: true },
    SlashItem { icon: "^", title: "각주", desc: "문서 하단 각주 링크 ([^1])", tpl: "[^1]: ", caret: END, color: [100, 116, 139], advanced: true },
];

fn slash_matches(query: &str) -> Vec<usize> {
    let q = query.to_lowercase();
    SLASH
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            s.title.to_lowercase().contains(&q)
                || s.desc.to_lowercase().contains(&q)
                || s.icon.to_lowercase().contains(&q)
        })
        .map(|(i, _)| i)
        .collect()
}

#[derive(Clone, Copy)]
struct EditorCursorInfo {
    block_start: usize,
    doc_rev: u64,
    on_first_row: bool,
    on_last_row: bool,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum CanvasSub {
    TurnInto,
    Insert,
}

#[derive(Clone)]
enum ContextMenu {
    Canvas(egui::Pos2),
    Tab(String, egui::Pos2),
    FileItem(String, egui::Pos2),
}

type CodeGalleyCache = HashMap<u64, (std::sync::Arc<egui::Galley>, Vec<f32>)>;
type InlineGalleyCache = HashMap<u64, (std::sync::Arc<egui::Galley>, Vec<(Range<usize>, String)>)>;
type TableParseCache = HashMap<u64, (String, std::sync::Arc<MdTable>)>;
type HangingLineCache = (u32, HashMap<u64, std::sync::Arc<egui::Galley>>);

struct AutoSaveReq {
    name: String,
    root: PathBuf,
    path: PathBuf,
    encoded: String,
    saved_text: String,
    prev_saved: String,
    prev_mtime: Option<std::time::SystemTime>,
    rev: u64,
}

enum AutoSaveOutcome {
    Saved {
        mtime: Option<std::time::SystemTime>,
    },
    Conflict,
    Err(String),
}

struct AutoSaveRes {
    name: String,
    root: PathBuf,
    rev: u64,
    saved_text: String,
    outcome: AutoSaveOutcome,
}

struct AutoSaveWorker {
    tx: std::sync::mpsc::Sender<AutoSaveReq>,
    rx: std::sync::mpsc::Receiver<AutoSaveRes>,
    pending: std::sync::Arc<(std::sync::Mutex<usize>, std::sync::Condvar)>,
}

impl AutoSaveWorker {
    fn new(wake: impl Fn() + Send + 'static) -> Self {
        let (tx, jobs) = std::sync::mpsc::channel::<AutoSaveReq>();
        let (done, rx) = std::sync::mpsc::channel::<AutoSaveRes>();
        let pending =
            std::sync::Arc::new((std::sync::Mutex::new(0usize), std::sync::Condvar::new()));
        let pending_bg = std::sync::Arc::clone(&pending);
        let _ = std::thread::Builder::new()
            .name("autosave".into())
            .spawn(move || {
                while let Ok(req) = jobs.recv() {
                    let external = req.prev_mtime.is_some()
                        && req.path.is_file()
                        && modified(&req.path).is_some_and(|d| Some(d) != req.prev_mtime)
                        && Doc::load(&req.path).map_or(true, |f| f.text != req.prev_saved);
                    let outcome = if external {
                        AutoSaveOutcome::Conflict
                    } else {
                        match save_atomic(&req.path, req.encoded.as_bytes()) {
                            Ok(()) => AutoSaveOutcome::Saved {
                                mtime: modified(&req.path),
                            },
                            Err(e) => AutoSaveOutcome::Err(e.to_string()),
                        }
                    };
                    let res = AutoSaveRes {
                        name: req.name,
                        root: req.root,
                        rev: req.rev,
                        saved_text: req.saved_text,
                        outcome,
                    };
                    let send_err = done.send(res).is_err();
                    if let Ok(mut n) = pending_bg.0.lock() {
                        *n = n.saturating_sub(1);
                        pending_bg.1.notify_all();
                    }
                    if send_err {
                        break;
                    }
                    wake();
                }
            });
        Self { tx, rx, pending }
    }

    fn submit(&self, req: AutoSaveReq) -> bool {
        if let Ok(mut n) = self.pending.0.lock() {
            *n += 1;
        }
        if self.tx.send(req).is_err() {
            if let Ok(mut n) = self.pending.0.lock() {
                *n = n.saturating_sub(1);
                self.pending.1.notify_all();
            }
            return false;
        }
        true
    }

    fn poll(&self) -> Vec<AutoSaveRes> {
        self.rx.try_iter().collect()
    }

    fn wait_idle(&self) {
        if let Ok(mut n) = self.pending.0.lock() {
            while *n > 0 {
                n = match self.pending.1.wait(n) {
                    Ok(g) => g,
                    Err(e) => e.into_inner(),
                };
            }
        }
    }
}

struct App {
    root: PathBuf,
    files: Vec<String>,
    docs: HashMap<String, Doc>,
    tabs: Vec<String>,
    current: Option<String>,
    dark: bool,
    full_width: bool,
    focus_mode: bool,
    small_text: bool,
    line_numbers: bool,
    spellcheck: bool,
    spell_ignore: std::collections::HashSet<String>,
    spell_worker: Option<spell::Worker>, // 전체 검사 작업 스레드 (처음 쓸 때 만든다)
    spell_ticket: u64,                   // 마지막으로 보낸 전체 검사 번호
    spell_rev: u64,
    spell_block_cache: Option<(u64, u64, usize, Vec<spell::SpellIssue>)>,
    live_spell_worker: Option<spell::Worker>, // 실시간 블록 검사 작업 스레드 (처음 쓸 때 만든다)
    live_spell_ticket: u64,
    live_spell_pending: Option<(u64, u64, Instant)>,
    live_spell_submitted: Option<(u64, u64, u64, usize)>,
    font_family: String,
    md_cache: CommonMarkCache,
    status: String,
    toast: (String, Option<Instant>),
    find: Find,
    dialog: Option<Dialog>,
    context_menu: Option<ContextMenu>,
    canvas_sub: Option<CanvasSub>,
    active: Option<usize>,           // 캔버스에서 편집 중인 블록의 시작 바이트
    blank: usize,                    // 활성 블록을 만들며 넣은 줄바꿈 수 (비운 채 떠나면 되돌린다)
    block_sel: Option<Range<usize>>, // 다중 블록 또는 전체 문서 선택 바이트 구간
    block_sel_anchor: Option<usize>, // Shift+방향키/Shift+클릭 다중 블록 선택 기준점
    block_sel_head: Option<usize>,
    drag_anchor: Option<(usize, egui::Pos2)>, // 비활성 블록에서 시작한 드래그 기준점
    drag_last_pos: Option<egui::Pos2>,
    drag_origin_rel: Option<egui::Vec2>, // 스크롤해도 드래그 시작 문서 위치를 유지하기 위한 ScrollArea 상대 좌표
    drag_anchor_block: Option<Range<usize>>, // 다중 블록 드래그 시작 블록 범위 (스크롤 시 뒤쪽 선택 끊김 방지)
    last_sel: Option<Sel>,
    pending_sel: Option<Sel>,
    pending_byte_caret: Option<(usize, usize)>,
    pending_click_pos: Option<(usize, egui::Pos2)>,
    focus_editor: bool,
    ime_composing: bool,
    // 다중 커서(Ctrl+클릭 / Alt+드래그): 편집 중인 블록 id와 그 안의 추가 커서들(문자 인덱스)
    extra_carets: Option<(egui::Id, Vec<usize>)>,
    pending_enter: bool,
    raw_table_mode: bool,
    table_focus_cell: Option<(usize, usize)>,
    last_table_cell: Option<(usize, usize)>,
    table_cell_draft: Option<(egui::Id, String)>,
    last_link_hint: Option<String>,
    scroll_cursor: Option<Option<egui::Align>>,
    scroll_tab_to_active: bool,
    cursor_info: Option<EditorCursorInfo>,
    slash_sel: usize,
    toc_open: bool,
    toc_active: Option<usize>,
    scroll_to: Option<usize>,
    allow_close: bool,
    last_title: String,
    block_heights: HashMap<egui::Id, f32>,
    height_cache_key: (u32, u32),
    active_char_cache: (usize, u64, usize, usize),
    active_line_cache: Option<(String, usize, u64, usize)>,
    status_line_cache: Option<(String, u64, usize, usize)>,
    canvas_slots: Vec<Range<usize>>,
    canvas_items: Vec<(Range<usize>, bool, usize)>,
    canvas_rects: Vec<(Range<usize>, egui::Rect)>,
    edit_buf: String,
    editor_galley_cache: Option<(u64, u32, u32, bool, std::sync::Arc<egui::Galley>)>,
    code_galley_cache: CodeGalleyCache,
    inline_galley_cache: InlineGalleyCache,
    table_cache: TableParseCache,
    hanging_line_cache: HangingLineCache,
    clear_galleys_next_frame: bool,
    line_num_buf: String,
    cached_scheme: (PathBuf, String),
    cached_root_label: (PathBuf, String),
    scan: ScanSlot, // 백그라운드 폴더 스캔 결과 (세대, 목록)
    scan_gen: u64,
    ectx: egui::Context,
    recovery_at: Option<Instant>,
    recovery_sig: Vec<(String, u64)>,
    auto_save_worker: Option<AutoSaveWorker>,
    auto_save_in_flight: HashSet<String>,
    draft_dirty: bool,
    pasted: PastedSlot, // (대상 탭 키, 백그라운드 이미지 저장 결과)
    format_brush: Option<FormatSample>, // 복사해 둔 서식 (서식 복사 / 붙여넣기)
}

type ScanSlot = std::sync::Arc<std::sync::Mutex<Option<(u64, Vec<String>)>>>; // (세대, 파일 목록)
type PastedSlot = std::sync::Arc<std::sync::Mutex<Vec<(String, Result<String, String>)>>>;

// 한 프레임의 키 입력 요약 (handle_shortcuts가 계산해 하위 처리 함수로 넘긴다)
#[derive(Clone, Copy)]
struct Keys {
    has_cmd: bool,
    has_alt: bool,
    nav_or_edit: bool,
    focused: bool,
    bs_del_tab: bool,
    block_ev: bool,
}

fn heading_slug(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.trim().chars() {
        if c.is_alphanumeric() {
            for lc in c.to_lowercase() {
                out.push(lc);
            }
        } else if c == ' ' || c == '-' || c == '_' {
            out.push('-');
        }
    }
    out
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, arg: Option<PathBuf>, cwd: &Path) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let get = |k: &str| cc.storage.and_then(|s| s.get_string(k));
        let font_family = get("font_family")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "auto".into());
        theme::install_fonts_with(&cc.egui_ctx, &font_family);
        let has_arg = arg.is_some();
        // "drafts": 비어 있지 않은 모든 임시 메모 탭(U+0001 구분). 없으면 예전 형식("draft") 하나만 복원한다.
        let mut drafts: Vec<String> = get("drafts")
            .map(|d| d.split('\u{1}').map(str::to_string).collect())
            .unwrap_or_default();
        if drafts.is_empty() {
            drafts.push(get("draft").unwrap_or_default());
        }
        let draft = drafts.remove(0);
        let (root, open) = match arg {
            Some(p) => resolve_target(Some(&p), cwd),
            None => {
                let r = get("root")
                    .map(PathBuf::from)
                    .filter(|p| p.is_dir())
                    .unwrap_or_else(|| resolve_target(None, cwd).0);
                (r, None)
            }
        };
        let dark = match get("theme").as_deref() {
            Some("light") => false,
            Some("dark") => true,
            _ => cc.egui_ctx.system_theme() != Some(egui::Theme::Light),
        };
        theme::apply(&cc.egui_ctx, dark);
        let flag = |k: &str| get(k).as_deref() == Some("true");
        let spellcheck = get("spellcheck").as_deref() != Some("false");
        let spell_ignore: std::collections::HashSet<String> = get("spell_ignore")
            .map(|s| {
                s.split('\u{1}')
                    .filter(|w| !w.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let mut app = Self {
            root: PathBuf::new(),
            files: Vec::new(),
            docs: HashMap::new(),
            tabs: Vec::new(),
            current: None,
            dark,
            full_width: flag("full_width"),
            focus_mode: false,
            small_text: flag("small_text"),
            line_numbers: flag("line_numbers"),
            spellcheck,
            spell_ignore,
            spell_worker: None,
            spell_ticket: 0,
            spell_rev: 0,
            spell_block_cache: None,
            live_spell_worker: None,
            live_spell_ticket: 0,
            live_spell_pending: None,
            live_spell_submitted: None,
            font_family,
            md_cache: CommonMarkCache::default(),
            status: String::new(),
            toast: (String::new(), None),
            find: Find::default(),
            dialog: None,
            context_menu: None,
            canvas_sub: None,
            active: None,
            blank: 0,
            block_sel: None,
            block_sel_anchor: None,
            block_sel_head: None,
            drag_anchor: None,
            drag_last_pos: None,
            drag_origin_rel: None,
            drag_anchor_block: None,
            last_sel: None,
            pending_sel: None,
            pending_byte_caret: None,
            pending_click_pos: None,
            focus_editor: false,
            ime_composing: false,
            extra_carets: None,
            pending_enter: false,
            raw_table_mode: false,
            table_focus_cell: None,
            last_table_cell: None,
            table_cell_draft: None,
            last_link_hint: None,
            scroll_cursor: None,
            scroll_tab_to_active: false,
            cursor_info: None,
            slash_sel: 0,
            toc_open: false,
            toc_active: None,
            scroll_to: None,
            allow_close: false,
            last_title: String::new(),
            block_heights: HashMap::new(),
            height_cache_key: (0, 0),
            active_char_cache: (usize::MAX, u64::MAX, 0, 0),
            active_line_cache: None,
            status_line_cache: None,
            canvas_slots: Vec::new(),
            canvas_items: Vec::new(),
            canvas_rects: Vec::new(),
            edit_buf: String::new(),
            editor_galley_cache: None,
            code_galley_cache: HashMap::new(),
            inline_galley_cache: HashMap::new(),
            table_cache: HashMap::new(),
            hanging_line_cache: (0, HashMap::new()),
            clear_galleys_next_frame: false,
            line_num_buf: String::new(),
            cached_scheme: (PathBuf::new(), String::new()),
            cached_root_label: (PathBuf::new(), String::new()),
            scan: Default::default(),
            scan_gen: 0,
            ectx: cc.egui_ctx.clone(),
            recovery_at: None,
            recovery_sig: Vec::new(),
            auto_save_worker: None,
            auto_save_in_flight: HashSet::new(),
            draft_dirty: false,
            pasted: Default::default(),
            format_brush: None,
        };
        if has_arg {
            app.switch_root(root, open);
            if !draft.is_empty() {
                app.docs.insert(SCRATCH_KEY.into(), Doc::scratch(draft));
                app.tabs.insert(0, SCRATCH_KEY.into());
            }
        } else {
            // 인자 없이 실행하면 메모장/Notepad++처럼 항상 새 문서(또는 저장 없이 유지된 임시 메모)로 시작
            app.root = root;
            app.open_scratch(draft);
            if let Some(last_file) = get("file").filter(|f| !f.is_empty()) {
                if app.root.join(&last_file).is_file() {
                    if let Ok(doc) = Doc::load(&app.root.join(&last_file)) {
                        app.docs.insert(last_file.clone(), doc);
                        if !app.tabs.contains(&last_file) {
                            app.tabs.push(last_file);
                        }
                    }
                }
            }
        }
        for body in drafts.into_iter().filter(|d| !d.is_empty()) {
            let key = app.next_scratch_key();
            app.docs.insert(key.clone(), Doc::scratch(body));
            app.tabs.push(key);
        }
        if app.status.starts_with("작업 폴더") {
            app.status.clear(); // 시작할 때는 알림을 띄우지 않는다
        }
        app
    }

    fn is_scratch(&self) -> bool {
        self.current.as_deref().is_some_and(is_scratch_key)
    }

    fn draft_text(&self) -> &str {
        if let Some(cur) = self.current.as_deref().filter(|k| is_scratch_key(k)) {
            if let Some(d) = self.docs.get(cur).filter(|d| !d.text.is_empty()) {
                return d.text.as_str();
            }
        }
        self.docs
            .get(SCRATCH_KEY)
            .filter(|d| !d.text.is_empty())
            .or_else(|| {
                self.tabs
                    .iter()
                    .filter(|k| is_scratch_key(k))
                    .filter_map(|k| self.docs.get(k))
                    .find(|d| !d.text.is_empty())
            })
            .map_or("", |d| d.text.as_str())
    }

    // 비어 있지 않은 모든 임시 메모 탭의 내용 (현재 탭이 맨 앞). 종료 후 복원용.
    fn drafts_joined(&self) -> String {
        let cur = self.current.as_deref().filter(|k| is_scratch_key(k));
        let mut keys: Vec<&String> = self.tabs.iter().filter(|k| is_scratch_key(k)).collect();
        keys.sort_by_key(|k| Some(k.as_str()) != cur); // 안정 정렬: 현재 탭만 앞으로
        keys.iter()
            .filter_map(|k| self.docs.get(*k))
            .filter(|d| !d.text.is_empty())
            .map(|d| d.text.as_str())
            .collect::<Vec<_>>()
            .join("\u{1}")
    }

    fn tab_title<'a>(&self, key: &'a str) -> std::borrow::Cow<'a, str> {
        if is_scratch_key(key) {
            return tab_base_name(key);
        }
        let base = tab_base_name(key);
        let dup = self
            .tabs
            .iter()
            .any(|k| k != key && !is_scratch_key(k) && tab_base_name(k) == base);
        if dup {
            std::borrow::Cow::Borrowed(key)
        } else {
            base
        }
    }

    fn next_scratch_key(&self) -> String {
        if !self.tabs.iter().any(|k| k == SCRATCH_KEY) && !self.docs.contains_key(SCRATCH_KEY) {
            return SCRATCH_KEY.into();
        }
        (2..)
            .map(|n| format!("{SCRATCH_PREFIX}{n}"))
            .find(|k| !self.tabs.contains(k) && !self.docs.contains_key(k))
            .expect("unbounded range")
    }

    fn save_current_tab_view(&mut self) {
        let Some(cur) = self.current.clone() else {
            return;
        };
        if let Some(doc) = self.docs.get_mut(&cur) {
            doc.view = TabView {
                active: self.active,
                blank: self.blank,
                block_sel: self.block_sel.clone(),
                block_sel_anchor: self.block_sel_anchor,
                block_sel_head: self.block_sel_head,
                last_sel: self.last_sel,
            };
        }
    }

    fn restore_tab_view(&mut self, name: &str) {
        self.drag_anchor = None;
        self.drag_last_pos = None;
        self.pending_sel = None;
        self.pending_byte_caret = None;
        self.pending_click_pos = None;
        self.cursor_info = None;
        self.active_char_cache = (usize::MAX, u64::MAX, 0, 0);
        self.active_line_cache = None;
        self.status_line_cache = None;
        self.editor_galley_cache = None;
        self.clear_live_spell();
        if let Some(doc) = self.docs.get(name) {
            if doc.text.is_empty() {
                self.active = Some(0);
                self.blank = 0;
                self.block_sel = None;
                self.block_sel_anchor = None;
                self.block_sel_head = None;
                self.last_sel = Some((0, 0));
                self.pending_sel = Some((0, 0));
                self.focus_editor = true;
            } else {
                let v = &doc.view;
                self.active = v
                    .active
                    .filter(|&a| a <= doc.text.len() && doc.text.is_char_boundary(a));
                self.blank = v.blank;
                self.block_sel = v
                    .block_sel
                    .as_ref()
                    .map(|r| clamp_char_range(&doc.text, r))
                    .filter(|r| r.start < r.end);
                self.block_sel_anchor = v.block_sel_anchor;
                self.block_sel_head = v.block_sel_head;
                self.last_sel = v.last_sel;
                if self.active.is_some() {
                    self.focus_editor = true;
                }
            }
        } else {
            self.active = None;
            self.blank = 0;
            self.block_sel = None;
            self.block_sel_anchor = None;
            self.block_sel_head = None;
            self.last_sel = None;
        }
    }

    fn switch_to_tab(&mut self, name: &str) {
        if self.current.as_deref() == Some(name) {
            return;
        }
        if !self.docs.contains_key(name) {
            return;
        }
        self.save_current_tab_view();
        if !self.tabs.iter().any(|k| k == name) {
            self.tabs.push(name.to_string());
        }
        self.current = Some(name.to_string());
        self.restore_tab_view(name);
        self.scroll_tab_to_active = true;
    }

    fn cycle_tab(&mut self, delta: isize) {
        if self.tabs.len() <= 1 {
            return;
        }
        let cur = self
            .current
            .as_deref()
            .and_then(|c| self.tabs.iter().position(|t| t == c))
            .unwrap_or(0);
        let n = self.tabs.len() as isize;
        let next = ((cur as isize + delta).rem_euclid(n)) as usize;
        let target = self.tabs[next].clone();
        self.switch_to_tab(&target);
    }

    fn new_tab(&mut self) {
        self.save_current_tab_view();
        let key = self.next_scratch_key();
        self.docs.insert(key.clone(), Doc::scratch(String::new()));
        self.tabs.push(key.clone());
        self.current = Some(key.clone());
        self.restore_tab_view(&key);
        self.scroll_tab_to_active = true;
    }

    fn close_tab(&mut self, name: &str) {
        let Some(idx) = self.tabs.iter().position(|t| t == name) else {
            return;
        };
        if is_scratch_key(name) {
            // 임시 메모는 닫으면 사라진다 → 내용이 있으면 묻는다
            if self
                .docs
                .get(name)
                .is_some_and(|d| !d.text.trim().is_empty())
            {
                self.dialog = Some(Dialog::Unsaved(After::CloseTab(name.to_string())));
                return;
            }
        } else if self.docs.get(name).is_some_and(Doc::dirty) {
            self.save_quiet(name);
            if self.docs.get(name).is_some_and(Doc::dirty) {
                if !matches!(self.dialog, Some(Dialog::Conflict(_))) {
                    self.dialog = Some(Dialog::Unsaved(After::CloseTab(name.to_string())));
                }
                return;
            }
        }
        self.force_close_tab(name, idx);
    }

    fn force_close_tab(&mut self, name: &str, idx: usize) {
        if is_scratch_key(name) {
            self.draft_dirty = true;
        }
        self.tabs.remove(idx);
        self.docs.remove(name);
        if self.current.as_deref() == Some(name) {
            self.active = None;
            self.blank = 0;
            self.block_sel = None;
            self.block_sel_anchor = None;
            self.block_sel_head = None;
            self.drag_anchor = None;
            self.drag_last_pos = None;
            self.last_sel = None;
            self.pending_sel = None;
            self.pending_byte_caret = None;
            self.pending_click_pos = None;
            self.cursor_info = None;
            self.editor_galley_cache = None;
            if self.tabs.is_empty() {
                self.current = None;
            } else {
                let next = self.tabs[idx.min(self.tabs.len() - 1)].clone();
                self.current = Some(next.clone());
                self.restore_tab_view(&next);
                self.scroll_tab_to_active = true;
            }
        }
    }

    fn close_other_tabs(&mut self, keep: &str) {
        let others: Vec<String> = self
            .tabs
            .iter()
            .filter(|t| t.as_str() != keep)
            .cloned()
            .collect();
        for t in others {
            self.close_tab(&t);
            if self.dialog.is_some() {
                return;
            }
        }
        self.switch_to_tab(keep);
    }

    fn close_tabs_to_right(&mut self, from: &str) {
        let Some(pos) = self.tabs.iter().position(|t| t == from) else {
            return;
        };
        let right: Vec<String> = self.tabs[pos + 1..].to_vec();
        for t in right {
            self.close_tab(&t);
            if self.dialog.is_some() {
                return;
            }
        }
    }

    fn duplicate_tab(&mut self, key: &str) {
        let Some(body) = self.docs.get(key).map(|d| d.text.clone()) else {
            return;
        };
        self.save_current_tab_view();
        let new_key = self.next_scratch_key();
        self.docs.insert(new_key.clone(), Doc::scratch(body));
        self.tabs.push(new_key.clone());
        self.current = Some(new_key.clone());
        self.restore_tab_view(&new_key);
        self.scroll_tab_to_active = true;
        self.status = "탭을 새 임시 문서로 복제했습니다".into();
    }

    fn open_scratch(&mut self, initial: String) {
        self.save_current_tab_view();
        self.docs.insert(SCRATCH_KEY.into(), Doc::scratch(initial));
        if !self.tabs.iter().any(|k| k == SCRATCH_KEY) {
            self.tabs.push(SCRATCH_KEY.into());
        }
        self.current = Some(SCRATCH_KEY.into());
        self.restore_tab_view(SCRATCH_KEY);
        self.scroll_tab_to_active = true;
    }

    fn doc(&self) -> Option<&Doc> {
        self.docs.get(self.current.as_ref()?)
    }

    fn doc_mut(&mut self) -> Option<&mut Doc> {
        let name = self.current.clone()?;
        self.docs.get_mut(&name)
    }

    fn switch_root(&mut self, root: PathBuf, open: Option<String>) {
        self.wait_auto_save();
        let keep_scratch = open.is_none()
            && self.current.as_deref().is_some_and(|k| {
                is_scratch_key(k) && self.docs.get(k).is_some_and(|d| !d.text.is_empty())
            });
        self.deactivate();
        self.root = root;
        self.docs.retain(|k, _| is_scratch_key(k));
        self.tabs.retain(|k| is_scratch_key(k));
        if !keep_scratch {
            self.current = self.tabs.first().cloned();
        }
        self.files.clear();
        self.block_heights.clear();
        self.status = format!("작업 폴더: {}", self.root.display());
        if keep_scratch {
            self.refresh_files();
            if let Some(cur) = self.current.clone() {
                self.restore_tab_view(&cur);
            }
            return;
        }
        // 열 파일이 정해져 있으면 폴더 스캔을 미룬다 (파일 탐색기를 열 때 스캔) → 큰 폴더에서도 바로 뜬다
        let mut first = open.filter(|n| self.root.join(n).is_file());
        if first.is_none() {
            self.scan_files_now();
            first = self.files.first().cloned();
        }
        if let Some(name) = first {
            self.open_file(&name);
        } else if let Some(cur) = self.current.clone() {
            self.restore_tab_view(&cur);
        }
    }

    // 폴더 전체 스캔 (시작 시 첫 파일을 정해야 할 때만 동기로)
    fn scan_files_now(&mut self) {
        self.scan_gen += 1; // 진행 중이던 백그라운드 스캔 결과는 버린다
        self.files = scan_markdown_files(&self.root);
    }

    // 파일 목록 갱신은 다른 스레드에서: 큰 폴더에서도 UI가 멈추지 않는다. 끝나면 poll_scan이 반영한다.
    fn refresh_files(&mut self) {
        self.scan_gen += 1;
        let (gen, root) = (self.scan_gen, self.root.clone());
        let (done, ctx) = (self.scan.clone(), self.ectx.clone());
        std::thread::spawn(move || {
            let names = scan_markdown_files(&root);
            if let Ok(mut r) = done.lock() {
                *r = Some((gen, names));
            }
            ctx.request_repaint();
        });
    }

    fn poll_scan(&mut self) {
        let done = self.scan.lock().ok().and_then(|mut r| r.take());
        if let Some((gen, names)) = done {
            if gen == self.scan_gen {
                self.files = names;
            }
        }
    }

    fn open_file(&mut self, name: &str) -> bool {
        if self.tabs.iter().any(|t| t == name) {
            self.switch_to_tab(name);
            return true;
        }
        if !self.docs.contains_key(name) {
            match Doc::load(&self.root.join(name)) {
                Ok(doc) => {
                    self.docs.insert(name.to_string(), doc);
                }
                Err(e) => {
                    self.status = format!("열기 실패: {name} — {e}");
                    return false;
                }
            }
        }
        self.save_current_tab_view();
        // 현재 탭이 비어 있고 아무 편집 이력이 없는 임시 메모라면 그 자리를 대체한다
        let replace_empty_scratch = self
            .current
            .as_deref()
            .filter(|k| is_scratch_key(k))
            .and_then(|k| self.docs.get(k).map(|d| (k.to_string(), d)))
            .is_some_and(|(_, d)| d.text.is_empty() && d.undo.is_empty() && d.redo.is_empty());
        if replace_empty_scratch {
            if let Some(old) = self.current.take() {
                if let Some(idx) = self.tabs.iter().position(|t| t == &old) {
                    self.tabs[idx] = name.to_string();
                } else {
                    self.tabs.push(name.to_string());
                }
                self.docs.remove(&old);
            }
        } else {
            self.tabs.push(name.to_string());
        }
        self.current = Some(name.to_string());
        self.restore_tab_view(name);
        self.scroll_tab_to_active = true;
        true
    }

    // Ctrl+S·저장 버튼: 결과를 알림으로 보여 준다
    fn save(&mut self, name: &str) -> bool {
        let ok = self.save_quiet(name);
        if ok {
            self.status = format!("저장됨: {name}");
        }
        ok
    }

    fn poll_auto_save(&mut self) {
        let results = self
            .auto_save_worker
            .as_ref()
            .map(AutoSaveWorker::poll)
            .unwrap_or_default();
        for res in results {
            self.auto_save_in_flight.remove(&res.name);
            if res.root != self.root {
                continue;
            }
            let Some(doc) = self.docs.get_mut(&res.name) else {
                continue;
            };
            if doc.scratch {
                continue;
            }
            match res.outcome {
                AutoSaveOutcome::Saved { mtime } => {
                    doc.saved = res.saved_text;
                    doc.mtime = mtime;
                    if doc.rev == res.rev {
                        doc.edited_at = None;
                        doc.dirty_flag = false;
                    } else {
                        doc.dirty_flag = doc.text.len() != doc.saved.len() || doc.text != doc.saved;
                    }
                }
                AutoSaveOutcome::Conflict => {
                    doc.edited_at = None;
                    self.status = format!("{}: 다른 프로그램이 파일을 바꿨습니다", res.name);
                    if self.dialog.is_none() {
                        self.dialog = Some(Dialog::Conflict(res.name));
                    }
                }
                AutoSaveOutcome::Err(e) => {
                    doc.edited_at = Some(Instant::now());
                    self.status = format!("저장 실패: {} — {e}", res.name);
                }
            }
        }
    }

    fn wait_auto_save(&mut self) {
        if !self.auto_save_in_flight.is_empty() {
            if let Some(w) = &self.auto_save_worker {
                w.wait_idle();
            }
        }
        self.poll_auto_save();
    }

    fn submit_auto_save(&mut self, ctx: &egui::Context, name: &str) {
        if is_scratch_key(name) || self.auto_save_in_flight.contains(name) {
            return;
        }
        let path = self.root.join(name);
        let Some(doc) = self.docs.get(name) else {
            return;
        };
        if doc.scratch || !doc.dirty() {
            return;
        }
        let req = AutoSaveReq {
            name: name.to_string(),
            root: self.root.clone(),
            path,
            encoded: doc.encoded(),
            saved_text: doc.text.clone(),
            prev_saved: doc.saved.clone(),
            prev_mtime: doc.mtime,
            rev: doc.rev,
        };
        let worker = self.auto_save_worker.get_or_insert_with(|| {
            let ctx = ctx.clone();
            AutoSaveWorker::new(move || ctx.request_repaint())
        });
        if worker.submit(req) {
            self.auto_save_in_flight.insert(name.to_string());
        } else {
            self.save_quiet(name);
        }
    }

    // 자동 저장·종료 시 저장: 성공은 조용히(상태 알약이 알려 준다), 실패만 알린다
    fn save_quiet(&mut self, name: &str) -> bool {
        if is_scratch_key(name) {
            return false;
        }
        self.wait_auto_save();
        let path = self.root.join(name);
        let Some(doc) = self.docs.get_mut(name) else {
            return false;
        };
        if doc.scratch {
            return false;
        }
        // 마지막으로 읽거나 쓴 뒤 다른 프로그램이 파일을 바꿨다면 덮어쓰지 않고 사용자에게 묻는다
        // (mtime만 바뀌고 내용이 같으면 클라우드 동기화 등이므로 그냥 저장)
        let external = doc.mtime.is_some()
            && path.is_file()
            && modified(&path).is_some_and(|d| Some(d) != doc.mtime)
            && Doc::load(&path).map_or(true, |f| f.text != doc.saved);
        if external {
            doc.edited_at = None; // 자동 저장 재시도 반복 방지 (변경 표시는 유지)
            self.status = format!("{name}: 다른 프로그램이 파일을 바꿨습니다");
            if self.dialog.is_none() {
                self.dialog = Some(Dialog::Conflict(name.to_string()));
            }
            return false;
        }
        match save_atomic(&path, doc.encoded().as_bytes()) {
            Ok(()) => {
                doc.saved.clear();
                doc.saved.push_str(&doc.text);
                doc.edited_at = None;
                doc.dirty_flag = false;
                doc.mtime = modified(&path);
                true
            }
            Err(e) => {
                doc.edited_at = Some(Instant::now()); // 자동 저장은 2초 뒤 다시 시도
                self.status = format!("저장 실패: {name} — {e}");
                false
            }
        }
    }

    // 창으로 돌아왔을 때: 다른 프로그램이 열린 문서를 바꿨으면 다시 읽는다 (편집 중이면 경고만, 저장 때 묻는다)
    fn sync_from_disk(&mut self) {
        self.poll_auto_save();
        let names: Vec<String> = self
            .docs
            .iter()
            .filter(|(n, d)| !d.scratch && !self.auto_save_in_flight.contains(*n))
            .map(|(n, _)| n.clone())
            .collect();
        for name in names {
            let path = self.root.join(&name);
            let (Some(disk), Some(doc)) = (modified(&path), self.docs.get_mut(&name)) else {
                continue;
            };
            if doc.mtime == Some(disk) {
                continue;
            }
            if doc.dirty() {
                self.status =
                    format!("{name}: 다른 프로그램이 파일을 바꿨습니다. 저장할 때 선택합니다");
            } else if let Ok(fresh) = Doc::load(&path) {
                if fresh.text == doc.text {
                    // 내용은 그대로이고 수정 시각만 바뀜(클라우드 동기화 등): 실행 취소 기록·커서를 유지한다
                    doc.mtime = fresh.mtime;
                    doc.crlf = fresh.crlf;
                    doc.bom = fresh.bom;
                    continue;
                }
                if self.current.as_deref() == Some(name.as_str()) {
                    self.deactivate();
                }
                self.status = format!("{name}: 바뀐 내용을 다시 불러왔습니다");
                self.docs.insert(name, fresh);
            }
        }
    }

    fn clear_live_spell(&mut self) {
        self.spell_block_cache = None;
        self.live_spell_pending = None;
        self.live_spell_submitted = None;
    }

    fn toggle_spellcheck(&mut self) {
        self.spellcheck = !self.spellcheck;
        self.spell_rev = self.spell_rev.wrapping_add(1);
        self.clear_live_spell();
        self.status = if self.spellcheck {
            "실시간 맞춤법 검사를 켰습니다".into()
        } else {
            "실시간 맞춤법 검사를 껐습니다".into()
        };
    }

    fn ignore_spell_word(&mut self, word: &str) {
        let key = word.trim();
        if key.is_empty() {
            return;
        }
        self.spell_ignore.insert(key.to_string());
        self.spell_rev = self.spell_rev.wrapping_add(1);
        self.clear_live_spell();
        self.status = format!("사전에 무시 단어 추가: '{key}'");
    }

    fn submit_live_spell(&mut self, ctx: &egui::Context, block: &str, bh: u64, block_start: usize) {
        let worker = self.live_spell_worker.get_or_insert_with(|| {
            let ctx_wake = ctx.clone();
            spell::Worker::new_block(move || ctx_wake.request_repaint())
        });
        self.live_spell_ticket += 1;
        let ticket = self.live_spell_ticket;
        worker.submit(ticket, block.to_string(), self.spell_ignore.clone());
        self.live_spell_submitted = Some((ticket, bh, self.spell_rev, block_start));
    }

    fn active_spell_issues(
        &mut self,
        ctx: &egui::Context,
        block: &str,
        block_start: usize,
        ime_composing: bool,
    ) -> Vec<spell::SpellIssue> {
        if !self.spellcheck || block.is_empty() {
            self.live_spell_pending = None;
            return Vec::new();
        }
        if let Some((ticket, issues)) = self.live_spell_worker.as_ref().and_then(|w| w.poll()) {
            if let Some((sub_t, sub_bh, sub_rev, sub_bs)) = self.live_spell_submitted {
                if ticket == sub_t {
                    self.live_spell_submitted = None;
                    self.spell_block_cache = Some((sub_bh, sub_rev, sub_bs, issues));
                }
            }
        }
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        block.hash(&mut h);
        let bh = h.finish();
        if let Some((cbh, crev, _, issues)) = &self.spell_block_cache {
            if *cbh == bh && *crev == self.spell_rev {
                self.live_spell_pending = None;
                return issues.clone();
            }
        }
        if !ime_composing {
            let already_submitted = self
                .live_spell_submitted
                .is_some_and(|(_, sub_bh, sub_rev, _)| sub_bh == bh && sub_rev == self.spell_rev);
            if !already_submitted {
                let is_new_focus = self
                    .spell_block_cache
                    .as_ref()
                    .is_none_or(|(_, _, bs, _)| *bs != block_start)
                    && self
                        .live_spell_submitted
                        .is_none_or(|(_, _, _, bs)| bs != block_start)
                    && self.live_spell_pending.is_none();
                if is_new_focus {
                    self.submit_live_spell(ctx, block, bh, block_start);
                } else {
                    match self.live_spell_pending {
                        Some((pbh, prev, since)) if pbh == bh && prev == self.spell_rev => {
                            let elapsed = since.elapsed();
                            if elapsed >= LIVE_SPELL_DEBOUNCE {
                                self.live_spell_pending = None;
                                self.submit_live_spell(ctx, block, bh, block_start);
                            } else {
                                ctx.request_repaint_after(LIVE_SPELL_DEBOUNCE - elapsed);
                            }
                        }
                        _ => {
                            self.live_spell_pending = Some((bh, self.spell_rev, Instant::now()));
                            ctx.request_repaint_after(LIVE_SPELL_DEBOUNCE);
                        }
                    }
                }
            }
        }
        if let Some((_, crev, cbs, issues)) = &self.spell_block_cache {
            if *crev == self.spell_rev && *cbs == block_start {
                return issues
                    .iter()
                    .filter(|i| block.get(i.range.clone()) == Some(i.word.as_str()))
                    .cloned()
                    .collect();
            }
        }
        Vec::new()
    }

    fn active_spell_issues_sync(
        &mut self,
        block: &str,
        block_start: usize,
    ) -> Vec<spell::SpellIssue> {
        if !self.spellcheck || block.is_empty() {
            return Vec::new();
        }
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        block.hash(&mut h);
        let bh = h.finish();
        if let Some((cbh, crev, _, issues)) = &self.spell_block_cache {
            if *cbh == bh && *crev == self.spell_rev {
                return issues.clone();
            }
        }
        let issues = spell::check_block(block, &self.spell_ignore);
        self.spell_block_cache = Some((bh, self.spell_rev, block_start, issues.clone()));
        self.live_spell_pending = None;
        issues
    }

    fn open_spellcheck(&mut self) {
        let Some(doc) = self.doc() else {
            self.status = "검사할 문서가 없습니다".into();
            return;
        };
        let _ = doc;
        // 첫 프레임에 작업 스레드로 검사를 보낸다 (cached_rev가 맞지 않으므로)
        self.dialog = Some(Dialog::Spellcheck {
            selected: 0,
            cached_key: self.current.clone(),
            cached_rev: u64::MAX,
            cached_spell_rev: self.spell_rev,
            issues: Vec::new(),
            checking: true,
        });
    }

    fn apply_spell_fix(&mut self, range: Range<usize>, replacement: &str) {
        let Some(doc) = self.doc_mut() else {
            return;
        };
        if range.start > range.end
            || range.end > doc.text.len()
            || !doc.text.is_char_boundary(range.start)
            || !doc.text.is_char_boundary(range.end)
        {
            return;
        }
        // 중복 단어 삭제(replacement == "")일 때는 바로 앞 공백 1칸도 함께 정리한다
        let (rep_start, rep_end) = if replacement.is_empty()
            && range.start > 0
            && doc.text.as_bytes()[range.start - 1] == b' '
        {
            (range.start - 1, range.end)
        } else {
            (range.start, range.end)
        };
        let mut next = doc.text.clone();
        next.replace_range(rep_start..rep_end, replacement);
        let caret = char_len(&next[..rep_start]) + char_len(replacement);
        self.set_text(next, (caret, caret));
        self.clear_live_spell();
    }

    // issues: 지금 문서 텍스트로 검사한 결과
    fn apply_all_spell_fixes(&mut self, issues: Vec<spell::SpellIssue>) -> usize {
        let Some(doc) = self.doc() else {
            return 0;
        };
        let fixes: Vec<(Range<usize>, String)> = issues
            .into_iter()
            .filter_map(|mut i| {
                spell::fill_suggestions(&mut i);
                let sug = i.suggestions.into_iter().next()?;
                Some((i.range, sug))
            })
            .collect();
        if fixes.is_empty() {
            return 0;
        }
        let count = fixes.len();
        let mut next = doc.text.clone();
        // 뒤에서부터 적용해야 앞쪽 바이트 오프셋이 밀리지 않는다
        let mut last_rs = usize::MAX;
        for (range, rep) in fixes.into_iter().rev() {
            if range.end > last_rs || range.end > next.len() {
                continue;
            }
            let (rs, re) = if rep.is_empty()
                && range.start > 0
                && range.start <= next.len()
                && next.as_bytes()[range.start - 1] == b' '
            {
                (range.start - 1, range.end)
            } else {
                (range.start, range.end)
            };
            if rs <= re
                && re <= next.len()
                && next.is_char_boundary(rs)
                && next.is_char_boundary(re)
            {
                next.replace_range(rs..re, &rep);
                last_rs = rs;
            }
        }
        self.set_text(next, (0, 0));
        self.clear_live_spell();
        self.status = format!("맞춤법 {count}건을 교정했습니다");
        count
    }

    fn current_line_number(&mut self) -> Option<usize> {
        let name = self.current.as_ref()?;
        let (_, caret) = self.last_sel?;
        let doc = self.docs.get(name)?;
        if let Some((k, rev, c, ln)) = &self.status_line_cache {
            if k == name && *rev == doc.rev && *c == caret {
                return Some(*ln);
            }
        }
        if let Some(r) = self.active_block() {
            if self.active_char_cache.0 == r.start && self.active_char_cache.1 == doc.rev {
                let base = self.active_char_cache.2;
                let blen = self.active_char_cache.3;
                if caret >= base && caret <= base + blen {
                    let lines_before = self
                        .active_line_cache
                        .as_ref()
                        .filter(|(k, bs, rv, _)| k == name && *bs == r.start && *rv == doc.rev)
                        .map(|x| x.3)
                        .unwrap_or_else(|| {
                            let lb = doc.text[..r.start].bytes().filter(|&x| x == b'\n').count();
                            self.active_line_cache = Some((name.clone(), r.start, doc.rev, lb));
                            lb
                        });
                    let slice = &doc.text[r];
                    let local_b = byte_idx(slice, caret - base);
                    let ln =
                        1 + lines_before + slice[..local_b].bytes().filter(|&x| x == b'\n').count();
                    self.status_line_cache = Some((name.clone(), doc.rev, caret, ln));
                    return Some(ln);
                }
            }
        }
        let ln = 1 + doc
            .text
            .chars()
            .take(caret.min(doc.chars))
            .filter(|&c| c == '\n')
            .count();
        self.status_line_cache = Some((name.clone(), doc.rev, caret, ln));
        Some(ln)
    }

    // 문서 단위 실행 취소 / 다시 실행 (블록 나누기·합치기·서식·모두 바꾸기 포함)
    fn history(&mut self, undo: bool) {
        self.leave_blank();
        let Some(doc) = self.doc_mut() else {
            return;
        };
        let Some(caret) = doc.step_history(undo) else {
            self.status = if undo {
                "더 되돌릴 내용이 없습니다".into()
            } else {
                "다시 실행할 내용이 없습니다".into()
            };
            return;
        };
        // 커서는 바뀐 곳에. 블록 사이에 떨어지면 바로 앞 블록 끝으로 옮긴다.
        let (start, caret) = doc
            .blocks
            .iter()
            .find(|b| b.start <= caret && caret <= b.end)
            .map(|b| (b.start, caret))
            .or_else(|| {
                let b = doc.blocks.iter().rev().find(|b| b.end <= caret)?;
                Some((b.start, b.end))
            })
            .unwrap_or((0, 0));
        self.blank = 0;
        self.block_sel = None;
        self.block_sel_anchor = None;
        self.block_sel_head = None;
        self.active = Some(start);
        self.pending_sel = None;
        self.pending_byte_caret = Some((start, caret));
        self.focus_editor = true;
        self.scroll_cursor = Some(None);
    }

    fn save_all(&mut self) -> bool {
        self.wait_auto_save();
        let dirty: Vec<String> = self
            .docs
            .iter()
            .filter(|(_, d)| !d.scratch && d.dirty())
            .map(|(n, _)| n.clone())
            .collect();
        dirty.iter().filter(|n| !self.save_quiet(n)).count() == 0 // 실패해도 나머지는 계속 저장
    }

    fn any_dirty_incl_scratch(&self) -> bool {
        self.docs.values().any(Doc::dirty)
    }

    fn any_dirty(&self) -> bool {
        self.docs.values().any(|d| !d.scratch && d.dirty())
    }

    fn save_scratch_as(&mut self, input: &str) -> Result<(), String> {
        let scratch_key = self
            .current
            .clone()
            .filter(|k| is_scratch_key(k))
            .unwrap_or_else(|| SCRATCH_KEY.into());
        let name = validate_name(input)?;
        let path = self.root.join(&name);
        if path.exists() {
            return Err(format!("이미 있는 파일입니다: {name}"));
        }
        let body = self
            .docs
            .get(&scratch_key)
            .map(|d| d.encoded())
            .unwrap_or_default();
        save_atomic(&path, body.as_bytes()).map_err(|e| format!("저장 실패: {e}"))?;
        let mut doc = self
            .docs
            .remove(&scratch_key)
            .unwrap_or_else(|| Doc::scratch(String::new()));
        doc.scratch = false;
        doc.saved = doc.text.clone();
        doc.edited_at = None;
        doc.dirty_flag = false;
        doc.mtime = modified(&path);
        self.docs.insert(name.clone(), doc);
        if let Some(slot) = self.tabs.iter_mut().find(|t| **t == scratch_key) {
            *slot = name.clone();
        } else if !self.tabs.iter().any(|t| t == &name) {
            self.tabs.push(name.clone());
        }
        self.current = Some(name.clone());
        self.draft_dirty = true;
        self.refresh_files();
        self.status = format!("저장됨: {name}");
        Ok(())
    }

    fn trigger_save(&mut self) {
        if self.is_scratch() {
            self.dialog = Some(Dialog::SaveScratch(String::new()));
        } else if let Some(n) = self.current.clone() {
            self.save(&n);
        }
    }

    fn save_as_path(&mut self, dest: &Path) -> Result<String, String> {
        self.wait_auto_save();
        let Some(cur_key) = self.current.clone() else {
            return Err("열린 문서가 없습니다".into());
        };
        let dest = if dest.extension().is_none() {
            dest.with_extension("md")
        } else {
            dest.to_path_buf()
        };
        let parent_dir = match dest.parent().filter(|p| !p.as_os_str().is_empty()) {
            Some(p) => {
                std::fs::create_dir_all(p).map_err(|e| format!("폴더 생성 실패: {e}"))?;
                canon(p).unwrap_or_else(|| p.to_path_buf())
            }
            None => self.root.clone(),
        };
        let file_name = dest
            .file_name()
            .ok_or_else(|| "파일 이름이 올바르지 않습니다".to_string())?;
        let dest_canon = parent_dir.join(file_name);
        // 작업 폴더 밖으로 저장하면 폴더가 바뀌며 다른 파일 탭이 모두 닫힌다 → 그 탭들을 먼저 저장하고, 못 하면 멈춘다
        if dest_canon.strip_prefix(&self.root).is_err() {
            let others: Vec<String> = self
                .docs
                .iter()
                .filter(|(k, d)| **k != cur_key && !d.scratch && d.dirty())
                .map(|(k, _)| k.clone())
                .collect();
            let failed: Vec<String> = others.into_iter().filter(|n| !self.save_quiet(n)).collect();
            if !failed.is_empty() {
                return Err(format!(
                    "다른 탭을 저장하지 못해 다른 폴더로 옮기지 않았습니다: {}",
                    failed.join(", ")
                ));
            }
        }
        let body = self
            .docs
            .get(&cur_key)
            .map(Doc::encoded)
            .unwrap_or_default();
        save_atomic(&dest_canon, body.as_bytes()).map_err(|e| format!("저장 실패: {e}"))?;
        self.save_current_tab_view();
        let mut doc = self
            .docs
            .remove(&cur_key)
            .unwrap_or_else(|| Doc::scratch(String::new()));
        let was_scratch = is_scratch_key(&cur_key) || doc.scratch;
        doc.scratch = false;
        doc.saved = doc.text.clone();
        doc.edited_at = None;
        doc.dirty_flag = false;
        doc.mtime = modified(&dest_canon);

        let rel = if let Some(r) = dest_canon.strip_prefix(&self.root).ok().map(to_slash) {
            if let Some(pos) = self.tabs.iter().position(|t| *t == cur_key) {
                if self.tabs.contains(&r) && self.tabs[pos] != r {
                    self.tabs.remove(pos);
                } else {
                    self.tabs[pos] = r.clone();
                }
            } else if !self.tabs.contains(&r) {
                self.tabs.push(r.clone());
            }
            self.docs.insert(r.clone(), doc);
            r
        } else {
            self.deactivate();
            self.root = parent_dir;
            self.docs.retain(|k, _| is_scratch_key(k));
            self.tabs.retain(|k| is_scratch_key(k));
            self.block_heights.clear();
            let r = to_slash(Path::new(file_name));
            if was_scratch {
                if let Some(slot) = self.tabs.iter_mut().find(|t| **t == cur_key) {
                    *slot = r.clone();
                } else if !self.tabs.contains(&r) {
                    self.tabs.push(r.clone());
                }
            } else if !self.tabs.contains(&r) {
                self.tabs.push(r.clone());
            }
            self.docs.insert(r.clone(), doc);
            r
        };
        self.current = Some(rel.clone());
        if was_scratch {
            self.draft_dirty = true;
        }
        self.refresh_files();
        self.status = format!("저장됨: {}", dest_canon.display());
        Ok(rel)
    }

    fn save_as_dialog(&mut self) -> bool {
        let Some(cur_key) = self.current.clone() else {
            return false;
        };
        let default_name = if is_scratch_key(&cur_key) {
            "새문서.md".to_string()
        } else {
            Path::new(&cur_key)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "새문서.md".to_string())
        };
        let Some(path) = rfd::FileDialog::new()
            .set_directory(self.doc_dir())
            .set_file_name(&default_name)
            .add_filter("Markdown", &["md", "markdown"])
            .save_file()
        else {
            return false;
        };
        match self.save_as_path(&path) {
            Ok(_) => true,
            Err(e) => {
                self.status = e;
                false
            }
        }
    }

    fn pick_save_folder(&mut self) {
        if let Some(dir) = rfd::FileDialog::new()
            .set_directory(&self.root)
            .pick_folder()
        {
            let (new_root, _) = resolve_target(Some(&dir), &self.root);
            self.save_all();
            if !self.any_dirty() {
                self.root = new_root;
                self.docs.retain(|k, _| is_scratch_key(k));
                self.tabs.retain(|k| is_scratch_key(k));
                self.block_heights.clear();
                self.refresh_files();
                self.status = format!("저장 폴더: {}", self.root.display());
            } else if !matches!(self.dialog, Some(Dialog::Conflict(_))) {
                self.dialog = Some(Dialog::Unsaved(After::Switch(new_root, None)));
            }
        }
    }

    fn create_file(&mut self, input: &str) -> Result<(), String> {
        let name = validate_name(input)?;
        let path = self.root.join(&name);
        if path.exists() {
            return Err(format!("이미 있는 파일입니다: {name}"));
        }
        save_atomic(&path, b"").map_err(|e| format!("만들기 실패: {e}"))?;
        self.refresh_files();
        self.open_file(&name);
        self.append_block();
        self.status = format!("새 문서: {name}");
        Ok(())
    }

    fn rename(&mut self, from: &str, input: &str) -> Result<(), String> {
        self.wait_auto_save();
        let to = validate_name(input)?;
        if to == from {
            return Ok(());
        }
        let dst = self.root.join(&to);
        // 대소문자만 바꾸는 경우(a.md → A.md): 대소문자 무시 파일 시스템에서는 dst가 원본 자신이다
        if dst.exists() && canon(&dst) != canon(&self.root.join(from)) {
            return Err(format!("이미 있는 파일입니다: {to}"));
        }
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::rename(self.root.join(from), &dst).map_err(|e| format!("이름 변경 실패: {e}"))?;
        if let Some(doc) = self.docs.remove(from) {
            self.docs.insert(to.clone(), doc);
        }
        if let Some(slot) = self.tabs.iter_mut().find(|t| **t == from) {
            *slot = to.clone();
        }
        if self.current.as_deref() == Some(from) {
            self.current = Some(to.clone());
        }
        self.refresh_files();
        self.status = format!("이름 변경: {from} → {to}");
        Ok(())
    }

    fn delete(&mut self, name: &str) {
        if is_scratch_key(name) {
            return;
        }
        self.wait_auto_save();
        match std::fs::remove_file(self.root.join(name)) {
            Ok(()) => {
                if let Some(idx) = self.tabs.iter().position(|t| t == name) {
                    self.force_close_tab(name, idx);
                } else {
                    self.docs.remove(name);
                }
                self.refresh_files();
                self.status = format!("삭제됨: {name}");
            }
            Err(e) => self.status = format!("삭제 실패: {e}"),
        }
    }

    // 저장 안 된 문서가 있으면 (자동 저장 시도 후) 확인 대화상자를 거친다
    fn guarded(&mut self, after: After) {
        self.save_all();
        if self.any_dirty() {
            if !matches!(self.dialog, Some(Dialog::Conflict(_))) {
                self.dialog = Some(Dialog::Unsaved(after));
            }
        } else {
            self.finish(after);
        }
    }

    fn finish(&mut self, after: After) {
        match after {
            After::Close => self.allow_close = true,
            After::CloseTab(name) => {
                if let Some(idx) = self.tabs.iter().position(|t| t == &name) {
                    self.force_close_tab(&name, idx);
                }
            }
            After::Switch(root, open) => self.switch_root(root, open),
        }
    }

    fn pick_folder(&mut self) {
        if let Some(dir) = rfd::FileDialog::new()
            .set_directory(&self.root)
            .pick_folder()
        {
            let (root, _) = resolve_target(Some(&dir), &self.root);
            self.guarded(After::Switch(root, None));
        }
    }

    // OS 파일 대화상자로 .md 열기: 작업 폴더 밖이면 그 폴더로 전환
    fn open_local_file(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_directory(self.doc_dir())
            .add_filter("Markdown", &["md", "markdown"])
            .pick_file()
        else {
            return;
        };
        let rel = canon(&path).and_then(|p| p.strip_prefix(&self.root).ok().map(to_slash));
        match rel {
            Some(r) => {
                self.open_file(&r);
            }
            None => {
                let (root, open) = resolve_target(Some(&path), &self.root);
                self.guarded(After::Switch(root, open));
            }
        }
    }

    fn open_files(&mut self) {
        self.refresh_files();
        let sel = self
            .current
            .as_ref()
            .and_then(|c| self.files.iter().position(|f| f == c))
            .unwrap_or(0);
        self.dialog = Some(Dialog::Files {
            filter: String::new(),
            sel,
        });
    }

    fn doc_dir(&self) -> PathBuf {
        match self.current.as_deref() {
            Some(cur) if !is_scratch_key(cur) => self
                .root
                .join(cur)
                .parent()
                .map_or_else(|| self.root.clone(), Path::to_path_buf),
            _ => self.root.clone(),
        }
    }

    fn editor_id(name: &str, start: usize) -> egui::Id {
        egui::Id::unique(("editor", name, start))
    }

    fn ask_rename(&mut self, name: &str) {
        self.dialog = Some(Dialog::Rename {
            from: name.to_string(),
            to: name.to_string(),
        });
    }

    // ----- 블록 편집 상태 -----

    // 활성 블록의 바이트 구간 (새로 만든 빈 블록이면 a..a) — O(log B) 이진 탐색
    fn active_block(&self) -> Option<Range<usize>> {
        let a = self.active?;
        let doc = self.doc()?;
        let text = &doc.text;
        if a > text.len() || !text.is_char_boundary(a) {
            return None;
        }
        let idx = doc.blocks.partition_point(|b| b.start <= a);
        Some(
            idx.checked_sub(1)
                .and_then(|i| doc.blocks.get(i))
                .filter(|b| a <= b.end)
                .cloned()
                .unwrap_or(a..a),
        )
    }

    // 상태바 표시용: 매 프레임 전체 문서 char_len 스캔 없이 선택된 글자 수만 O(1) 계산
    fn selected_chars(&self) -> usize {
        if let Some(bs) = &self.block_sel {
            let Some(doc) = self.doc() else {
                return 0;
            };
            let r = clamp_char_range(&doc.text, bs);
            return if r.end > r.start {
                char_len(&doc.text[r])
            } else {
                0
            };
        }
        if self.active.is_some() {
            if let Some((a, b)) = self.last_sel {
                return b.saturating_sub(a);
            }
        }
        0
    }

    fn clear_all_galley_caches(&mut self) {
        self.editor_galley_cache = None;
        self.code_galley_cache.clear();
        self.inline_galley_cache.clear();
        self.hanging_line_cache.1.clear();
        self.block_heights.clear();
        self.md_cache = CommonMarkCache::default();
    }

    fn apply_font(&mut self, ctx: &egui::Context, font: String) {
        if self.font_family == font {
            return;
        }
        theme::install_fonts_with(ctx, &font);
        self.font_family = font;
        // egui::Context::set_fonts takes effect in begin_pass of the NEXT frame.
        // Defer clearing cached Galleys until the top of App::ui on the next frame
        // so canvas in the current frame does not repopulate caches with old-atlas UVs.
        self.clear_galleys_next_frame = true;
        ctx.request_repaint();
    }

    fn is_visual_table_active(&self) -> bool {
        if self.raw_table_mode {
            return false;
        }
        let (Some(r), Some(doc)) = (self.active_block(), self.doc()) else {
            return false;
        };
        !r.is_empty() && parse_md_table(&doc.text[r]).is_some()
    }

    // 붙여넣은 글을 마크다운으로 정리(normalize_pasted_markdown)할지: 캔버스의 일반 블록에 넣을 때만.
    // 코드·수식 블록과 표 셀, 대화상자에는 원문 그대로 넣는다 (탭 → 공백 변환이 Makefile·Go 코드를 망가뜨린다)
    fn paste_normalizes(&self) -> bool {
        if self.dialog.is_some() || self.is_visual_table_active() {
            return false;
        }
        match (self.active_block(), self.doc()) {
            (Some(r), Some(doc)) => {
                let b = &doc.text[r];
                code_body_range(b).is_none() && !is_math_block(b)
            }
            _ => self.block_sel.is_some(),
        }
    }

    fn editor_focused(&self, ctx: &egui::Context) -> bool {
        if self.is_visual_table_active() {
            return !self.find.open
                && self.dialog.is_none()
                && self.context_menu.is_none()
                && ctx.memory(|m| m.focused().is_some());
        }
        match (&self.current, self.active_block()) {
            (Some(name), Some(r)) => ctx.memory(|m| m.has_focus(Self::editor_id(name, r.start))),
            _ => false,
        }
    }

    fn block_char_bounds(&self, r: &Range<usize>, doc: &Doc) -> (usize, usize) {
        if self.active_char_cache.0 == r.start && self.active_char_cache.1 == doc.rev {
            (self.active_char_cache.2, self.active_char_cache.3)
        } else {
            (
                char_len(&doc.text[..r.start]),
                char_len(&doc.text[r.clone()]),
            )
        }
    }

    fn block_byte_of(&self, r: &Range<usize>, doc: &Doc, char_pos: usize) -> usize {
        let (base, _) = self.block_char_bounds(r, doc);
        if char_pos >= base {
            (r.start + byte_idx(&doc.text[r.clone()], char_pos - base)).min(r.end)
        } else {
            r.start
        }
    }

    // 현재 활성 블록 내 로컬 문자 선택 범위 조회 (전체 문서 UTF-8 스캔 없음)
    fn local_cursor(&self, ctx: &egui::Context, doc: &Doc, r: &Range<usize>) -> Sel {
        let Some(name) = self.current.as_deref() else {
            return (0, 0);
        };
        if let Some((ps, caret)) = self.pending_byte_caret.filter(|&(ps, _)| ps == r.start) {
            let c = char_len(&doc.text[ps..caret.clamp(ps, r.end)]);
            return (c, c);
        }
        if let Some(cr) = egui::TextEdit::load_state(ctx, Self::editor_id(name, r.start))
            .and_then(|s| s.cursor.char_range())
        {
            let cr = cr.as_sorted_char_range();
            return (cr.start.0, cr.end.0);
        }
        let blen = char_len(&doc.text[r.clone()]);
        if let Some(sel) = self.last_sel {
            if self.active_char_cache.0 == r.start && self.active_char_cache.1 == doc.rev {
                let base = self.active_char_cache.2;
                if sel.0 >= base && sel.1 <= base + blen {
                    return (sel.0 - base, sel.1 - base);
                }
            }
        }
        (blen, blen)
    }

    // 현재 선택 영역 (문서 전체 기준 문자 인덱스)
    fn cursor(&self, ctx: &egui::Context) -> Sel {
        let (Some(name), Some(doc)) = (&self.current, self.doc()) else {
            return (0, 0);
        };
        if let Some(bs) = &self.block_sel {
            let r = clamp_char_range(&doc.text, bs);
            return (char_len(&doc.text[..r.start]), char_len(&doc.text[..r.end]));
        }
        let len = doc.chars;
        let Some(r) = self.active_block() else {
            return (len, len);
        };
        let (base, blen) = self.block_char_bounds(&r, doc);
        if self.context_menu.is_some() {
            if let Some(sel) = self
                .last_sel
                .filter(|&(la, lb)| la >= base && lb <= base + blen)
            {
                return sel;
            }
        }
        egui::TextEdit::load_state(ctx, Self::editor_id(name, r.start))
            .and_then(|s| s.cursor.char_range())
            .map(|c| {
                let c = c.as_sorted_char_range();
                (base + c.start.0.min(blen), base + c.end.0.min(blen))
            })
            .or_else(|| {
                self.last_sel
                    .filter(|&(la, lb)| la >= base && lb <= base + blen)
            })
            .unwrap_or((base + blen, base + blen))
    }

    // 새 빈 블록을 비운 채 떠나면 만들 때 넣은 줄바꿈을 되돌린다. 지운 위치를 돌려준다.
    fn leave_blank(&mut self) -> Option<(usize, usize)> {
        let n = std::mem::take(&mut self.blank);
        let a = self.active?;
        let doc = self.doc_mut()?;
        let t = &doc.text;
        if n == 0 || a > t.len() || !t.is_char_boundary(a) {
            return None;
        }
        if a >= n
            && t[..a].ends_with(&"\n".repeat(n))
            && t[a..].chars().next().is_none_or(|c| c == '\n')
        {
            doc.text.replace_range(a - n..a, "");
            doc.refresh_meta();
            return Some((a - n, n));
        }
        if a == 0 && t.starts_with(&"\n".repeat(n)) {
            doc.text.replace_range(0..n, "");
            doc.refresh_meta();
            return Some((0, n));
        }
        None
    }

    fn deactivate(&mut self) {
        self.leave_blank();
        self.active = None;
        self.block_sel = None;
        self.block_sel_anchor = None;
        self.block_sel_head = None;
        self.drag_anchor = None;
        self.drag_last_pos = None;
        self.drag_origin_rel = None;
        self.drag_anchor_block = None;
        self.last_sel = None;
        self.pending_sel = None;
        self.pending_byte_caret = None;
        self.pending_click_pos = None;
        self.ime_composing = false;
        self.pending_enter = false;
        self.raw_table_mode = false;
        self.table_focus_cell = None;
        self.cursor_info = None;
    }

    // 블록으로 이동 (위치는 이동 전 기준 바이트)
    fn go(&mut self, start: usize, caret: usize) {
        let removed = self.leave_blank();
        let fix = |p: usize| match removed {
            Some((r, n)) if p >= r + n => p - n,
            _ => p,
        };
        let (start, caret) = (fix(start), fix(caret));
        self.block_sel = None;
        self.block_sel_anchor = None;
        self.block_sel_head = None;
        self.active = Some(start);
        self.pending_sel = None;
        self.pending_byte_caret = Some((start, caret));
        self.pending_click_pos = None;
        self.ime_composing = false;
        self.pending_enter = false;
        self.raw_table_mode = false;
        self.table_focus_cell = Some((0, 0));
        self.focus_editor = true;
        self.scroll_cursor = Some(None);
    }

    // 비활성 블록 클릭 시 커서를 블록 끝에 먼저 두지 않고 첫 프레임에 클릭 좌표로 바로 배치한다
    fn go_click(&mut self, start: usize, mut pos: egui::Pos2) {
        let removed = self.leave_blank();
        let start = match removed {
            Some((r, n)) if start >= r + n => {
                let body = if self.small_text { 14.5 } else { 16.0 };
                pos.y -= body * 1.6 + 4.0;
                start - n
            }
            _ => start,
        };
        self.block_sel = None;
        self.block_sel_anchor = None;
        self.block_sel_head = None;
        self.active = Some(start);
        self.pending_sel = None;
        self.pending_byte_caret = None;
        self.pending_click_pos = Some((start, pos));
        self.ime_composing = false;
        self.pending_enter = false;
        self.raw_table_mode = false;
        self.table_focus_cell = Some((0, 0));
        self.focus_editor = true;
    }

    fn doc_slots(&self) -> Vec<Range<usize>> {
        let Some(doc) = self.doc() else {
            return Vec::new();
        };
        let active_empty = self
            .active_block()
            .filter(|r| r.is_empty())
            .map(|r| r.start);
        let mut out = Vec::new();
        collect_doc_slots_into(&doc.text, &doc.blocks, active_empty, &mut out);
        out
    }

    // 방향키 이동용: 접힌 제목 아래 구간을 건너뛴다.
    // 합치기·지우기는 doc_slots를 써야 한다 (보이는 이웃과 합치면 그 사이 숨은 내용이 지워진다)
    fn nav_slots(&self) -> Vec<Range<usize>> {
        let mut out = self.doc_slots();
        if let Some(doc) = self.doc() {
            hide_folded(&doc.text, &mut out, &doc.folds, |_| false);
        }
        out
    }

    // 다중 블록 / 전체 문서 선택 구간 삭제
    fn delete_block_sel(&mut self) -> bool {
        let Some(sel) = self.block_sel.take() else {
            return false;
        };
        self.block_sel_anchor = None;
        self.block_sel_head = None;
        self.leave_blank();
        let Some(doc) = self.doc_mut() else {
            return false;
        };
        let len = doc.text.len();
        let clamped = clamp_char_range(&doc.text, &sel);
        let (start, end) = (clamped.start, clamped.end);
        if start >= end && len > 0 {
            return false;
        }
        let pre_nl = doc.text[..start]
            .bytes()
            .rev()
            .take_while(|&b| b == b'\n')
            .count();
        let post_nl = doc.text[end..].bytes().take_while(|&b| b == b'\n').count();
        let (del_start, del_end, ins, caret_off) = if start == 0 && end + post_nl == len {
            (0, len, "", 0)
        } else if start == 0 {
            (0, (end + post_nl).min(len), "", 0)
        } else if end + post_nl == len {
            let rm = pre_nl.min(2);
            (start - rm, end, "", 0)
        } else {
            let rm_pre = pre_nl.min(2);
            let rm_post = post_nl.min(2);
            (start - rm_pre, end + rm_post, "\n\n", 2)
        };
        doc.text.replace_range(del_start..del_end, ins);
        doc.edited_at = Some(Instant::now());
        doc.refresh_meta();
        if doc.text.is_empty() {
            self.active = Some(0);
            self.blank = 0;
            self.pending_sel = None;
            self.pending_byte_caret = Some((0, 0));
            self.focus_editor = true;
        } else {
            let caret = (del_start + caret_off).min(doc.text.len());
            let block_start = doc
                .blocks
                .iter()
                .find(|b| b.start <= caret && caret <= b.end)
                .or_else(|| doc.blocks.iter().rev().find(|b| b.end <= caret))
                .or_else(|| doc.blocks.first())
                .map_or(0, |b| b.start);
            self.active = Some(block_start);
            self.blank = 0;
            self.pending_sel = None;
            self.pending_byte_caret = Some((block_start, caret));
            self.focus_editor = true;
        }
        true
    }

    // 다중 블록 / 전체 문서 선택 구간을 새 문자열로 대체
    fn replace_block_sel(&mut self, with: &str) {
        let Some(sel) = self.block_sel.take() else {
            return;
        };
        self.block_sel_anchor = None;
        self.block_sel_head = None;
        self.leave_blank();
        let Some(doc) = self.doc_mut() else {
            return;
        };
        let clamped = clamp_char_range(&doc.text, &sel);
        let (start, end) = (clamped.start, clamped.end);
        doc.text.replace_range(start..end, with);
        doc.edited_at = Some(Instant::now());
        doc.refresh_meta();
        let caret = start + with.len();
        let block_start = doc
            .blocks
            .iter()
            .find(|b| b.start <= caret && caret <= b.end)
            .map_or(start, |b| b.start);
        self.active = Some(block_start);
        self.blank = 0;
        self.pending_sel = None;
        self.pending_byte_caret = Some((block_start, caret));
        self.focus_editor = true;
    }

    // 문서 끝에 새 빈 블록을 만들고 편집 시작
    fn append_block(&mut self) {
        self.deactivate();
        let Some(doc) = self.doc_mut() else {
            return;
        };
        let t = &doc.text;
        let n = if t.is_empty() || t.ends_with("\n\n") {
            0
        } else if t.ends_with('\n') {
            1
        } else {
            2
        };
        doc.text.push_str(&"\n".repeat(n));
        doc.refresh_meta();
        let a = doc.text.len();
        self.active = Some(a);
        self.blank = n;
        self.pending_sel = None;
        self.pending_byte_caret = Some((a, a));
        self.focus_editor = true;
        self.scroll_cursor = Some(None);
    }

    // b 위치에서 블록을 둘로 나누고 뒤 블록을 편집 (노션의 Enter)
    fn split(&mut self, b: usize) {
        let Some(doc) = self.doc_mut() else {
            return;
        };
        let rest_empty = doc.text[b..].chars().next().is_none_or(|c| c == '\n');
        doc.text.insert_str(b, "\n\n");
        doc.edited_at = Some(Instant::now());
        doc.refresh_meta();
        let a = b + 2;
        self.active = Some(a);
        self.blank = if rest_empty { 2 } else { 0 };
        self.pending_sel = None;
        self.pending_byte_caret = Some((a, a));
        self.focus_editor = true;
        self.scroll_cursor = Some(None);
    }

    fn set_text(&mut self, text: String, sel: Sel) {
        let was_block_sel = self.block_sel.is_some();
        let next_bs = if let Some(doc) = self.doc_mut() {
            doc.text = text;
            doc.edited_at = Some(Instant::now());
            doc.refresh_meta();
            if was_block_sel {
                let b0 = byte_idx(&doc.text, sel.0);
                let b1 = byte_idx(&doc.text, sel.1).max(b0);
                let anchor_block = doc
                    .blocks
                    .iter()
                    .find(|b| b.end > b0 && b.start < b1)
                    .map_or(b0, |b| b.start);
                let head_block = doc
                    .blocks
                    .iter()
                    .rfind(|b| b.end > b0 && b.start < b1)
                    .map_or(b1, |b| b.start);
                if b0 == b1 || (anchor_block == head_block && b0 > anchor_block) {
                    None
                } else {
                    Some((b0..b1, anchor_block, head_block))
                }
            } else {
                None
            }
        } else {
            return;
        };
        if let Some((range, anchor, head)) = next_bs {
            self.block_sel = Some(range);
            self.block_sel_anchor = Some(anchor);
            self.block_sel_head = Some(head);
        } else {
            if was_block_sel {
                self.block_sel = None;
                self.block_sel_anchor = None;
                self.block_sel_head = None;
            }
            if self.active_block().is_some_and(|r| !r.is_empty()) {
                self.blank = 0;
            }
            self.pending_byte_caret = None;
            self.pending_sel = Some(sel);
            self.focus_editor = true;
        }
    }

    // 현재 문서에 편집 연산을 적용하고 커서/선택을 갱신한다
    fn edit(&mut self, ctx: &egui::Context, f: impl FnOnce(&str, Sel) -> Option<(String, Sel)>) {
        let sel = self.cursor(ctx);
        let Some(doc) = self.doc() else {
            return;
        };
        if let Some((text, new_sel)) = f(&doc.text, sel) {
            self.set_text(text, new_sel);
        }
    }

    fn insert(&mut self, ctx: &egui::Context, s: &str) {
        if self.block_sel.is_some() {
            self.replace_block_sel(s);
            return;
        }
        if self.active_block().is_none() {
            self.append_block();
        }
        let s = s.to_string();
        self.edit(ctx, move |t, sel| {
            let at = sel.0 + char_len(&s);
            Some((replace_range(t, sel, &s), (at, at)))
        });
    }

    // 커서가 링크 안이면 그 링크를, 아니면 선택한 글자를 링크로 만드는 대화상자를 연다
    fn open_link_dialog(&mut self, ctx: &egui::Context) {
        let sel = self.cursor(ctx);
        let (Some(doc), r) = (self.doc(), self.active_block()) else {
            return;
        };
        let text = &doc.text;
        let found = r.and_then(|r| {
            let base = char_len(&text[..r.start]);
            let blk = &text[r];
            block_inline_spans(blk)
                .into_iter()
                .find(|&([os, _, _, ce], k)| {
                    k == Inline::Link && sel.0 >= base + os && sel.1 <= base + ce
                })
                .map(|([os, bs, be, ce], _)| {
                    let label: String = blk.chars().skip(bs).take(be - bs).collect();
                    let url: String = blk.chars().skip(be + 2).take(ce - 1 - (be + 2)).collect();
                    ((base + os, base + ce), label, url)
                })
        });
        self.dialog = Some(match found {
            Some((range, label, url)) => Dialog::Link {
                range,
                label,
                url,
                existing: true,
            },
            None => Dialog::Link {
                range: sel,
                label: text[byte_idx(text, sel.0)..byte_idx(text, sel.1)].to_string(),
                url: String::new(),
                existing: false,
            },
        });
    }

    // 링크 대화상자 결과 반영: url이 None이면 링크 해제(글자만 남김)
    fn apply_link(&mut self, range: Sel, label: &str, url: Option<&str>) {
        let Some(doc) = self.doc() else {
            return;
        };
        let md = match url {
            Some(u) => format!("[{}]({u})", if label.is_empty() { u } else { label }),
            None => label.to_string(),
        };
        let t = replace_range(&doc.text, range, &md);
        let caret = range.0 + char_len(&md);
        self.set_text(t, (caret, caret));
        self.focus_editor = true;
    }

    // 서식 복사: 커서·선택 위치의 서식을 기억(copy)했다가 다른 글에 입힌다(paste)
    fn format_brush_action(&mut self, ctx: &egui::Context, copy: bool) {
        if self.current.is_none() || self.is_visual_table_active() {
            return;
        }
        if self.active_block().is_none() && self.block_sel.is_none() {
            self.status = "서식을 복사할 글을 먼저 선택하세요".into();
            return;
        }
        if copy {
            let sel = self.cursor(ctx);
            let sample = {
                let Some(doc) = self.doc() else {
                    return;
                };
                sample_format(&doc.text, sel)
            };
            self.status = format!("서식 복사됨: {}", describe_format(&sample));
            self.format_brush = Some(sample);
            return;
        }
        let Some(sample) = self.format_brush.clone() else {
            self.status = "먼저 서식을 복사하세요 (Ctrl+Alt+B)".into();
            return;
        };
        let sel = self.cursor(ctx);
        let changed = self.doc().and_then(|d| apply_format(&d.text, sel, &sample));
        match changed {
            Some((text, new_sel)) => {
                self.set_text(text, new_sel);
                self.status = format!("서식 적용됨: {}", describe_format(&sample));
            }
            None => {
                self.status = "이미 같은 서식입니다 (글자를 선택하면 글자 서식도 입혀집니다)".into()
            }
        }
    }

    fn format(&mut self, ctx: &egui::Context, f: Fmt) {
        if let Fmt::Block(kind) = f {
            self.convert_block(ctx, kind);
            return;
        }
        if matches!(f, Fmt::BrushCopy | Fmt::BrushPaste) {
            self.format_brush_action(ctx, matches!(f, Fmt::BrushCopy));
            return;
        }
        if self.active_block().is_none() && self.block_sel.is_none() {
            if matches!(f, Fmt::Clear) {
                return;
            }
            self.append_block();
        }
        match f {
            // 링크는 주소가 화면에 안 보이므로 대화상자로 넣고 고친다
            Fmt::Wrap("[", _, _) => self.open_link_dialog(ctx),
            Fmt::Wrap(pre, suf, ph) => {
                self.edit(ctx, |t, sel| Some(wrap_selection(t, sel, pre, suf, ph)))
            }
            Fmt::Block(_) | Fmt::BrushCopy | Fmt::BrushPaste => unreachable!(),
            Fmt::Clear => self.edit(ctx, |t, (a, b)| {
                if a == b {
                    return None;
                }
                let s = clear_inline_formatting(&t[byte_idx(t, a)..byte_idx(t, b)]);
                Some((replace_range(t, (a, b), &s), (a, a + char_len(&s))))
            }),
        }
    }

    // 블록 유형 전환 (단축키 Ctrl+Alt+0~9 / Ctrl+Alt+G 또는 우클릭 컨텍스트 메뉴)
    fn convert_block(&mut self, ctx: &egui::Context, kind: BlockKind) {
        if self.current.is_none() || self.is_visual_table_active() {
            return;
        }
        if self.active_block().is_none() && self.block_sel.is_none() {
            self.append_block();
        }
        // 현재 블록이 코드·콜아웃·토글인데 일반 줄 블록(문단·제목·목록·인용)으로 전환할 때는 먼저 래퍼를 벗긴다
        if !matches!(
            kind,
            BlockKind::Code | BlockKind::Callout | BlockKind::Toggle
        ) {
            if let (Some(r), Some(doc)) = (
                self.block_sel.clone().or_else(|| self.active_block()),
                self.doc(),
            ) {
                let t = doc.text[r.clone()].trim_start();
                if t.starts_with("```") || t.starts_with("~~~") {
                    self.convert_block(ctx, BlockKind::Code);
                } else if callout_kind(&doc.text[r.clone()]).is_some() {
                    self.convert_block(ctx, BlockKind::Callout);
                } else if is_toggle_block(&doc.text[r.clone()]) {
                    self.convert_block(ctx, BlockKind::Toggle);
                } else if parse_md_table(&doc.text[r]).is_some() {
                    return;
                }
            }
            if kind == BlockKind::Paragraph {
                self.edit(ctx, move |t, sel| {
                    let (out, new_sel) = convert_lines_kind(t, sel, kind);
                    if kind == BlockKind::Numbered && new_sel.0 == new_sel.1 {
                        let (renum, c) = renumber_block_at(&out, new_sel.0);
                        Some((renum, (c, c)))
                    } else {
                        Some((out, new_sel))
                    }
                });
                return;
            }
        }
        if matches!(
            kind,
            BlockKind::Code | BlockKind::Callout | BlockKind::Toggle
        ) {
            let Some(r) = self.block_sel.clone().or_else(|| self.active_block()) else {
                return;
            };
            let Some(doc) = self.doc_mut() else {
                return;
            };
            let src = doc.text[r.clone()].to_string();
            let t = src.trim_start();
            let (replacement, caret_off) = match kind {
                BlockKind::Code => {
                    if t.starts_with("```") || t.starts_with("~~~") {
                        let fence = &t[..3];
                        let first_nl = t.find('\n').map_or(t.len(), |i| i + 1);
                        let body = t[first_nl..]
                            .trim_end()
                            .strip_suffix(fence)
                            .unwrap_or(&t[first_nl..])
                            .trim_end_matches(['\n', '\r'])
                            .to_string();
                        let len = body.len();
                        (body, len)
                    } else if src.is_empty() {
                        ("```\n\n```".to_string(), 4)
                    } else {
                        let s = format!("```\n{src}\n```");
                        let off = 4 + src.len();
                        (s, off)
                    }
                }
                BlockKind::Callout => {
                    if t.starts_with("> [!") {
                        let first_nl = t.find('\n').map_or(t.len(), |i| i + 1);
                        let body: Vec<&str> = t[first_nl..]
                            .lines()
                            .map(|l| {
                                l.strip_prefix("> ")
                                    .or_else(|| l.strip_prefix('>'))
                                    .unwrap_or(l)
                            })
                            .collect();
                        let s = body.join("\n");
                        let len = s.len();
                        (s, len)
                    } else if src.trim().is_empty() {
                        ("> [!NOTE]\n> ".to_string(), 12)
                    } else {
                        let body: Vec<String> = src
                            .lines()
                            .map(|l| format!("> {}", l.strip_prefix("> ").unwrap_or(l)))
                            .collect();
                        let s = format!("> [!NOTE]\n{}", body.join("\n"));
                        let len = s.len();
                        (s, len)
                    }
                }
                BlockKind::Toggle => {
                    if let Some(rest) = t.strip_prefix(":::toggle") {
                        let first_nl = rest.find('\n').unwrap_or(rest.len());
                        let title = rest[..first_nl].trim();
                        let body_part = rest.get(first_nl + 1..).unwrap_or("");
                        let inner = body_part
                            .trim_end()
                            .strip_suffix(":::")
                            .unwrap_or(body_part)
                            .trim_end_matches(['\n', '\r']);
                        let s = if inner.is_empty() || inner == "내용" {
                            title.to_string()
                        } else {
                            format!("{title}\n{inner}")
                        };
                        let len = s.len();
                        (s, len)
                    } else if src.trim().is_empty() {
                        (":::toggle 제목\n내용\n:::".to_string(), 10)
                    } else if !src.contains('\n') {
                        let s = format!(":::toggle {src}\n내용\n:::");
                        let off = ":::toggle ".len() + src.len() + 1;
                        (s, off)
                    } else {
                        let mut lines = src.lines();
                        let title = lines.next().unwrap_or("제목");
                        let body: Vec<&str> = lines.collect();
                        let s = format!(":::toggle {title}\n{}\n:::", body.join("\n"));
                        let len = s.len().saturating_sub(4);
                        (s, len)
                    }
                }
                _ => unreachable!(),
            };
            doc.text.replace_range(r.clone(), &replacement);
            doc.edited_at = Some(Instant::now());
            doc.refresh_meta();
            let caret = (r.start + caret_off).min(doc.text.len());
            self.blank = 0;
            self.block_sel = None;
            self.block_sel_anchor = None;
            self.block_sel_head = None;
            self.active = Some(r.start);
            self.pending_sel = None;
            self.pending_byte_caret = Some((r.start, caret));
            self.focus_editor = true;
            return;
        }
        self.edit(ctx, move |t, sel| {
            let (out, new_sel) = convert_lines_kind(t, sel, kind);
            if kind == BlockKind::Numbered && new_sel.0 == new_sel.1 {
                let (renum, c) = renumber_block_at(&out, new_sel.0);
                Some((renum, (c, c)))
            } else {
                Some((out, new_sel))
            }
        });
    }

    fn toggle_todo(&mut self, ctx: &egui::Context) {
        if self.current.is_none() {
            return;
        }
        if self.active_block().is_none() && self.block_sel.is_none() {
            self.append_block();
        }
        self.edit(ctx, |t, sel| Some(toggle_todo_lines(t, sel)));
    }

    fn indent_active(&mut self, ctx: &egui::Context) {
        self.edit(ctx, |t, sel| {
            indent_list(t, sel, false).or_else(|| Some(indent_lines(t, sel, false)))
        });
    }

    fn outdent_active(&mut self, ctx: &egui::Context) {
        self.edit(ctx, |t, sel| {
            if let Some(r) = indent_list(t, sel, true) {
                return Some(r);
            }
            let (out, new_sel) = indent_lines(t, sel, true);
            if out != t {
                return Some((out, new_sel));
            }
            // 줄 맨 앞에 공백이 없더라도 커서 바로 앞이 Tab으로 넣은 공백(2칸)이나 '\t'이면 되돌린다
            if sel.0 == sel.1 && sel.0 > 0 {
                let b = byte_idx(t, sel.0);
                if t[..b].ends_with('\t') {
                    return Some((
                        replace_range(t, (sel.0 - 1, sel.0), ""),
                        (sel.0 - 1, sel.0 - 1),
                    ));
                }
                if sel.0 >= 2 && t[..b].ends_with("  ") {
                    return Some((
                        replace_range(t, (sel.0 - 2, sel.0), ""),
                        (sel.0 - 2, sel.0 - 2),
                    ));
                }
            }
            Some((out, new_sel))
        });
    }

    // 현재 블록이 비어 있으면 그 자리에, 내용이 있으면 바로 아래 새 블록으로 템플릿(표·구분선·수식 등) 삽입
    fn insert_block_template(&mut self, tpl: &str, caret: usize) {
        if self.current.is_none() {
            return;
        }
        if let Some(bs) = self.block_sel.take() {
            self.block_sel_anchor = None;
            self.block_sel_head = None;
            self.leave_blank();
            let Some(doc) = self.doc_mut() else {
                return;
            };
            let s = bs.start.min(doc.text.len());
            let e = bs.end.min(doc.text.len());
            doc.text.replace_range(s..e, tpl);
            doc.edited_at = Some(Instant::now());
            doc.refresh_meta();
            let at = s + caret.min(tpl.len());
            self.blank = 0;
            self.active = Some(s);
            self.pending_sel = None;
            self.pending_byte_caret = Some((s, at));
            self.focus_editor = true;
            if tpl == "---" {
                self.split(s + tpl.len());
            }
            return;
        }
        if self.active_block().is_none() {
            self.append_block();
        }
        let Some(r) = self.active_block() else {
            return;
        };
        let Some(doc) = self.doc_mut() else {
            return;
        };
        let is_empty = r.is_empty() || doc.text[r.clone()].trim().is_empty();
        let start = if is_empty {
            doc.text.replace_range(r.clone(), tpl);
            r.start
        } else {
            let ins = format!("\n\n{tpl}");
            doc.text.insert_str(r.end, &ins);
            r.end + 2
        };
        doc.edited_at = Some(Instant::now());
        doc.refresh_meta();
        let at = start + caret.min(tpl.len());
        self.blank = 0;
        self.active = Some(start);
        self.pending_sel = None;
        self.pending_byte_caret = Some((start, at));
        self.focus_editor = true;
        self.scroll_cursor = Some(None);
        if tpl == "---" {
            self.split(start + tpl.len());
        }
    }

    fn delete_current_blocks(&mut self) -> bool {
        if self.block_sel.is_some() {
            return self.delete_block_sel();
        }
        let Some(r) = self.active_block() else {
            return false;
        };
        if r.is_empty() {
            return false;
        }
        self.block_sel = Some(r);
        self.delete_block_sel()
    }

    fn insert_block_below(&mut self) {
        if let Some(bs) = self.block_sel.take() {
            self.block_sel_anchor = None;
            self.block_sel_head = None;
            self.split(bs.end);
        } else if let Some(r) = self.active_block() {
            self.split(r.end);
        } else {
            self.append_block();
        }
    }

    fn copy_selection(&mut self, ctx: &egui::Context) {
        let Some(doc) = self.doc() else {
            return;
        };
        if let Some(bs) = &self.block_sel {
            let r = clamp_char_range(&doc.text, bs);
            if r.start < r.end {
                ctx.copy_text(doc.text[r].to_string());
                self.status = "선택한 블록을 복사했습니다".into();
            }
            return;
        }
        let (a, b) = self.cursor(ctx);
        if a < b {
            let (ba, bb) = (byte_idx(&doc.text, a), byte_idx(&doc.text, b));
            ctx.copy_text(doc.text[ba..bb].to_string());
        } else if let Some(r) = self.active_block().filter(|r| !r.is_empty()) {
            ctx.copy_text(doc.text[r].to_string());
            self.status = "현재 블록을 복사했습니다".into();
        }
    }

    fn cut_selection(&mut self, ctx: &egui::Context) {
        if self.block_sel.is_some() {
            self.copy_selection(ctx);
            self.delete_block_sel();
            return;
        }
        let (a, b) = self.cursor(ctx);
        if a < b {
            if let Some(doc) = self.doc() {
                let (ba, bb) = (byte_idx(&doc.text, a), byte_idx(&doc.text, b));
                ctx.copy_text(doc.text[ba..bb].to_string());
                let next = replace_range(&doc.text, (a, b), "");
                self.set_text(next, (a, a));
            }
        } else if let Some(r) = self.active_block().filter(|r| !r.is_empty()) {
            if let Some(doc) = self.doc() {
                ctx.copy_text(doc.text[r.clone()].to_string());
            }
            self.block_sel = Some(r);
            self.delete_block_sel();
        }
    }

    fn paste_from_clipboard(&mut self, ctx: &egui::Context) {
        if let Some(s) = theme::clipboard_text() {
            let norm = if self.paste_normalizes() {
                normalize_pasted_markdown(&s)
            } else {
                s.replace("\r\n", "\n").replace('\r', "\n")
            };
            self.insert(ctx, &norm);
        } else {
            self.status = "클립보드에 붙여넣을 텍스트가 없습니다".into();
        }
    }

    fn select_all_doc(&mut self) {
        if let Some(len) = self.doc().map(|d| d.text.len()).filter(|&l| l > 0) {
            self.leave_blank();
            self.active = None;
            self.block_sel = Some(0..len);
            self.block_sel_anchor = Some(0);
            self.block_sel_head = Some(len);
        }
    }

    fn copy_markdown_source(&mut self, ctx: &egui::Context) {
        if let Some(d) = self.doc() {
            ctx.copy_text(d.text.clone());
            self.status = "마크다운 원본을 복사했습니다".into();
        }
    }

    fn copy_html_code(&mut self, ctx: &egui::Context) {
        if let Some(d) = self.doc() {
            ctx.copy_text(md_to_html(&d.text));
            self.status = "HTML 코드를 복사했습니다".into();
        }
    }

    fn slash_query(&self) -> Option<String> {
        let r = self.active_block()?;
        let q = self.doc()?.text[r].strip_prefix('/')?.to_string();
        (!q.contains(char::is_whitespace)).then_some(q)
    }

    fn apply_slash(&mut self, i: usize) {
        let item = &SLASH[i];
        let Some(r) = self.active_block() else {
            return;
        };
        let Some(doc) = self.doc_mut() else {
            return;
        };
        doc.text.replace_range(r.clone(), item.tpl);
        doc.edited_at = Some(Instant::now());
        doc.refresh_meta();
        let caret = r.start + item.caret.min(item.tpl.len());
        self.blank = 0;
        self.slash_sel = 0;
        self.active = Some(r.start);
        self.pending_sel = None;
        self.pending_byte_caret = Some((r.start, caret));
        self.focus_editor = true;
        if item.tpl == "---" {
            self.split(caret);
        } else if item.title == "이미지" {
            self.dialog = Some(Dialog::Image {
                url_tab: false,
                url: String::new(),
                alt: String::new(),
            });
        }
    }

    // Enter: 목록은 다음 항목, 빈 항목은 목록 끝내기, 빈 블록은 빈 줄 추가 후 아래로,
    // 코드 펜스("```"/"```lang")는 자동 닫기 및 닫는 줄 탈출·자동 들여쓰기 지원, 그 밖에는 블록 나누기.
    fn enter(&mut self, ctx: &egui::Context) -> bool {
        let mut sel = self.cursor(ctx);
        if std::mem::take(&mut self.ime_composing) && sel.0 != sel.1 {
            // 한글/CJK IME 조합 중인 음절 범위(sel.0..sel.1)를 선택 삭제로 오인하지 않고 음절 끝에서 Enter 처리
            sel = (sel.1, sel.1);
        }
        let (Some(r), Some(doc)) = (self.active_block(), self.doc()) else {
            return false;
        };
        let t0 = doc.text[r.clone()].trim_start();
        if t0.starts_with("```") || t0.starts_with("~~~") {
            let fence = &t0[..3];
            let b = self.block_byte_of(&r, doc, sel.0);
            let ls = doc.text[..b].rfind('\n').map_or(0, |i| i + 1).max(r.start);
            let le = doc.text[b..].find('\n').map_or(doc.text.len(), |i| b + i);
            if !doc.text[r.clone()].contains('\n') {
                if let Some((indent, f_tok, lang)) = fence_opener_line(&doc.text[r.clone()]) {
                    if sel.0 == sel.1 && b >= r.start + indent.len() + 3 {
                        let expanded = format!("{indent}{f_tok}{lang}\n{indent}\n{indent}{f_tok}");
                        let caret =
                            r.start + indent.len() + f_tok.len() + lang.len() + 1 + indent.len();
                        if let Some(d) = self.doc_mut() {
                            d.apply_block_edit(r.clone(), &expanded);
                            d.edited_at = Some(Instant::now());
                        }
                        self.blank = 0;
                        self.active = Some(r.start);
                        self.pending_sel = None;
                        self.pending_byte_caret = Some((r.start, caret));
                        self.focus_editor = true;
                        self.scroll_cursor = Some(None);
                        return true;
                    }
                }
                return false;
            }
            if ls > r.start
                && sel.0 == sel.1
                && doc.text[ls..le].trim() == fence
                && doc.text[b..le].trim().is_empty()
            {
                self.split(r.end);
                return true;
            }
            let (ba, bb) = if sel.0 != sel.1 {
                (
                    self.block_byte_of(&r, doc, sel.0),
                    self.block_byte_of(&r, doc, sel.1),
                )
            } else {
                (b, b)
            };
            let line_indent = if ls > r.start {
                doc.text[ls..ba]
                    .chars()
                    .take_while(|&c| c == ' ' || c == '\t')
                    .collect::<String>()
            } else {
                String::new()
            };
            if !line_indent.is_empty() {
                let local_a = ba - r.start;
                let local_b = bb - r.start;
                let mut new_blk = doc.text[r.clone()].to_string();
                new_blk.replace_range(local_a..local_b, &format!("\n{line_indent}"));
                let caret = ba + 1 + line_indent.len();
                if let Some(d) = self.doc_mut() {
                    d.apply_block_edit(r.clone(), &new_blk);
                    d.edited_at = Some(Instant::now());
                }
                self.blank = 0;
                self.active = Some(r.start);
                self.pending_sel = None;
                self.pending_byte_caret = Some((r.start, caret));
                self.focus_editor = true;
                self.scroll_cursor = Some(None);
                return true;
            }
            return false;
        }
        if ["$$", ":::toggle", "|"].iter().any(|p| t0.starts_with(p)) {
            if (t0.starts_with("$$") || t0.starts_with(":::toggle"))
                && doc.text[r.clone()].contains('\n')
            {
                let close_tok = if t0.starts_with("$$") { "$$" } else { ":::" };
                let b = self.block_byte_of(&r, doc, sel.0);
                let ls = doc.text[..b].rfind('\n').map_or(0, |i| i + 1).max(r.start);
                let le = doc.text[b..].find('\n').map_or(doc.text.len(), |i| b + i);
                if ls > r.start
                    && sel.0 == sel.1
                    && doc.text[ls..le].trim() == close_tok
                    && doc.text[b..le].trim().is_empty()
                {
                    self.split(r.end);
                    return true;
                }
            }
            return false;
        }
        // 아무 내용 없이 빈 블록에서 Enter: 의도적인 빈 줄을 남기고 다음 줄로 이동
        if r.is_empty() {
            self.blank = 0;
            self.split(r.start);
            self.blank = 0;
            return true;
        }
        let b0 = self.block_byte_of(&r, doc, sel.0);
        let ls0 = doc.text[..b0].rfind('\n').map_or(0, |i| i + 1).max(r.start);
        let le0 = doc.text[b0..].find('\n').map_or(doc.text.len(), |i| b0 + i);
        if sel.0 == sel.1 && ls0 > r.start {
            if let Some((indent, f_tok, lang)) = fence_opener_line(&doc.text[ls0..le0]) {
                if b0 >= ls0 + indent.len() + 3 {
                    let mut t2 = doc.text.clone();
                    let expanded = format!("\n\n{indent}{f_tok}{lang}\n{indent}\n{indent}{f_tok}");
                    let new_start = ls0 + 1;
                    let caret =
                        new_start + indent.len() + f_tok.len() + lang.len() + 1 + indent.len();
                    t2.replace_range(ls0 - 1..le0, &expanded);
                    if let Some(d) = self.doc_mut() {
                        d.text = t2;
                        d.edited_at = Some(Instant::now());
                        d.refresh_meta();
                    }
                    self.blank = 0;
                    self.active = Some(new_start);
                    self.pending_sel = None;
                    self.pending_byte_caret = Some((new_start, caret));
                    self.focus_editor = true;
                    self.scroll_cursor = Some(None);
                    return true;
                }
            }
        }
        let text = doc.text.clone();
        if sel.0 != sel.1 {
            let (ba, bb) = (
                self.block_byte_of(&r, doc, sel.0),
                self.block_byte_of(&r, doc, sel.1),
            );
            let mut t2 = text;
            t2.replace_range(ba..bb, "");
            let c = char_len(&t2[..ba]);
            self.set_text(t2, (c, c));
            self.split(ba);
            return true;
        }
        let mut text = text;
        let mut b = self.block_byte_of(&r, doc, sel.0);
        let block = &text[r.clone()];
        // 제목·인용·알림 상자·각주의 내용 맨 앞 Enter → 블록은 그대로 두고 위에 빈 문단 (노션식)
        let vis0 = visible_bounds(block).0;
        if vis0 > 0
            && list_marker_bytes(block).is_none()
            && char_len(&text[r.start..b]) == vis0
            && !text[b..r.end].trim().is_empty()
        {
            self.split(r.start);
            return true;
        }
        let ls = text[..b].rfind('\n').map_or(0, |i| i + 1).max(r.start);
        let le = text[b..].find('\n').map_or(text.len(), |i| b + i);
        // **굵게**·[링크](주소)·`코드` 안에서 Enter → 기호를 닫고 다시 열어 두 쪽 모두 서식 유지
        let cl = b - ls;
        let span = inline_spans(&text[ls..le])
            .into_iter()
            .find(|s| s.0 < cl && cl < s.3 && s.4 != Inline::FootRef);
        if let Some((os, bs, be, ce, _)) = span {
            if cl <= bs {
                b = ls + os;
            } else if cl >= be {
                b = ls + ce;
            } else {
                let (open, close) = (
                    text[ls + os..ls + bs].to_string(),
                    text[ls + be..ls + ce].to_string(),
                );
                text.insert_str(b, &format!("{close}{open}"));
                b += close.len();
            }
        }
        let sel = (char_len(&text[..b]), char_len(&text[..b]));
        let le = ls + text[ls..].find('\n').unwrap_or(text.len() - ls);
        let line = &text[ls..le];
        let r_end = r.end + (text.len() - doc.text.len());
        // 인용·알림 상자 줄 중간 Enter → 인용 안에서 줄바꿈 ("> " 이어감). 마지막 줄 끝이면 아래 새 문단
        if line.trim_start().starts_with('>') && !(le == r_end && b == le) {
            let pfx = line[..hidden_prefix(line)].to_string();
            if line.trim() == ">" && ls > r.start {
                // 빈 인용 줄에서 Enter → 그 줄을 지우고 인용 밖으로 ('\n\n\n' 3중 줄바꿈이 생기지 않게 정리)
                let rm_end = if le < r_end { le + 1 } else { le };
                text.replace_range(ls - 1..rm_end, "");
                let c = char_len(&text[..ls - 1]);
                self.set_text(text, (c, c));
                self.split(ls - 1);
                return true;
            }
            text.insert_str(b, &format!("\n{pfx}"));
            let c = char_len(&text[..b]) + 1 + char_len(&pfx);
            self.set_text(text, (c, c));
            return true;
        }
        if text.len() != doc.text.len() {
            self.set_text(text.clone(), sel);
        }
        if let Some((.., empty)) = list_marker(&text[ls..le]) {
            if !empty {
                if let Some((t2, c)) = smart_enter(&text, sel.0) {
                    self.set_text(t2, (c, c));
                    return true;
                }
                return false;
            }
            // 들여쓴 빈 항목에서 Enter → 한 단계 내어쓰기 (노션·구글 문서와 같은 동작)
            if text[ls..le].starts_with(' ') {
                if let Some((t2, s2)) = indent_list(&text, sel, true) {
                    self.set_text(t2, s2);
                    return true;
                }
            }
            let mut t2 = text;
            if ls == r.start {
                if le < r_end {
                    // 다중 줄 블록의 첫 줄이 빈 항목이면 빈 첫 줄 전체를 지워 '\n\n\n' 고아 빈 줄을 막는다
                    t2.replace_range(ls..le + 1, "");
                    let (t3, c0) = renumber_block_at(&t2, char_len(&t2[..ls]));
                    let next_le = line_end(&t3, c0);
                    let next_line = &t3[byte_idx(&t3, c0)..byte_idx(&t3, next_le)];
                    let caret = c0 + char_len(&next_line[..hidden_prefix(next_line)]);
                    self.set_text(t3, (caret, caret));
                    return true;
                }
                // 한 줄짜리 빈 항목이면 기호만 지우고 일반 문단으로
                t2.replace_range(ls..le, "");
                let c = char_len(&t2[..ls]);
                self.set_text(t2, (c, c));
                return true;
            }
            let rm_end = if le < r_end { le + 1 } else { le };
            t2.replace_range(ls - 1..rm_end, "");
            let c = char_len(&t2[..ls - 1]);
            self.set_text(t2, (c, c));
            self.split(ls - 1);
            return true;
        }
        self.split(b);
        true
    }

    // 숨긴 인라인 기호(** 등) 옆의 Backspace/Delete. true면 직접 처리했으니 키를 소비한다.
    fn delete_hidden(&mut self, ctx: &egui::Context, back: bool) -> bool {
        let (Some(r), Some(doc), Some(name)) =
            (self.active_block(), self.doc(), self.current.clone())
        else {
            return false;
        };
        let block = &doc.text[r.clone()];
        if code_body_range(block).is_some() {
            return false;
        }
        let local = self.local_cursor(ctx, doc, &r);
        if local.0 != local.1 {
            return false;
        }
        match delete_near_hidden(block, local.0, back) {
            Some(Ok((blk, caret))) => {
                let c = char_len(&doc.text[..r.start]) + caret;
                let mut t = doc.text.clone();
                t.replace_range(r, &blk);
                self.set_text(t, (c, c));
                true
            }
            Some(Err(c)) => {
                // 커서만 기호 바깥으로 옮기고 나머지는 기본 Backspace/Delete 흐름에 맡긴다
                let id = Self::editor_id(&name, r.start);
                let mut st = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
                let cc = egui::text::CCursor::new(c);
                st.cursor
                    .set_char_range(Some(egui::text::CCursorRange::one(cc)));
                egui::TextEdit::store_state(ctx, id, st);
                false
            }
            None => false,
        }
    }

    fn backspace_list(&mut self, ctx: &egui::Context) -> bool {
        let (Some(r), Some(doc)) = (self.active_block(), self.doc()) else {
            return false;
        };
        let sel = self.cursor(ctx);
        let block = &doc.text[r.clone()];
        if let Some((a, b)) = fenced_body_range(block).filter(|_| !is_toggle_block(block)) {
            // 빈 코드·수식 블록 맨 앞 Backspace → 블록을 지우고 빈 문단으로 (노션식)
            let local = self.local_cursor(ctx, doc, &r);
            let body: String = block.chars().skip(a).take(b - a).collect();
            if local == (a, a) && body.trim().is_empty() {
                let mut t = doc.text.clone();
                t.replace_range(r.clone(), "");
                let c = char_len(&t[..r.start]);
                self.set_text(t, (c, c));
                return true;
            }
            return false;
        }
        if sel.0 != sel.1 {
            return false;
        }
        match backspace_list_marker(&doc.text, sel.0) {
            Some((t, c)) => {
                self.set_text(t, (c, c));
                true
            }
            None => false,
        }
    }

    fn delete_empty_list(&mut self, ctx: &egui::Context) -> bool {
        let Some(doc) = self.doc() else {
            return false;
        };
        let sel = self.cursor(ctx);
        if sel.0 != sel.1 {
            return false;
        }
        match delete_empty_list_line(&doc.text, sel.0) {
            Some((t, c)) => {
                self.set_text(t, (c, c));
                true
            }
            None => false,
        }
    }

    // 바로 앞이 빈 줄 슬롯(prev.is_empty())이고 커서가 블록 내용 시작에 있으면,
    // 블록의 목록·제목·코드 기호를 깨지 않고 위의 빈 줄만 먼저 지워 블록을 위로 당긴다.
    fn backspace_blank_above(&mut self, ctx: &egui::Context) -> bool {
        let (Some(r), Some(doc)) = (self.active_block(), self.doc()) else {
            return false;
        };
        if r.is_empty() {
            return false;
        }
        let local = self.local_cursor(ctx, doc, &r);
        if local.0 != local.1 {
            return false;
        }
        let blk = &doc.text[r.clone()];
        let vis0 = visible_bounds(blk).0;
        if local.0 != 0 && local.0 != vis0 {
            return false;
        }
        let vis_b = byte_idx(blk, vis0);
        if blk[vis_b..].trim().is_empty() && !is_atomic_block(blk) {
            return false;
        }
        let slots = self.doc_slots();
        let Some(idx) = slot_index(&slots, &r) else {
            return false;
        };
        let Some(prev) = idx.checked_sub(1).and_then(|i| slots.get(i)).cloned() else {
            return false;
        };
        if !prev.is_empty() {
            return false;
        }
        let rm_start = r.start.saturating_sub(2).max(prev.start);
        let mut t2 = doc.text.clone();
        t2.replace_range(rm_start..r.start, "");
        let c = char_len(&t2[..rm_start]) + local.0;
        self.blank = 0;
        self.active = Some(rm_start);
        self.set_text(t2, (c, c));
        self.scroll_cursor = Some(None);
        true
    }

    // 블록 맨 앞 Backspace: 앞 빈 줄을 지우거나, 내용이 있으면 앞 블록과 합친다
    fn backspace_at_start(&mut self, ctx: &egui::Context) -> bool {
        let (Some(r), Some(doc)) = (self.active_block(), self.doc()) else {
            return false;
        };
        let local = self.local_cursor(ctx, doc, &r);
        if local.0 != local.1 || local.0 != 0 {
            return false;
        }
        if r.start == 0 && r.is_empty() {
            let post = doc.text.bytes().take_while(|&b| b == b'\n').count();
            if post == 0 {
                return false;
            }
            let rm = post.min(2);
            let mut t2 = doc.text.clone();
            t2.replace_range(0..rm, "");
            self.blank = 0;
            self.active = Some(0);
            self.set_text(t2, (0, 0));
            return true;
        }
        let slots = self.doc_slots();
        let Some(idx) = slot_index(&slots, &r) else {
            return false;
        };
        let Some(prev) = idx.checked_sub(1).and_then(|i| slots.get(i)).cloned() else {
            return false;
        };
        if prev.is_empty() {
            // 바로 앞이 빈 줄 블록이면 빈 줄 하나(최대 2바이트)만 지우고 현재 블록을 위로 올린다
            let rm_start = r.start.saturating_sub(2).max(prev.start);
            let mut t2 = doc.text.clone();
            t2.replace_range(rm_start..r.start, "");
            let c = char_len(&t2[..rm_start]);
            self.blank = 0;
            self.active = Some(rm_start);
            self.set_text(t2, (c, c));
            self.scroll_cursor = Some(None);
            return true;
        }
        if r.is_empty() {
            // 두 블록 사이 빈 줄 슬롯에서 Backspace: 뒤에 블록이 있으면 블록 구분자('\n\n') 2개는 반드시 남겨 두 블록이 합쳐지지 않게 한다
            let total_nl = doc.text[prev.end..]
                .bytes()
                .take_while(|&b| b == b'\n')
                .count();
            let has_after = prev.end + total_nl < doc.text.len();
            let rm = if has_after {
                total_nl.saturating_sub(2).clamp(1, 2)
            } else {
                (r.start - prev.end).min(2)
            };
            let rm_start = r.start.saturating_sub(rm).max(prev.end);
            let mut t2 = doc.text.clone();
            t2.replace_range(rm_start..r.start, "");
            let c = char_len(&t2[..prev.end]);
            self.blank = 0;
            self.active = Some(prev.start);
            self.set_text(t2, (c, c));
            self.scroll_cursor = Some(None);
            return true;
        }
        let pb = &doc.text[prev.clone()];
        let cur = &doc.text[r.clone()];
        if is_atomic_block(pb) {
            // 바로 앞이 구분선·이미지면 노션처럼 그 블록을 지운다 (글자를 붙이면 원문이 드러난다)
            let mut t2 = doc.text.clone();
            t2.replace_range(prev.start..r.start, "");
            let c = char_len(&t2[..prev.start]);
            self.blank = 0;
            self.active = Some(prev.start);
            self.set_text(t2, (c, c));
            self.scroll_cursor = Some(None);
            return true;
        }
        if is_unmergeable_block(pb)
            || is_unmergeable_block(cur)
            || (heading_level(pb.trim_start()).is_some() && cur.contains('\n'))
        {
            return false;
        }
        let mut t2 = doc.text.clone();
        t2.replace_range(prev.end..r.start, "");
        let c = char_len(&t2[..prev.end]);
        self.blank = 0;
        self.active = Some(prev.start);
        self.set_text(t2, (c, c));
        self.scroll_cursor = Some(None);
        true
    }

    // 블록 맨 끝 Delete: 뒤 빈 줄을 지우거나, 내용이 있으면 뒤 블록과 합친다
    fn delete_at_end(&mut self, ctx: &egui::Context) -> bool {
        let (Some(r), Some(doc)) = (self.active_block(), self.doc()) else {
            return false;
        };
        let local = self.local_cursor(ctx, doc, &r);
        if local.0 != local.1 {
            return false;
        }
        let blen = char_len(&doc.text[r.clone()]);
        if local.0 < blen {
            return false;
        }
        let slots = self.doc_slots();
        let Some(idx) = slot_index(&slots, &r) else {
            return false;
        };
        let Some(next) = slots.get(idx + 1).cloned() else {
            if r.is_empty() && r.start > 0 {
                return self.backspace_at_start(ctx);
            }
            return false;
        };
        if next.is_empty() {
            let rm_end = (r.end + 2).min(doc.text.len());
            let mut t2 = doc.text.clone();
            t2.replace_range(r.end..rm_end, "");
            let c = char_len(&t2[..r.end]);
            self.blank = 0;
            self.active = Some(r.start);
            self.set_text(t2, (c, c));
            self.scroll_cursor = Some(None);
            return true;
        }
        // 구분선·이미지 위 Delete → 그 블록을 지운다 / 바로 뒤가 구분선·이미지면 그 블록을 지운다
        let (cur, nb) = (&doc.text[r.clone()], &doc.text[next.clone()]);
        let gone = if is_atomic_block(cur) {
            Some(r.start..next.start)
        } else if !r.is_empty() && is_atomic_block(nb) {
            Some(r.end..next.end)
        } else {
            None
        };
        if let Some(gone) = gone {
            let mut t2 = doc.text.clone();
            t2.replace_range(gone.clone(), "");
            let at = gone.start.min(r.end);
            let c = char_len(&t2[..at]);
            self.blank = 0;
            self.active = Some(r.start);
            self.set_text(t2, (c, c));
            self.scroll_cursor = Some(None);
            return true;
        }
        if !r.is_empty() && (is_unmergeable_block(cur) || is_unmergeable_block(nb)) {
            return false;
        }
        if !r.is_empty() {
            let cur_last = cur.rsplit('\n').next().unwrap_or(cur);
            let nb_first = nb.split('\n').next().unwrap_or(nb);
            let both_lists =
                list_marker_bytes(cur_last).is_some() && list_marker_bytes(nb_first).is_some();
            let both_quotes = cur_last.trim_start().starts_with('>')
                && nb_first.trim_start().starts_with('>')
                && callout_kind(nb).is_none();
            if both_lists || both_quotes {
                // 두 목록(또는 두 인용) 블록 사이에서 Delete → 기호를 깨지 않고 두 블록을 한 블록('\n')으로 잇는다
                let mut t2 = doc.text.clone();
                t2.replace_range(r.end..next.start, "\n");
                let c0 = char_len(&t2[..r.end]);
                let (t3, c) = renumber_block_at(&t2, c0);
                self.blank = 0;
                self.active = Some(r.start);
                self.set_text(t3, (c, c));
                self.scroll_cursor = Some(None);
                return true;
            }
        }
        // 뒤 블록의 숨긴 기호(## · - · > 등)는 떼고 내용만 붙인다. 여러 줄 구조(목록·인용)는 깨지니 그대로 둔다
        let vis0 = if r.is_empty() {
            0
        } else {
            visible_bounds(nb).0
        };
        if vis0 > 0 && nb.contains('\n') {
            return false;
        }
        let cut = next.start + byte_idx(nb, vis0);
        let mut t2 = doc.text.clone();
        t2.replace_range(r.end..cut, "");
        let c = char_len(&t2[..r.end]);
        self.blank = 0;
        self.active = Some(r.start);
        self.set_text(t2, (c, c));
        self.scroll_cursor = Some(None);
        true
    }

    fn at_block_vertical_edge(&self, r: &Range<usize>, b: usize, doc: &Doc, up: bool) -> bool {
        if r.is_empty() {
            return true;
        }
        if let Some(info) = self
            .cursor_info
            .filter(|i| i.block_start == r.start && i.doc_rev == doc.rev)
        {
            return if up {
                info.on_first_row
            } else {
                info.on_last_row
            };
        }
        if up {
            !doc.text[r.start..b].contains('\n')
        } else {
            !doc.text[b..r.end].contains('\n')
        }
    }

    // 첫 줄에서 ↑ / 마지막 줄에서 ↓ 는 열 위치를 유지하며 이웃 블록(빈 줄 포함)으로 이동
    fn arrow(&mut self, ctx: &egui::Context, up: bool) -> bool {
        let (Some(r), Some(doc)) = (self.active_block(), self.doc()) else {
            return false;
        };
        let local = self.local_cursor(ctx, doc, &r);
        if local.0 != local.1 {
            return false;
        }
        let b = r.start + byte_idx(&doc.text[r.clone()], local.0);
        if !self.at_block_vertical_edge(&r, b, doc, up) {
            return false;
        }
        let ls = doc.text[r.start..b]
            .rfind('\n')
            .map_or(r.start, |i| r.start + i + 1);
        let col = char_len(&doc.text[ls..b]);
        let slots = self.nav_slots();
        let Some(idx) = slot_index(&slots, &r) else {
            return false;
        };
        let target = if up {
            idx.checked_sub(1).and_then(|i| slots.get(i))
        } else {
            slots.get(idx + 1)
        }
        .cloned();
        let Some(t) = target else {
            // 문서 맨 끝 코드·수식·접기 블록의 마지막 줄에서 ↓: 아래에 새 문단을 만들고 빠져나간다 (닫는 줄은 숨겨져 있음)
            if !up && fenced_body_range(&doc.text[r.clone()]).is_some() {
                self.split(r.end);
                return true;
            }
            return false;
        };
        let caret = if t.is_empty() {
            t.start
        } else if up {
            let t_ls = doc.text[t.start..t.end]
                .rfind('\n')
                .map_or(t.start, |i| t.start + i + 1);
            let max_c = char_len(&doc.text[t_ls..t.end]);
            t_ls + byte_idx(&doc.text[t_ls..t.end], col.min(max_c))
        } else {
            let t_le = doc.text[t.start..t.end]
                .find('\n')
                .map_or(t.end, |i| t.start + i);
            let max_c = char_len(&doc.text[t.start..t_le]);
            t.start + byte_idx(&doc.text[t.start..t_le], col.min(max_c))
        };
        self.go(t.start, caret);
        true
    }

    // 블록 맨 앞에서 ← / 맨 끝에서 → 로 이웃 블록 이동
    fn arrow_lr(&mut self, ctx: &egui::Context, right: bool) -> bool {
        let (Some(r), Some(doc)) = (self.active_block(), self.doc()) else {
            return false;
        };
        let local = self.local_cursor(ctx, doc, &r);
        if local.0 != local.1 {
            return false;
        }
        let (vis_start, vis_end) = visible_bounds(&doc.text[r.clone()]);
        if !right {
            if local.0 > vis_start {
                return false;
            }
        } else if local.0 < vis_end {
            return false;
        }
        let slots = self.nav_slots();
        let Some(idx) = slot_index(&slots, &r) else {
            return false;
        };
        let target = if right {
            slots.get(idx + 1).map(|t| (t.start, t.start))
        } else {
            idx.checked_sub(1)
                .and_then(|i| slots.get(i))
                .map(|t| (t.start, t.end))
        };
        match target {
            Some((s, c)) => {
                self.go(s, c);
                true
            }
            None => false,
        }
    }

    // Shift + 방향키로 블록 경계를 넘어 다중 블록 선택 확장/축소
    fn shift_arrow(&mut self, ctx: &egui::Context, up: bool, horizontal: bool) -> bool {
        if let Some(bs) = self.block_sel.clone() {
            let slots = self.nav_slots();
            if slots.is_empty() {
                return false;
            }
            let anchor_pos = self.block_sel_anchor.unwrap_or(bs.start);
            let head_pos = self
                .block_sel_head
                .unwrap_or(if up { bs.start } else { bs.end });
            let a_idx = slots
                .iter()
                .position(|s| s.start <= anchor_pos && anchor_pos <= s.end)
                .unwrap_or(0);
            let h_idx = slots
                .iter()
                .position(|s| s.start <= head_pos && head_pos <= s.end)
                .unwrap_or(a_idx);
            let new_h = if up {
                h_idx.saturating_sub(1)
            } else {
                (h_idx + 1).min(slots.len() - 1)
            };
            let lo = a_idx.min(new_h);
            let hi = a_idx.max(new_h);
            let range = slots[lo].start..slots[hi].end;
            self.block_sel = Some(range);
            self.block_sel_anchor = Some(slots[a_idx].start);
            self.block_sel_head = Some(slots[new_h].start);
            self.scroll_to = Some(slots[new_h].start);
            return true;
        }
        let (Some(r), Some(doc)) = (self.active_block(), self.doc()) else {
            return false;
        };
        let sel = self.cursor(ctx);
        let b = self.block_byte_of(&r, doc, if up { sel.0 } else { sel.1 });
        let at_edge = if horizontal {
            if up {
                b == r.start
            } else {
                b == r.end
            }
        } else {
            self.at_block_vertical_edge(&r, b, doc, up)
        };
        if !at_edge {
            return false;
        }
        let slots = self.nav_slots();
        let Some(idx) = slot_index(&slots, &r) else {
            return false;
        };
        let adj_idx = if up {
            idx.checked_sub(1)
        } else {
            (idx + 1 < slots.len()).then_some(idx + 1)
        };
        let Some(adj) = adj_idx.and_then(|i| slots.get(i)).cloned() else {
            return false;
        };
        let mut start = r.start.min(adj.start);
        let mut end = r.end.max(adj.end);
        let mut anchor = r.start;
        let mut head = adj.start;
        if let Some((removed_at, n)) = self.leave_blank() {
            let shift = |p: usize| {
                if p > removed_at {
                    p.saturating_sub(n).max(removed_at)
                } else {
                    p
                }
            };
            start = shift(start);
            end = shift(end);
            anchor = shift(anchor);
            head = shift(head);
        }
        let doc_len = self.doc().map_or(0, |d| d.text.len());
        start = start.min(doc_len);
        end = end.clamp(start, doc_len);
        self.active = None;
        self.block_sel = (start < end).then_some(start..end);
        self.block_sel_anchor = Some(anchor.min(doc_len));
        self.block_sel_head = Some(head.min(doc_len));
        self.scroll_to = Some(head.min(doc_len));
        true
    }

    // Ctrl+Home / Ctrl+End: 문서 맨 처음 / 맨 끝으로 이동
    fn jump_doc_edge(&mut self, to_end: bool) {
        let slots = self.doc_slots();
        if let Some(t) = if to_end { slots.last() } else { slots.first() }.cloned() {
            let caret = if to_end { t.end } else { t.start };
            self.go(t.start, caret);
        } else if let Some(doc) = self.doc() {
            let pos = if to_end { doc.text.len() } else { 0 };
            self.go(pos, pos);
        }
    }

    // Alt+↑ / Alt+↓: 현재 블록(또는 선택된 블록들)을 위/아래 블록과 자리 바꾸기
    fn move_blocks(&mut self, ctx: &egui::Context, up: bool) -> bool {
        let (sel_range, was_block_sel, caret_rel) = if let Some(bs) = self.block_sel.clone() {
            (bs, true, 0)
        } else if let Some((r, doc)) = self
            .active_block()
            .filter(|r| !r.is_empty())
            .zip(self.doc())
        {
            let local = self.local_cursor(ctx, doc, &r);
            (r.clone(), false, local.0)
        } else {
            return false;
        };
        let Some(doc) = self.doc_mut() else {
            return false;
        };
        let first_i = doc
            .blocks
            .iter()
            .position(|b| b.end > sel_range.start && b.start < sel_range.end)
            .or_else(|| doc.blocks.iter().position(|b| b.start == sel_range.start));
        let last_i = doc
            .blocks
            .iter()
            .rposition(|b| b.end > sel_range.start && b.start < sel_range.end)
            .or(first_i);
        let (Some(first_i), Some(last_i)) = (first_i, last_i) else {
            return false;
        };
        let (a_range, b_range) = if up {
            let Some(prev) = first_i
                .checked_sub(1)
                .and_then(|i| doc.blocks.get(i))
                .cloned()
            else {
                return false;
            };
            (prev, doc.blocks[first_i].start..doc.blocks[last_i].end)
        } else {
            let Some(next) = doc.blocks.get(last_i + 1).cloned() else {
                return false;
            };
            (doc.blocks[first_i].start..doc.blocks[last_i].end, next)
        };
        let a_str = doc.text[a_range.clone()].to_string();
        let b_str = doc.text[b_range.clone()].to_string();
        let replaced = format!("{b_str}\n\n{a_str}");
        doc.text
            .replace_range(a_range.start..b_range.end, &replaced);
        doc.edited_at = Some(Instant::now());
        doc.refresh_meta();
        let new_start = if up {
            a_range.start
        } else {
            a_range.start + b_str.len() + 2
        };
        let moved_len = if up { b_str.len() } else { a_str.len() };
        if was_block_sel {
            self.block_sel = Some(new_start..new_start + moved_len);
            self.block_sel_anchor = Some(new_start);
            self.block_sel_head = Some(new_start);
            self.scroll_to = Some(new_start);
        } else {
            let max_c = char_len(&doc.text[new_start..new_start + moved_len]);
            let caret = new_start
                + byte_idx(
                    &doc.text[new_start..new_start + moved_len],
                    caret_rel.min(max_c),
                );
            self.blank = 0;
            self.active = Some(new_start);
            self.pending_sel = None;
            self.pending_byte_caret = Some((new_start, caret));
            self.focus_editor = true;
            self.scroll_cursor = Some(None);
        }
        true
    }

    // Ctrl+D: 현재 블록(또는 다중 선택된 블록 구간)을 바로 아래에 복제
    fn duplicate_blocks(&mut self) -> bool {
        let (sel_range, was_block_sel) = if let Some(bs) = self.block_sel.clone() {
            (bs, true)
        } else if let Some(r) = self.active_block().filter(|r| !r.is_empty()) {
            (r, false)
        } else {
            return false;
        };
        let Some(doc) = self.doc_mut() else {
            return false;
        };
        let first_i = doc
            .blocks
            .iter()
            .position(|b| b.end > sel_range.start && b.start < sel_range.end);
        let last_i = doc
            .blocks
            .iter()
            .rposition(|b| b.end > sel_range.start && b.start < sel_range.end)
            .or(first_i);
        let (Some(first_i), Some(last_i)) = (first_i, last_i) else {
            return false;
        };
        let span = doc.blocks[first_i].start..doc.blocks[last_i].end;
        let chunk = doc.text[span.clone()].to_string();
        let insert_at = span.end;
        let ins = format!("\n\n{chunk}");
        doc.text.insert_str(insert_at, &ins);
        doc.edited_at = Some(Instant::now());
        doc.refresh_meta();
        let new_start = insert_at + 2;
        let new_end = new_start + chunk.len();
        if was_block_sel {
            self.block_sel = Some(new_start..new_end);
            self.block_sel_anchor = Some(new_start);
            self.block_sel_head = Some(new_start);
            self.scroll_to = Some(new_start);
        } else {
            self.blank = 0;
            self.active = Some(new_start);
            self.pending_sel = None;
            self.pending_byte_caret = Some((new_start, new_end));
            self.focus_editor = true;
            self.scroll_cursor = Some(None);
        }
        true
    }

    // 이미지 파일: 문서 폴더 안이면 상대 경로로, 밖이면 문서 폴더/images/ 로 복사 후 링크
    fn insert_image_file(&mut self, ctx: &egui::Context, src: &Path) {
        if self.current.is_none() {
            self.status = "이미지를 넣을 문서를 먼저 여세요".into();
            return;
        }
        let dir = self.doc_dir();
        let src = canon(src).unwrap_or_else(|| src.to_path_buf());
        let rel_inside = src
            .strip_prefix(&dir)
            .ok()
            .map(to_slash)
            .or_else(|| {
                // 작업 루트 안의 다른 하위 폴더 이미지면 상대 경로(../ 등) 계산
                let rel_src = src.strip_prefix(&self.root).ok()?;
                let rel_dir = dir.strip_prefix(&self.root).ok()?;
                let depth = rel_dir.components().count();
                let mut up = String::new();
                for _ in 0..depth {
                    up.push_str("../");
                }
                Some(format!("{up}{}", to_slash(rel_src)))
            })
            .filter(|r| !r.contains(['(', ')', '\n']));
        let link = match rel_inside {
            Some(rel) if !rel.contains(' ') => rel,
            Some(rel) => rel.replace(' ', "%20"),
            None => {
                let name =
                    sanitize_file_name(&src.file_name().unwrap_or_default().to_string_lossy());
                let dst = unique_path(&dir.join("images"), &name);
                let copied = std::fs::create_dir_all(dir.join("images"))
                    .and_then(|_| std::fs::copy(&src, &dst));
                if let Err(e) = copied {
                    self.status = format!("이미지 복사 실패: {e}");
                    return;
                }
                format!(
                    "images/{}",
                    dst.file_name().unwrap_or_default().to_string_lossy()
                )
            }
        };
        let alt = Path::new(&link)
            .file_stem()
            .map_or("image".into(), |s| pct_decode(&s.to_string_lossy()));
        self.insert(ctx, &format!("![{alt}]({link})"));
        self.status = format!("이미지 삽입: {link}");
    }

    // 클립보드 이미지(스크린샷) → 문서 폴더/images/paste-<시각>.png
    fn insert_pasted_image(&mut self, ctx: &egui::Context, img: &egui::ColorImage) {
        let Some(target_tab) = self.current.clone() else {
            self.status = "이미지를 넣을 문서를 먼저 여세요".into();
            return;
        };
        // PNG 인코딩·저장은 다른 스레드에서: 큰 스크린샷도 UI가 멈추지 않는다. 끝나면 poll_pasted가 링크를 넣는다.
        let (w, h) = (img.size[0] as u32, img.size[1] as u32);
        let pixels = img.pixels.clone();
        let dir = self.doc_dir().join("images");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let dst = unique_path(&dir, &format!("paste-{stamp}.png"));
        let (done, ctx) = (self.pasted.clone(), ctx.clone());
        std::thread::spawn(move || {
            let rgba: Vec<u8> = pixels
                .iter()
                .flat_map(|c| c.to_srgba_unmultiplied())
                .collect();
            let saved = image::RgbaImage::from_raw(w, h, rgba)
                .ok_or_else(|| "이미지 형식 오류".to_string())
                .and_then(|buf| {
                    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                    buf.save(&dst).map_err(|e| e.to_string())
                })
                .map(|()| {
                    dst.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                });
            if let Ok(mut q) = done.lock() {
                q.push((target_tab, saved));
            }
            ctx.request_repaint();
        });
    }

    // 백그라운드에서 저장이 끝난 붙여넣기 이미지를 원래 요청한 탭 문서에 넣는다
    fn poll_pasted(&mut self, ctx: &egui::Context) {
        let done: Vec<_> = self
            .pasted
            .lock()
            .map(|mut q| q.drain(..).collect())
            .unwrap_or_default();
        for (target_tab, r) in done {
            match r {
                Ok(name) => {
                    let md = format!("![image](images/{name})");
                    if self.current.as_deref() == Some(target_tab.as_str()) {
                        self.insert(ctx, &md);
                    } else if let Some(doc) = self.docs.get_mut(&target_tab) {
                        if !doc.text.is_empty() && !doc.text.ends_with('\n') {
                            doc.text.push_str("\n\n");
                        } else if doc.text.ends_with('\n') && !doc.text.ends_with("\n\n") {
                            doc.text.push('\n');
                        }
                        doc.text.push_str(&md);
                        doc.edited_at = Some(Instant::now());
                        doc.refresh_meta();
                    }
                    self.status = format!("스크린샷 붙여넣기: images/{name}");
                }
                Err(e) => self.status = format!("이미지 저장 실패: {e}"),
            }
        }
    }

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        for path in dropped {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if is_image_name(name) {
                self.insert_image_file(ctx, &path);
                if matches!(self.dialog, Some(Dialog::Image { .. })) {
                    self.dialog = None;
                }
            } else if path.is_dir() || is_markdown_name(name) {
                let rel = canon(&path)
                    .and_then(|p| p.strip_prefix(&self.root).ok().map(to_slash))
                    .filter(|r| is_markdown_name(r));
                match rel {
                    Some(r) => {
                        self.open_file(&r);
                    }
                    None => {
                        let (root, open) = resolve_target(Some(&path), &self.root);
                        self.guarded(After::Switch(root, open));
                    }
                }
            } else {
                self.status = format!("지원하지 않는 파일: {name}");
            }
        }
    }

    fn doc_scheme(&mut self) -> &str {
        let dir = self.doc_dir();
        if self.cached_scheme.0 != dir || self.cached_scheme.1.is_empty() {
            self.cached_scheme.1 = format!("file:///{}/", to_slash(&dir).trim_start_matches('/'));
            self.cached_scheme.0 = dir;
        }
        &self.cached_scheme.1
    }

    fn root_label(&mut self) -> &str {
        if self.cached_root_label.0 != self.root || self.cached_root_label.1.is_empty() {
            self.cached_root_label.1 = self
                .root
                .file_name()
                .and_then(|n| n.to_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| self.root.display().to_string());
            self.cached_root_label.0 = self.root.clone();
        }
        &self.cached_root_label.1
    }

    // 캔버스 링크 중 로컬 .md 문서와 문서 내 #제목 앵커는 앱 안에서 처리한다 (나머지 URL은 시스템 브라우저)
    fn handle_links(&mut self, ctx: &egui::Context) {
        let scheme_prefix = self.doc_scheme().to_string();
        let local: Vec<String> = ctx.output_mut(|o| {
            let mut taken = Vec::new();
            o.commands.retain(|c| match c {
                egui::OutputCommand::OpenUrl(u)
                    if !u.url.contains("://")
                        && !u
                            .url
                            .trim_start()
                            .to_ascii_lowercase()
                            .starts_with("mailto:") =>
                {
                    taken.push(u.url.clone());
                    false
                }
                egui::OutputCommand::OpenUrl(u)
                    if u.url.starts_with(&scheme_prefix)
                        && is_markdown_name(u.url.split('#').next().unwrap_or("")) =>
                {
                    taken.push(u.url[scheme_prefix.len()..].to_string());
                    false
                }
                egui::OutputCommand::OpenUrl(u) => external_url_allowed(&u.url),
                _ => true,
            });
            taken
        });
        for url in local {
            if let Some(anchor) = url.strip_prefix('#') {
                let target_slug = heading_slug(anchor);
                if let Some(byte_pos) = self.doc().and_then(|d| {
                    d.outline
                        .iter()
                        .find(|(_, title, _, _, _)| {
                            title.eq_ignore_ascii_case(anchor) || heading_slug(title) == target_slug
                        })
                        .map(|(_, _, _, _, b)| *b)
                }) {
                    self.scroll_to = Some(byte_pos);
                }
                continue;
            }
            let path = url.split('#').next().unwrap_or("");
            let path = pct_decode(path);
            if path.is_empty() {
                continue;
            }
            let rel = canon(&self.doc_dir().join(&path))
                .and_then(|p| p.strip_prefix(&self.root).ok().map(to_slash))
                .filter(|r| is_markdown_name(r));
            match rel {
                Some(r) => {
                    self.open_file(&r);
                }
                None => self.status = format!("열 수 없는 링크: {url}"),
            }
        }
    }

    fn auto_save_tick(
        &mut self,
        ctx: &egui::Context,
        storage: Option<&mut (dyn eframe::Storage + 'static)>,
    ) {
        self.poll_auto_save();
        let now = Instant::now();
        // 저장 안 된 문서가 있으면 변경이 있고 입력이 잠깐 멈춘 뒤 복구용 스냅샷을 갱신한다
        if !self.any_dirty_incl_scratch() {
            if !self.recovery_sig.is_empty() || self.recovery_at.take().is_some() {
                self.recovery_sig.clear();
                if let Ok(mut r) = RECOVERY.lock() {
                    r.clear();
                }
            }
        } else {
            let mut sig: Vec<(String, u64)> = self
                .docs
                .iter()
                .filter(|(_, d)| d.dirty())
                .map(|(n, d)| (n.clone(), d.rev))
                .collect();
            sig.sort_by(|a, b| a.0.cmp(&b.0));
            if sig != self.recovery_sig {
                let mut wait = Duration::ZERO;
                for d in self.docs.values().filter(|d| d.dirty()) {
                    if let Some(t) = d.edited_at {
                        let e = now.duration_since(t);
                        if e < Duration::from_millis(500) {
                            wait = wait.max(Duration::from_millis(500) - e);
                        }
                    }
                }
                if let Some(t) = self.recovery_at {
                    let e = now.duration_since(t);
                    if e < Duration::from_secs(2) {
                        wait = wait.max(Duration::from_secs(2) - e);
                    }
                }
                if wait.is_zero() {
                    self.recovery_at = Some(now);
                    self.recovery_sig = sig;
                    let snap: Vec<(PathBuf, String)> = self
                        .docs
                        .iter()
                        .filter(|(_, d)| d.dirty())
                        .map(|(n, d)| {
                            let path = if is_scratch_key(n) {
                                let tag = n.replace(':', "");
                                home()
                                    .join("Documents")
                                    .join("MDEditor")
                                    .join(format!("recovered-scratch{tag}.md"))
                            } else {
                                let mut p = self.root.join(n).into_os_string();
                                p.push(".recovered.md");
                                PathBuf::from(p)
                            };
                            (path, d.text.clone())
                        })
                        .collect();
                    if let Ok(mut r) = RECOVERY.lock() {
                        *r = snap;
                    }
                } else {
                    ctx.request_repaint_after(wait.max(Duration::from_millis(50)));
                }
            }
        }
        let mut due = Vec::new();
        let mut flush_draft = std::mem::take(&mut self.draft_dirty);
        for (name, doc) in &self.docs {
            if let (true, Some(t)) = (doc.dirty(), doc.edited_at) {
                let elapsed = now.duration_since(t);
                if elapsed >= AUTO_SAVE_DELAY {
                    if doc.scratch {
                        flush_draft = true;
                    } else if !self.auto_save_in_flight.contains(name) {
                        due.push(name.clone());
                    }
                } else {
                    ctx.request_repaint_after(AUTO_SAVE_DELAY - elapsed);
                }
            }
        }
        if flush_draft {
            let draft_body = self.draft_text().to_string();
            let drafts = self.drafts_joined();
            if let Some(st) = storage {
                st.set_string("draft", draft_body);
                st.set_string("drafts", drafts);
                st.flush();
            }
            for doc in self.docs.values_mut() {
                if doc.scratch {
                    doc.saved.clear();
                    doc.saved.push_str(&doc.text);
                    doc.edited_at = None;
                    doc.dirty_flag = false;
                }
            }
        }
        for name in due {
            self.submit_auto_save(ctx, &name);
        }
    }

    // Ctrl 조합 단축키. 처리했으면 true (이후 단축키 처리를 건너뛴다)
    fn shortcuts_cmd(&mut self, ctx: &egui::Context, ks: Keys) -> bool {
        let Keys {
            has_alt, focused, ..
        } = ks;
        let cmd = Modifiers::COMMAND;
        let shift_cmd = Modifiers::COMMAND | Modifiers::SHIFT;
        let alt_cmd = Modifiers::COMMAND | Modifiers::ALT;
        let k = |m, key| ctx.input_mut(|i| i.consume_key(m, key));
        // Shift / Alt 조합을 먼저 본다 (consume_key는 추가 Shift를 무시하므로)
        if k(shift_cmd, Key::O) {
            self.context_menu = None;
            self.pick_folder();
            return true;
        } else if has_alt && k(alt_cmd, Key::O) {
            self.context_menu = None;
            self.open_local_file();
            return true;
        } else if k(cmd, Key::O) {
            self.context_menu = None;
            self.open_files();
            return true;
        }
        if k(shift_cmd, Key::I) {
            self.context_menu = None;
            self.dialog = Some(Dialog::Image {
                url_tab: false,
                url: String::new(),
                alt: String::new(),
            });
            return true;
        }
        if k(shift_cmd, Key::N) {
            self.context_menu = None;
            self.dialog = Some(Dialog::NewFile(String::new()));
            return true;
        } else if has_alt && k(alt_cmd, Key::N) {
            self.context_menu = None;
            self.line_numbers = !self.line_numbers;
            return true;
        } else if (!has_alt && k(cmd, Key::N)) || (!has_alt && k(cmd, Key::T)) {
            self.context_menu = None;
            self.new_tab();
            return true;
        }
        if k(shift_cmd, Key::E) {
            self.context_menu = None;
            self.export();
            return true;
        }
        if k(shift_cmd, Key::L) {
            self.context_menu = None;
            self.toc_open = !self.toc_open;
            return true;
        }
        if k(shift_cmd, Key::F) {
            self.context_menu = None;
            self.focus_mode = !self.focus_mode;
            return true;
        }
        if k(shift_cmd, Key::C) {
            self.context_menu = None;
            self.copy_markdown_source(ctx);
            return true;
        }
        if has_alt {
            if k(alt_cmd, Key::C) {
                self.context_menu = None;
                self.copy_html_code(ctx);
                return true;
            }
            if k(alt_cmd, Key::D) {
                self.context_menu = None;
                self.dark = !self.dark;
                theme::apply(ctx, self.dark);
                return true;
            }
            if k(alt_cmd, Key::W) {
                self.context_menu = None;
                self.full_width = !self.full_width;
                return true;
            }
            if k(alt_cmd, Key::F) {
                self.context_menu = None;
                self.small_text = !self.small_text;
                return true;
            }
            if k(alt_cmd, Key::S) {
                self.context_menu = None;
                self.save_as_dialog();
                return true;
            }
            // 블록 전환 및 삽입 단축키 (Ctrl+Alt+0~9, G, T, M, -)
            if !self.find.open && self.current.is_some() {
                if k(alt_cmd, Key::B) {
                    self.context_menu = None;
                    self.format(ctx, Fmt::BrushCopy);
                    return true;
                }
                if k(alt_cmd, Key::P) {
                    self.context_menu = None;
                    self.format(ctx, Fmt::BrushPaste);
                    return true;
                }
                for (key, kind) in [
                    (Key::Num0, BlockKind::Paragraph),
                    (Key::Num1, BlockKind::Heading(1)),
                    (Key::Num2, BlockKind::Heading(2)),
                    (Key::Num3, BlockKind::Heading(3)),
                    (Key::Num4, BlockKind::Todo),
                    (Key::Num5, BlockKind::Bullet),
                    (Key::Num6, BlockKind::Numbered),
                    (Key::Num7, BlockKind::Quote),
                    (Key::Num8, BlockKind::Code),
                    (Key::Num9, BlockKind::Callout),
                    (Key::G, BlockKind::Toggle),
                ] {
                    if k(alt_cmd, key) {
                        self.context_menu = None;
                        self.convert_block(ctx, kind);
                        return true;
                    }
                }
                if k(alt_cmd, Key::T) {
                    self.context_menu = None;
                    self.insert_block_template("| 제목 | 내용 |\n| --- | --- |\n|  |  |", 2);
                    return true;
                }
                if k(alt_cmd, Key::M) {
                    self.context_menu = None;
                    self.insert_block_template("```mermaid\ngraph TD\n  A --> B\n```", 11);
                    return true;
                }
                if k(alt_cmd, Key::Minus) {
                    self.context_menu = None;
                    self.insert_block_template("---", 3);
                    return true;
                }
            }
        }
        if k(cmd, Key::W) {
            self.context_menu = None;
            if let Some(cur) = self.current.clone() {
                self.close_tab(&cur);
            }
            return true;
        }
        if k(shift_cmd, Key::Tab) || k(cmd, Key::PageUp) {
            self.context_menu = None;
            self.cycle_tab(-1);
            return true;
        } else if k(cmd, Key::Tab) || k(cmd, Key::PageDown) {
            self.context_menu = None;
            self.cycle_tab(1);
            return true;
        }
        for (i, key) in [
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
            Key::Num7,
            Key::Num8,
            Key::Num9,
        ]
        .into_iter()
        .enumerate()
        {
            if k(cmd, key) {
                self.context_menu = None;
                if let Some(target) = self.tabs.get(i).cloned() {
                    self.switch_to_tab(&target);
                }
                return true;
            }
        }
        // 서식 단축키 중 Shift 조합을 먼저 처리 (Ctrl+Shift+S 취소선, Ctrl+Shift+H 형광펜, Ctrl+Shift+K 키캡)
        let can_edit_blocks = !self.find.open && (focused || self.block_sel.is_some());
        if can_edit_blocks {
            if k(shift_cmd, Key::S) {
                self.context_menu = None;
                self.format(ctx, Fmt::Wrap("~~", "~~", "취소선"));
                return true;
            }
            if k(shift_cmd, Key::H) {
                self.context_menu = None;
                self.format(ctx, Fmt::Wrap("==", "==", "강조"));
                return true;
            }
            if k(shift_cmd, Key::K) {
                self.context_menu = None;
                self.format(ctx, Fmt::Wrap("[[", "]]", "Ctrl"));
                return true;
            }
            if k(shift_cmd, Key::D) {
                self.context_menu = None;
                self.delete_current_blocks();
                return true;
            }
            if k(shift_cmd, Key::Enter) {
                self.context_menu = None;
                self.insert_block_below();
                return true;
            }
            if k(cmd, Key::Enter) {
                self.context_menu = None;
                // 코드·수식·접기 블록 안에서는 할 일 대신 블록 바로 아래 새 문단으로 (닫는 줄은 숨겨져 있음)
                let fenced = self.active_block().filter(|r| {
                    self.doc()
                        .is_some_and(|d| fenced_body_range(&d.text[r.clone()]).is_some())
                });
                match fenced {
                    Some(r) => self.split(r.end),
                    None => self.toggle_todo(ctx),
                }
                return true;
            }
            if k(cmd, Key::E) {
                self.context_menu = None;
                self.format(ctx, Fmt::Wrap("`", "`", "code"));
                return true;
            }
            if k(cmd, Key::Backslash) {
                self.context_menu = None;
                self.format(ctx, Fmt::Clear);
                return true;
            }
        }
        if k(cmd, Key::S) {
            self.context_menu = None;
            self.trigger_save();
            return true;
        }
        if k(cmd, Key::H) {
            self.context_menu = None;
            self.open_find(ctx, true);
        } else if k(cmd, Key::F) {
            self.context_menu = None;
            self.open_find(ctx, false);
        }
        if k(cmd, Key::P) {
            self.context_menu = None;
            self.print(ctx);
        }
        if k(cmd, Key::Q) {
            self.context_menu = None;
            self.guarded(After::Close);
        }
        // 실행 취소: 찾기 입력창 등 다른 입력란에 포커스가 있으면 그쪽 기본 동작에 맡긴다
        if focused || ctx.memory(|m| m.focused().is_none()) {
            if k(shift_cmd, Key::Z) || k(cmd, Key::Y) {
                self.context_menu = None;
                self.history(false);
                return true;
            }
            if k(cmd, Key::Z) {
                self.context_menu = None;
                self.history(true);
                return true;
            }
        }
        // Ctrl+Home / Ctrl+End: 문서 맨 처음 / 맨 끝으로 이동
        if !self.find.open && self.current.is_some() {
            if k(cmd, Key::Home) {
                self.jump_doc_edge(false);
                return true;
            }
            if k(cmd, Key::End) {
                self.jump_doc_edge(true);
                return true;
            }
        }
        // Ctrl+A: 비활성 상태이거나 현재 블록이 이미 전부 선택된 상태면 문서 전체 선택
        if !self.find.open && ctx.input(|i| i.key_pressed(Key::A) && i.modifiers.command_only()) {
            let select_all_doc = if focused {
                let sel = self.cursor(ctx);
                match (self.active_block(), self.doc()) {
                    (Some(r), Some(doc)) if !r.is_empty() => {
                        let base = char_len(&doc.text[..r.start]);
                        let blen = char_len(&doc.text[r]);
                        sel.0 == base && sel.1 == base + blen
                    }
                    _ => true,
                }
            } else {
                true
            };
            if select_all_doc {
                k(cmd, Key::A);
                if let Some(len) = self.doc().map(|d| d.text.len()).filter(|&l| l > 0) {
                    self.leave_blank();
                    self.active = None;
                    self.block_sel = Some(0..len);
                    self.block_sel_anchor = Some(0);
                    self.block_sel_head = Some(len);
                }
                return true;
            }
        }
        false
    }

    // 다중 블록 / 문서 전체 선택 상태의 키 처리. 처리했으면 true
    fn shortcuts_block_sel(&mut self, ctx: &egui::Context, ks: Keys) -> bool {
        let Keys {
            has_cmd,
            has_alt,
            nav_or_edit,
            bs_del_tab,
            block_ev,
            ..
        } = ks;
        let cmd = Modifiers::COMMAND;
        let k = |m, key| ctx.input_mut(|i| i.consume_key(m, key));
        if let Some(bs) = self.block_sel.clone().filter(|_| !self.find.open) {
            if nav_or_edit {
                if k(Modifiers::SHIFT, Key::ArrowUp) && self.shift_arrow(ctx, true, false) {
                    return true;
                }
                if k(Modifiers::SHIFT, Key::ArrowDown) && self.shift_arrow(ctx, false, false) {
                    return true;
                }
                if k(Modifiers::SHIFT, Key::ArrowLeft) && self.shift_arrow(ctx, true, true) {
                    return true;
                }
                if k(Modifiers::SHIFT, Key::ArrowRight) && self.shift_arrow(ctx, false, true) {
                    return true;
                }
                if has_alt && k(Modifiers::ALT, Key::ArrowUp) {
                    self.move_blocks(ctx, true);
                    return true;
                }
                if has_alt && k(Modifiers::ALT, Key::ArrowDown) {
                    self.move_blocks(ctx, false);
                    return true;
                }
            }
            if has_cmd && k(cmd, Key::D) {
                self.duplicate_blocks();
                return true;
            }
            if nav_or_edit
                && (k(Modifiers::NONE, Key::ArrowUp)
                    || k(Modifiers::NONE, Key::ArrowLeft)
                    || k(Modifiers::NONE, Key::Home))
            {
                let start = self
                    .doc()
                    .and_then(|d| {
                        d.blocks
                            .iter()
                            .find(|b| b.end > bs.start && b.start < bs.end)
                    })
                    .map_or(bs.start, |b| b.start);
                self.go(start, start);
                return true;
            }
            if nav_or_edit
                && (k(Modifiers::NONE, Key::ArrowDown)
                    || k(Modifiers::NONE, Key::ArrowRight)
                    || k(Modifiers::NONE, Key::End))
            {
                let (start, end) = self
                    .doc()
                    .and_then(|d| {
                        d.blocks
                            .iter()
                            .rfind(|b| b.end > bs.start && b.start < bs.end)
                    })
                    .map_or((bs.end, bs.end), |b| (b.start, b.end));
                self.go(start, end);
                return true;
            }
            if bs_del_tab {
                if k(Modifiers::SHIFT, Key::Tab) {
                    self.outdent_active(ctx);
                    return true;
                } else if k(Modifiers::NONE, Key::Tab) {
                    self.indent_active(ctx);
                    return true;
                }
                if k(Modifiers::NONE, Key::Backspace) || k(Modifiers::NONE, Key::Delete) {
                    self.delete_block_sel();
                    return true;
                }
            }
            if nav_or_edit && k(Modifiers::NONE, Key::Enter) {
                self.delete_block_sel();
                return true;
            }
            if has_cmd || block_ev {
                let mut do_copy = has_cmd && k(cmd, Key::C);
                let mut do_cut = has_cmd && k(cmd, Key::X);
                let mut replace_with: Option<String> = None;
                ctx.input_mut(|i| {
                    i.events.retain(|e| match e {
                        egui::Event::Copy => {
                            do_copy = true;
                            false
                        }
                        egui::Event::Cut => {
                            do_cut = true;
                            false
                        }
                        egui::Event::Paste(s) => {
                            replace_with = Some(s.clone());
                            false
                        }
                        // 노션처럼 블록 선택 중 글자 입력은 무시 (Shift+↑ 한 번 뒤 한 글자로 블록 통째가 사라지지 않게)
                        egui::Event::Text(_) if !i.modifiers.command && !i.modifiers.ctrl => false,
                        _ => true,
                    });
                });
                if do_copy || do_cut {
                    if let (Some(bs), Some(doc)) = (&self.block_sel, self.doc()) {
                        let r = clamp_char_range(&doc.text, bs);
                        if r.start < r.end {
                            ctx.copy_text(doc.text[r].to_string());
                        }
                    }
                    if do_cut {
                        self.delete_block_sel();
                        return true;
                    }
                }
                if let Some(s) = replace_with {
                    self.replace_block_sel(&s);
                    return true;
                }
            }
            if has_cmd {
                if k(cmd, Key::B) {
                    self.format(ctx, Fmt::Wrap("**", "**", "굵게"));
                    return true;
                }
                if k(cmd, Key::I) {
                    self.format(ctx, Fmt::Wrap("*", "*", "기울임"));
                    return true;
                }
                if k(cmd, Key::K) {
                    self.format(ctx, Fmt::Wrap("[", "](https://)", "링크"));
                    return true;
                }
            }
        }
        false
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.dialog.is_some() {
            return;
        }
        if self.extra_carets.is_some() && self.multi_caret_keys(ctx) {
            return;
        }
        let has_block_sel = self.block_sel.is_some();
        let (
            any_key,
            has_cmd,
            has_alt,
            nav_or_edit,
            fn_key,
            esc,
            bs_del_tab,
            block_ev,
            has_ime_or_text,
            ime_still_composing,
        ) = ctx.input(|i| {
            let mut any_key = false;
            let mut has_cmd = i.modifiers.command;
            let mut has_alt = i.modifiers.alt;
            let mut nav_or_edit = false;
            let mut fn_key = false;
            let mut esc = false;
            let mut bs_del_tab = false;
            let mut block_ev = false;
            let mut has_ime_or_text = false;
            let mut has_ime_preedit = false;
            let mut ime_still_composing = self.ime_composing;
            for e in &i.events {
                match e {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => {
                        any_key = true;
                        has_cmd |= modifiers.command;
                        has_alt |= modifiers.alt;
                        match key {
                            Key::Enter
                            | Key::Home
                            | Key::End
                            | Key::ArrowUp
                            | Key::ArrowDown
                            | Key::ArrowLeft
                            | Key::ArrowRight => nav_or_edit = true,
                            Key::Backspace | Key::Delete | Key::Tab => bs_del_tab = true,
                            Key::Escape => esc = true,
                            Key::F1 | Key::F2 | Key::F3 | Key::F7 | Key::F11 => fn_key = true,
                            _ => {}
                        }
                    }
                    egui::Event::Ime(ime) => {
                        has_ime_or_text = true;
                        match ime {
                            egui::ImeEvent::Preedit { text, .. } => {
                                has_ime_preedit = !text.is_empty();
                                ime_still_composing = !text.is_empty();
                            }
                            _ => {
                                ime_still_composing = false;
                            }
                        }
                    }
                    egui::Event::PointerButton { pressed: true, .. } => {
                        if !has_ime_preedit {
                            ime_still_composing = false;
                        }
                    }
                    egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_)
                        if has_block_sel =>
                    {
                        block_ev = true;
                    }
                    egui::Event::Text(s) if !s.is_empty() && s != "\n" && s != "\r" => {
                        has_ime_or_text = true;
                        if !has_ime_preedit {
                            ime_still_composing = false;
                        }
                        if has_block_sel {
                            block_ev = true;
                        }
                    }
                    _ => {}
                }
            }
            (
                any_key,
                has_cmd,
                has_alt,
                nav_or_edit,
                fn_key,
                esc,
                bs_del_tab,
                block_ev,
                has_ime_or_text,
                ime_still_composing,
            )
        });
        if (!any_key && !block_ev)
            || (!has_cmd && !has_alt && !nav_or_edit && !fn_key && !esc && !bs_del_tab && !block_ev)
        {
            return;
        }

        let cmd = Modifiers::COMMAND;
        let k = |m, key| ctx.input_mut(|i| i.consume_key(m, key));
        let focused = self.editor_focused(ctx);
        let ks = Keys {
            has_cmd,
            has_alt,
            nav_or_edit,
            focused,
            bs_del_tab,
            block_ev,
        };

        if has_cmd && self.shortcuts_cmd(ctx, ks) {
            return;
        }

        if fn_key {
            if !self.find.query.is_empty() {
                if k(Modifiers::SHIFT, Key::F3) {
                    self.find_step(ctx, false);
                } else if k(Modifiers::NONE, Key::F3) {
                    self.find_step(ctx, true);
                }
            }
            if k(Modifiers::NONE, Key::F1) {
                self.context_menu = None;
                self.dialog = Some(Dialog::Help);
            }
            if k(Modifiers::NONE, Key::F2) {
                self.context_menu = None;
                if self.is_scratch() {
                    self.dialog = Some(Dialog::SaveScratch(String::new()));
                } else if let Some(n) = self.current.clone() {
                    self.ask_rename(&n);
                }
            }
            if k(Modifiers::NONE, Key::F11) {
                self.context_menu = None;
                self.focus_mode = !self.focus_mode;
            }
            if k(Modifiers::SHIFT, Key::F7) {
                self.context_menu = None;
                self.toggle_spellcheck();
            } else if k(Modifiers::NONE, Key::F7) {
                self.context_menu = None;
                self.open_spellcheck();
            }
        }

        if focused && (nav_or_edit || self.slash_sel != 0) {
            if let Some(q) = self.slash_query() {
                if nav_or_edit {
                    let n = slash_matches(&q).len().max(1);
                    if k(Modifiers::NONE, Key::ArrowDown) {
                        self.slash_sel = (self.slash_sel + 1) % n;
                    }
                    if k(Modifiers::NONE, Key::ArrowUp) {
                        self.slash_sel = (self.slash_sel + n - 1) % n;
                    }
                    if k(Modifiers::NONE, Key::Enter) {
                        if let Some(&i) = slash_matches(&q).get(self.slash_sel) {
                            self.apply_slash(i);
                        }
                        return;
                    }
                }
            } else {
                self.slash_sel = 0;
            }
        }
        if esc && k(Modifiers::NONE, Key::Escape) {
            if self.context_menu.is_some() {
                self.context_menu = None;
                self.canvas_sub = None;
                if self.active.is_some() {
                    self.focus_editor = true;
                }
            } else if self.find.open {
                self.find.open = false;
                if self.active.is_some() {
                    self.focus_editor = true;
                }
            } else if self.block_sel.is_some() {
                self.block_sel = None;
                self.block_sel_anchor = None;
                self.block_sel_head = None;
            } else {
                self.deactivate();
            }
        }

        // 다중 블록 / 문서 전체 선택 상태에서의 삭제·복사·잘라내기·붙여넣기·서식·이동·타이핑
        if self.shortcuts_block_sel(ctx, ks) {
            return;
        }

        if !focused {
            if !nav_or_edit && !bs_del_tab {
                return;
            }
            // 활성 블록에 선택 영역이 있거나 포커스가 잠시 빠진 경우 Tab / Shift+Tab / Backspace / Delete 처리
            if !self.find.open
                && self.active.is_some()
                && !self.is_visual_table_active()
                && bs_del_tab
            {
                if k(Modifiers::SHIFT, Key::Tab) {
                    self.outdent_active(ctx);
                    return;
                } else if k(Modifiers::NONE, Key::Tab) {
                    self.indent_active(ctx);
                    return;
                }
                let sel = self.cursor(ctx);
                if sel.0 < sel.1
                    && (k(Modifiers::NONE, Key::Backspace) || k(Modifiers::NONE, Key::Delete))
                {
                    if let Some(t) = self.doc().map(|d| replace_range(&d.text, sel, "")) {
                        self.set_text(t, (sel.0, sel.0));
                    }
                }
            } else if !self.find.open
                && self.active.is_none()
                && self.current.is_some()
                && nav_or_edit
                && ctx.memory(|m| m.focused().is_none())
            {
                if k(Modifiers::NONE, Key::Enter) {
                    self.append_block();
                } else if k(Modifiers::NONE, Key::ArrowDown) {
                    self.jump_doc_edge(false);
                } else if k(Modifiers::NONE, Key::ArrowUp) {
                    self.jump_doc_edge(true);
                }
            }
            return;
        }
        if has_cmd {
            if k(cmd, Key::D) && self.duplicate_blocks() {
                return;
            }
            if k(cmd, Key::B) {
                self.format(ctx, Fmt::Wrap("**", "**", "굵게"));
            }
            if k(cmd, Key::I) {
                self.format(ctx, Fmt::Wrap("*", "*", "기울임"));
            }
            if k(cmd, Key::K) {
                self.format(ctx, Fmt::Wrap("[", "](https://)", "링크"));
            }
        }
        if has_alt && nav_or_edit {
            if k(Modifiers::ALT, Key::ArrowUp) && self.move_blocks(ctx, true) {
                return;
            }
            if k(Modifiers::ALT, Key::ArrowDown) && self.move_blocks(ctx, false) {
                return;
            }
        }
        if self.is_visual_table_active() {
            return;
        }
        // Shift+Tab / Tab 은 한글 조합 직후·조합 중에도 즉시 들여쓰기/역들여쓰기 동작해야 한다
        if bs_del_tab {
            if k(Modifiers::SHIFT, Key::Tab) {
                self.outdent_active(ctx);
                return;
            } else if k(Modifiers::NONE, Key::Tab) {
                self.indent_active(ctx);
                return;
            }
        }
        // egui TextEdit은 조합 중인 음절(Preedit)을 항상 비어 있지 않은 선택 범위(sel.0 < sel.1)로 유지한다.
        // 선택 범위가 없다면(sel.0 == sel.1) 이미 조합이 끝난 상태이므로 잔여 플래그가 키 입력을 막지 않게 해제한다.
        let mut ime_still_composing = ime_still_composing;
        if ime_still_composing && !has_ime_or_text {
            let sel = self.cursor(ctx);
            if sel.0 == sel.1 {
                self.ime_composing = false;
                ime_still_composing = false;
            }
        }
        if ime_still_composing {
            if nav_or_edit && (has_ime_or_text || self.ime_composing) {
                let enter = ctx.input(|i| i.modifiers.is_none() && i.key_pressed(Key::Enter));
                if enter {
                    k(Modifiers::NONE, Key::Enter);
                    self.pending_enter = true;
                }
            }
            return;
        }
        if bs_del_tab {
            let (bs, del) = ctx.input(|i| {
                (
                    i.modifiers.is_none() && i.key_pressed(Key::Backspace),
                    i.modifiers.is_none() && i.key_pressed(Key::Delete),
                )
            });
            if bs
                && (self.delete_hidden(ctx, true)
                    || self.backspace_blank_above(ctx)
                    || self.backspace_list(ctx)
                    || self.backspace_at_start(ctx))
            {
                k(Modifiers::NONE, Key::Backspace);
            }
            if del
                && (self.delete_hidden(ctx, false)
                    || self.delete_empty_list(ctx)
                    || self.delete_at_end(ctx))
            {
                k(Modifiers::NONE, Key::Delete);
            }
        }
        if nav_or_edit {
            let (enter, s_enter, up, down, left, right, s_up, s_down, s_left, s_right) =
                ctx.input(|i| {
                    let none = i.modifiers.is_none();
                    let shift = i.modifiers.shift_only();
                    (
                        none && i.key_pressed(Key::Enter),
                        shift && i.key_pressed(Key::Enter),
                        none && i.key_pressed(Key::ArrowUp),
                        none && i.key_pressed(Key::ArrowDown),
                        none && i.key_pressed(Key::ArrowLeft),
                        none && i.key_pressed(Key::ArrowRight),
                        shift && i.key_pressed(Key::ArrowUp),
                        shift && i.key_pressed(Key::ArrowDown),
                        shift && i.key_pressed(Key::ArrowLeft),
                        shift && i.key_pressed(Key::ArrowRight),
                    )
                });
            if enter {
                if has_ime_or_text || self.ime_composing {
                    k(Modifiers::NONE, Key::Enter);
                    self.pending_enter = true;
                    return;
                } else if self.enter(ctx) {
                    k(Modifiers::NONE, Key::Enter);
                }
            } else if s_enter
                && self.active_block().is_some_and(|r| r.is_empty())
                && self.enter(ctx)
            {
                k(Modifiers::SHIFT, Key::Enter);
            }
            if s_up && self.shift_arrow(ctx, true, false) {
                k(Modifiers::SHIFT, Key::ArrowUp);
            } else if s_down && self.shift_arrow(ctx, false, false) {
                k(Modifiers::SHIFT, Key::ArrowDown);
            } else if s_left && self.shift_arrow(ctx, true, true) {
                k(Modifiers::SHIFT, Key::ArrowLeft);
            } else if s_right && self.shift_arrow(ctx, false, true) {
                k(Modifiers::SHIFT, Key::ArrowRight);
            }
            if up && self.arrow(ctx, true) {
                k(Modifiers::NONE, Key::ArrowUp);
            }
            if down && self.arrow(ctx, false) {
                k(Modifiers::NONE, Key::ArrowDown);
            }
            if left && self.arrow_lr(ctx, false) {
                k(Modifiers::NONE, Key::ArrowLeft);
            }
            if right && self.arrow_lr(ctx, true) {
                k(Modifiers::NONE, Key::ArrowRight);
            }
        }
    }

    // 다중 커서 중 키 처리. Esc는 추가 커서만 끈다. Backspace/Delete는 블록 병합 같은 특수 처리를 건너뛰고
    // TextEdit에 맡긴다(편집은 block_editor가 모든 커서에 복제). 이동·Enter·Tab·단축키는 추가 커서를 끄고 평소대로.
    // 반환값이 true면 나머지 단축키 처리를 건너뛴다.
    fn multi_caret_keys(&mut self, ctx: &egui::Context) -> bool {
        let (mut esc, mut clear, mut plain_edit) = (false, false, false);
        ctx.input(|i| {
            for e in &i.events {
                let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = e
                else {
                    continue;
                };
                match key {
                    Key::Escape => esc = true,
                    Key::Backspace | Key::Delete if modifiers.is_none() => plain_edit = true,
                    Key::V | Key::C if modifiers.command_only() => {}
                    Key::Enter
                    | Key::Tab
                    | Key::ArrowUp
                    | Key::ArrowDown
                    | Key::ArrowLeft
                    | Key::ArrowRight
                    | Key::Home
                    | Key::End
                    | Key::PageUp
                    | Key::PageDown
                    | Key::Backspace
                    | Key::Delete => clear = true,
                    _ if modifiers.command || modifiers.alt => clear = true,
                    _ => {}
                }
            }
        });
        if esc {
            ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
            self.extra_carets = None;
            return true;
        }
        if clear {
            self.extra_carets = None;
            return false;
        }
        plain_edit
    }

    fn open_find(&mut self, ctx: &egui::Context, replace: bool) {
        // 한 줄짜리 선택 영역이 있으면 그것을 검색어로
        let (a, b) = self.cursor(ctx);
        if let Some(sel) = self
            .doc()
            .filter(|_| b > a)
            .map(|d| &d.text[byte_idx(&d.text, a)..byte_idx(&d.text, b)])
            .filter(|s| !s.contains('\n') && s.len() <= 200)
        {
            self.find.query = sel.to_string();
        }
        self.find.open = true;
        self.find.replace = replace;
        self.find.focus = true;
    }

    fn find_step(&mut self, ctx: &egui::Context, forward: bool) {
        let (a, b) = self.cursor(ctx);
        let Some(name) = self.current.as_ref() else {
            return;
        };
        let Some(doc) = self.docs.get(name) else {
            return;
        };
        let matches = self.find.matches(doc);
        if matches.is_empty() {
            self.status = "일치 항목 없음".into();
            return;
        }
        let next = if forward {
            matches.iter().find(|m| m.0 >= b).or(matches.first())
        } else {
            matches.iter().rev().find(|m| m.1 <= a).or(matches.last())
        }
        .copied();
        self.leave_blank();
        self.block_sel = None;
        self.block_sel_anchor = None;
        self.block_sel_head = None;
        self.pending_byte_caret = None;
        self.pending_sel = next; // 찾기 입력창 포커스는 유지
        self.scroll_cursor = Some(Some(egui::Align::Center));
    }

    fn replace_one(&mut self, ctx: &egui::Context) {
        let (q, w, o) = (
            self.find.query.clone(),
            self.find.with.clone(),
            self.find.opts(),
        );
        self.edit(ctx, |t, sel| {
            let hit = find_replacements(t, &q, o, Some(&w))
                .ok()?
                .into_iter()
                .find(|m| m.0 == sel)?;
            let at = sel.0 + char_len(&hit.1);
            Some((replace_range(t, sel, &hit.1), (at, at)))
        });
        self.focus_editor = false;
        self.find_step(ctx, true);
    }

    fn replace_everything(&mut self, ctx: &egui::Context) {
        let (q, w, o) = (
            self.find.query.clone(),
            self.find.with.clone(),
            self.find.opts(),
        );
        let mut count = 0;
        self.edit(ctx, |t, _| {
            let m = find_replacements(t, &q, o, Some(&w)).ok()?;
            count = m.len();
            (!m.is_empty()).then(|| {
                (
                    replace_each(t, m.iter().map(|(r, s)| (*r, s.as_str()))),
                    (0, 0),
                )
            })
        });
        self.pending_sel = None;
        self.focus_editor = false;
        self.active = None;
        self.block_sel = None;
        self.blank = 0;
        self.status = format!("{count}개 바꿈");
    }

    fn export(&mut self) {
        let Some(name) = self.current.clone() else {
            return;
        };
        let stem = Path::new(&name)
            .file_stem()
            .map_or("document".into(), |s| s.to_string_lossy().into_owned());
        let Some(path) = rfd::FileDialog::new()
            .set_directory(self.doc_dir())
            .set_file_name(format!("{stem}.html"))
            .add_filter("HTML", &["html"])
            .save_file()
        else {
            return;
        };
        let html = export_html(&stem, &self.docs[&name].text);
        self.status = match save_atomic(&path, html.as_bytes()) {
            Ok(()) => format!("HTML 내보내기 완료: {}", path.display()),
            Err(e) => format!("HTML 내보내기 실패: {e}"),
        };
    }

    // 인쇄: 임시 HTML을 기본 브라우저로 열어 인쇄 대화상자를 띄운다 (웹 버전의 Ctrl+P와 같은 결과)
    fn print(&mut self, ctx: &egui::Context) {
        let (Some(name), Some(doc)) = (self.current.clone(), self.doc()) else {
            return;
        };
        let stem = Path::new(&name)
            .file_stem()
            .map_or("document".into(), |s| s.to_string_lossy().into_owned());
        let base = format!(
            "file:///{}/",
            pct_encode(to_slash(&self.doc_dir()).trim_start_matches('/'))
        );
        let html = export_html(&stem, &doc.text)
            .replacen("<head>", &format!("<head>\n<base href=\"{base}\">"), 1)
            .replacen(
                "</body>",
                "<script>window.onload = () => window.print();</script>\n</body>",
                1,
            );
        match save_atomic(&print_path(), html.as_bytes()) {
            Ok(()) => ctx.open_url(egui::OpenUrl::new_tab(print_url())),
            Err(e) => self.status = format!("인쇄 준비 실패: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_names_detected() {
        assert!(is_markdown_name("memo.md"));
        assert!(is_markdown_name("doc.MARKDOWN"));
        assert!(!is_markdown_name("memo.txt"));
        assert!(is_image_name("a.PNG"));
        assert!(!is_image_name("a.md"));
    }

    #[test]
    fn resolve_target_file_and_directory() {
        let temp = std::env::temp_dir().join("mdeditor_native_test_resolve");
        let sub = temp.join("folder");
        std::fs::create_dir_all(&sub).unwrap();
        let file = sub.join("doc.md");
        std::fs::write(&file, b"# Hello").unwrap();

        let (root, open) = resolve_target(Some(&sub), &temp);
        assert_eq!(root, canon(&sub).unwrap());
        assert!(!root.to_string_lossy().starts_with(r"\\?\"));
        assert_eq!(open, None);

        let (root, open) = resolve_target(Some(&file), &temp);
        assert_eq!(root, canon(&sub).unwrap());
        assert_eq!(open.as_deref(), Some("doc.md"));

        let (root, open) = resolve_target(None, &temp);
        assert_eq!(root, canon(&temp).unwrap());
        assert_eq!(open, None);

        std::fs::remove_dir_all(&temp).unwrap();
    }

    #[test]
    fn save_atomic_roundtrip_and_line_endings() {
        let dir = std::env::temp_dir().join("mdeditor_native_test_save");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.md");
        save_atomic(&p, "\u{feff}a\r\nb\r\n".as_bytes()).unwrap();
        let mut doc = Doc::load(&p).unwrap();
        assert_eq!(doc.text, "a\nb\n");
        assert!(!doc.dirty());
        doc.text.push_str("c\n");
        assert!(doc.dirty());
        save_atomic(&p, doc.encoded().as_bytes()).unwrap();
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            "\u{feff}a\r\nb\r\nc\r\n"
        );
        std::fs::write(&p, [0xff, 0xfe, 0x00]).unwrap();
        assert!(
            Doc::load(&p).is_err(),
            "UTF-8이 아니면 빈 문서로 위장하지 않는다"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn names_are_validated() {
        assert_eq!(validate_name("memo").unwrap(), "memo.md");
        assert_eq!(validate_name(r"notes\할일.md").unwrap(), "notes/할일.md");
        for bad in [
            "", "../x", "/abs", "C:x", "a/./b", "con", "LPT1.md", "a?.md", "x.",
        ] {
            assert!(validate_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn unique_and_sanitized_names() {
        assert_eq!(sanitize_file_name("my pic (1).png"), "my-pic--1-.png");
        assert_eq!(sanitize_file_name("스크린샷.png"), "스크린샷.png");
        let dir = std::env::temp_dir().join("mdeditor_native_test_unique");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.png"), b"x").unwrap();
        std::fs::write(dir.join("a-1.png"), b"x").unwrap();
        assert_eq!(unique_path(&dir, "a.png"), dir.join("a-2.png"));
        assert_eq!(unique_path(&dir, "b.png"), dir.join("b.png"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn wrap_and_line_prefix() {
        assert_eq!(
            wrap_selection("a 굵게 b", (2, 4), "**", "**", "x"),
            ("a **굵게** b".into(), (4, 6))
        );
        assert_eq!(
            wrap_selection("ab", (1, 1), "*", "*", "x"),
            ("a*x*b".into(), (2, 3))
        );
    }

    #[test]
    fn wrap_toggles_and_link_selects_url() {
        // 감싼 뒤 같은 선택으로 다시 누르면 해제된다
        let (s, sel) = wrap_selection("a b c", (2, 3), "**", "**", "굵게");
        assert_eq!((s.as_str(), sel), ("a **b** c", (4, 5)));
        let (s, sel) = wrap_selection(&s, sel, "**", "**", "굵게");
        assert_eq!((s.as_str(), sel), ("a b c", (2, 3)));
        // 기호까지 선택한 경우
        assert_eq!(
            wrap_selection("a **b** c", (2, 7), "**", "**", "").0,
            "a b c"
        );
        // 굵게 안에서 기울임은 별표를 벗기지 않고 더한다
        assert_eq!(wrap_selection("**b**", (2, 3), "*", "*", "").0, "***b***");
        // 링크는 바로 주소를 칠 수 있게 URL 자리를 선택한다
        let (s, sel) = wrap_selection("go", (0, 2), "[", "](https://)", "링크");
        assert_eq!((s.as_str(), sel), ("[go](https://)", (5, 13)));
    }

    #[test]
    fn undo_redo_restores_text_and_skips_invisible_steps() {
        let dir = std::env::temp_dir().join("mdeditor_undo_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("u.md");
        std::fs::write(&file, "가\n\n나\n").unwrap();
        let mut d = Doc::load(&file).unwrap();
        assert_eq!(d.step_history(true), None);
        // 이어 치는 입력은 한 단계로 묶인다
        for t in ["가!\n\n나\n", "가!!\n\n나\n"] {
            d.text = t.into();
            d.refresh_meta();
        }
        assert_eq!(d.undo.len(), 1);
        // 빈 블록을 만들며 넣는 줄바꿈은 보이지 않는 변경 → 실행 취소가 건너뛴다
        d.text.push_str("\n\n");
        d.refresh_meta();
        assert_eq!(d.undo.len(), 2);
        assert!(d.step_history(true).is_some());
        assert_eq!(d.text, "가\n\n나\n");
        assert!(d.step_history(false).is_some());
        assert!(d.text.starts_with("가!!\n\n나"), "{:?}", d.text);
        // 새로 편집하면 다시 실행 기록은 버린다
        d.text = "다".into();
        d.refresh_meta();
        assert!(d.redo.is_empty());
        assert_eq!(change_end("한글 abc", "한글 abXc"), "한글 abX".len());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn indent_and_outdent() {
        assert_eq!(
            indent_lines("a\nb", (0, 3), false),
            ("  a\n  b".into(), (2, 7))
        );
        assert_eq!(indent_lines("  a\n b", (0, 6), true).0, "a\nb");
        assert_eq!(indent_lines("- x", (3, 3), false), ("  - x".into(), (5, 5)));
    }

    // 실제 마크다운 파서가 보는 항목별 중첩 깊이
    fn item_depths(md: &str) -> Vec<usize> {
        use pulldown_cmark::{Event, Parser, Tag, TagEnd};
        let (mut d, mut out) = (0, vec![]);
        for ev in Parser::new(md) {
            match ev {
                Event::Start(Tag::List(_)) => d += 1,
                Event::End(TagEnd::List(_)) => d -= 1,
                Event::Start(Tag::Item) => out.push(d),
                _ => {}
            }
        }
        out
    }

    #[test]
    fn list_tab_builds_real_hierarchy() {
        let tab = |s: &str, line: usize| {
            let c = s
                .split('\n')
                .take(line)
                .map(|l| l.chars().count() + 1)
                .sum::<usize>();
            indent_list(s, (c, c), false).unwrap().0
        };
        let shift_tab = |s: &str, line: usize| {
            let c = s
                .split('\n')
                .take(line)
                .map(|l| l.chars().count() + 1)
                .sum::<usize>();
            indent_list(s, (c, c), true).unwrap().0
        };
        // 번호 목록: 3칸 들여써야 자식이 되고, 하위 번호는 1부터, 뒤 번호는 당겨진다
        let s = tab("1. AA\n2. BB\n3. CC", 1);
        assert_eq!(s, "1. AA\n   1. BB\n2. CC");
        assert_eq!(item_depths(&s), [1, 2, 1]);
        // 한 번 더 Tab을 누르면 한 단계 더(+2칸) 하위 항목으로 들여써지고 Shift+Tab으로 단계별 복귀한다
        let s2 = tab(&s, 1);
        assert_eq!(s2, "1. AA\n     1. BB\n2. CC");
        assert_eq!(shift_tab(&s2, 1), s);
        // 글머리 목록 3단계
        let s = tab(&tab("- a\n- b\n- c", 1), 2);
        assert_eq!(s, "- a\n  - b\n  - c");
        let s = tab(&s, 2);
        assert_eq!(s, "- a\n  - b\n    - c");
        assert_eq!(item_depths(&s), [1, 2, 3]);
        // 섞인 목록: 번호 아래 글머리
        assert_eq!(item_depths(&tab("1. a\n- b", 1)), [1, 2]);
        // 내어쓰기: 하위 항목도 같이 나오고, 번호는 부모 다음으로
        let s = "1. a\n   1. b\n      - c\n2. d";
        let o = shift_tab(s, 1);
        assert_eq!(o, "1. a\n2. b\n   - c\n3. d");
        assert_eq!(item_depths(&o), [1, 1, 2, 1]);
        // 편집 화면 깊이·번호는 렌더러와 같은 규칙 (번호 빈칸도 1, 2로 표시)
        let mut st = vec![];
        let got: Vec<_> = "1. a\n   - b\n1. c\n5. d"
            .split('\n')
            .map(|l| list_item_step(&mut st, l).unwrap())
            .collect();
        assert_eq!(got, [(0, Some(1)), (1, None), (0, Some(2)), (0, Some(3))]);
        // 숨긴 기호 안 커서 → 내용 시작 / 내용 시작에서 왼쪽 → 윗줄 끝
        assert_eq!(clamp_hidden("- a\n  - b", 4, None), 8);
        assert_eq!(clamp_hidden("- a\n  - b", 7, Some(8)), 3);
        assert_eq!(clamp_hidden("plain", 0, None), 0);
        // 인라인 기호: "a**bc**d" — 숨긴 ** 안에 커서가 머물지 않고, 제자리걸음 없이 한 글자씩 움직인다
        let s = "a**bc**d";
        assert_eq!(clamp_hidden(s, 2, Some(1)), 4, "a| → → 는 b 뒤로");
        assert_eq!(clamp_hidden(s, 2, Some(3)), 0, "|b 에서 ← 는 a 앞으로");
        assert_eq!(clamp_hidden(s, 6, Some(5)), 8, "c| → → 는 d 뒤로");
        assert_eq!(clamp_hidden(s, 6, None), 7);
        // 제목 기호도 숨김: Home → 내용 시작
        assert_eq!(clamp_hidden("## 제목", 0, Some(5)), 3);
        // 숨긴 ** 옆 Backspace/Delete는 보이는 글자를 지우고, 마지막 한 글자면 서식째 지운다
        assert_eq!(
            delete_near_hidden(s, 7, true),
            Some(Ok(("a**b**d".into(), 4)))
        );
        assert_eq!(delete_near_hidden(s, 3, true), Some(Err(1)));
        assert_eq!(
            delete_near_hidden(s, 1, false),
            Some(Ok(("a**c**d".into(), 3)))
        );
        assert_eq!(
            delete_near_hidden("x**y**", 6, true),
            Some(Ok(("x".into(), 1)))
        );
        assert_eq!(delete_near_hidden("plain", 2, true), None);
        // 제목 맨 앞 Backspace → 일반 문단
        assert_eq!(
            backspace_list_marker("## 제목", 3),
            Some(("제목".into(), 0))
        );

        // 편집 화면에서 숨기는 줄머리: 인용·알림 상자 제목(줄 전체)·구분선(줄 전체)·각주 정의·접기 제목
        assert_eq!(hidden_prefix("> 인용"), 2);
        assert_eq!(hidden_prefix("> [!NOTE]"), "> [!NOTE]".len());
        assert_eq!(hidden_prefix("---"), 3);
        assert_eq!(hidden_prefix("[^1]: 설명"), 6);
        assert_eq!(hidden_prefix(":::toggle 제목"), 10);
        assert_eq!(hidden_prefix("보통 문단"), 0);
        // 링크는 글자만, 각주 참조는 "^"만 숨긴다
        let line = "a [구글](https://g.com) b[^1]";
        let spans: Vec<_> = inline_spans(line).into_iter().map(|t| t.4).collect();
        assert_eq!(spans, [Inline::Link, Inline::FootRef]);
        let s = "x [ab](u) y";
        assert_eq!(
            clamp_hidden(s, 3, Some(2)),
            4,
            "링크 앞 [ 안에 머물지 않는다"
        );
        assert_eq!(
            clamp_hidden(s, 6, Some(5)),
            10,
            "b 뒤에서 → 는 숨긴 ](u)를 건너 다음 글자 뒤로"
        );
        assert_eq!(
            delete_near_hidden("a[^1] b", 5, true),
            Some(Ok(("a b".into(), 1)))
        );
        // 인용·알림 상자·접기 맨 앞 Backspace (노션식)
        assert_eq!(
            backspace_list_marker("> a\n> b", 2),
            Some(("a\nb".into(), 0))
        );
        assert_eq!(
            backspace_list_marker("> a\n> b", 6),
            Some(("> ab".into(), 3))
        );
        assert_eq!(
            backspace_list_marker("> [!NOTE]\n> 내용", 12),
            Some(("내용".into(), 0))
        );
        assert_eq!(
            backspace_list_marker(":::toggle 제목\n안\n:::", 10),
            Some(("제목\n안".into(), 0))
        );
        // 수식·접기 블록도 숨긴 앞뒤 줄을 지킨다
        assert_eq!(fenced_body_range("$$\nE=1\n$$"), Some((3, 6)));
        assert_eq!(fenced_body_range(":::toggle 제목\n안\n:::"), Some((10, 14)));
        assert_eq!(
            keep_code_fences("$$\nE=1\n$$", "x"),
            Some(("$$\nx\n$$".into(), 4))
        );
        assert_eq!(visible_bounds("> [!NOTE]\n> 내용"), (12, 14));
        // 내용 맨 앞 Backspace: 하위 → 내어쓰기, 첫 항목 → 문단, 그 외 → 윗줄과 합치기
        assert_eq!(
            backspace_list_marker("- a\n  - b", 8),
            Some(("- a\n- b".into(), 6))
        );
        assert_eq!(backspace_list_marker("- a", 2), Some(("a".into(), 0)));
        assert_eq!(
            backspace_list_marker("- a\n- b", 6),
            Some(("- ab".into(), 3))
        );
        assert_eq!(backspace_list_marker("- ab", 3), None);
        // 코드 블록: 숨긴 ``` 줄은 지켜지고 코드 변경은 유지
        let code = "```sas\ndata a;\n```";
        assert_eq!(code_body_range(code), Some((7, 14)));
        assert_eq!(keep_code_fences(code, "```sas\ndata b;\n```"), None);
        assert_eq!(
            keep_code_fences(code, "```sasdata a;\n```"),
            Some((code.into(), 7)),
            "맨 앞 Backspace는 펜스를 못 지운다"
        );
        assert_eq!(
            keep_code_fences(code, "x"),
            Some(("```sas\nx\n```".into(), 8)),
            "전체 선택 후 입력은 코드만 바꾼다"
        );
        assert_eq!(
            keep_code_fences("```\n```", "```\nQ```"),
            Some(("```\nQ\n```".into(), 5)),
            "빈 코드 블록에 친 글자는 닫는 ``` 앞 줄로"
        );
        // 첫 항목도 Tab으로 하위 항목(2칸 들여쓰기)이 되고, 목록 아닌 줄은 None
        assert_eq!(tab("- a", 0), "  - a");
        assert_eq!(shift_tab("  - a", 0), "- a");
        assert!(indent_list("plain", (0, 0), false).is_none());
    }

    #[test]
    fn smart_enter_continues_and_ends_lists() {
        assert_eq!(smart_enter("- a", 3), Some(("- a\n- ".into(), 6)));
        assert_eq!(
            smart_enter("  9. a", 6),
            Some(("  9. a\n  10. ".into(), 13))
        );
        assert_eq!(
            smart_enter("- [x] 끝", 7),
            Some(("- [x] 끝\n- [ ] ".into(), 14))
        );
        assert_eq!(
            smart_enter("x\n- ", 4),
            Some(("x\n".into(), 2)),
            "빈 항목은 목록 종료"
        );
        assert_eq!(smart_enter("plain", 5), None);
        assert_eq!(smart_enter("- a", 1), None, "기호 안쪽은 일반 줄바꿈");
    }

    #[test]
    fn find_and_replace() {
        assert_eq!(
            find_all("Ab ab AB", "ab", false),
            vec![(0, 2), (3, 5), (6, 8)]
        );
        assert_eq!(find_all("Ab ab AB", "ab", true), vec![(3, 5)]);
        assert_eq!(find_all("한글 한글", "글", true), vec![(1, 2), (4, 5)]);
        assert!(find_all("x", "", false).is_empty());
        let m = find_all("aXbX", "X", true);
        assert_eq!(replace_all("aXbX", &m, "yy"), "ayybyy");

        // 단어 단위·정규식 (문자 인덱스, 한글 포함)
        let o = |case, word, regex| FindOpts { case, word, regex };
        let sels = |s: &str, q: &str, f| -> Vec<Sel> {
            find_replacements(s, q, f, None)
                .unwrap()
                .into_iter()
                .map(|x| x.0)
                .collect()
        };
        assert_eq!(
            sels("cat category Cat", "cat", o(false, true, false)),
            vec![(0, 3), (13, 16)]
        );
        assert_eq!(
            sels("한글 한글을 글", "한글", o(true, true, false)),
            vec![(0, 2)]
        );
        assert_eq!(sels("a.b axb", "a.b", o(true, true, false)), vec![(0, 3)]); // 단어 단위만이면 . 은 글자 그대로
        assert_eq!(
            sels("가1 나22", r"\d+", o(true, false, true)),
            vec![(1, 2), (4, 6)]
        );
        assert_eq!(sels("x\ny", "^", o(true, false, true)), vec![]); // 길이 0 일치는 건너뜀
        assert_eq!(sels("ab\ncd", "^c", o(true, false, true)), vec![(3, 4)]); // 줄 단위 ^
        assert_eq!(
            find_replacements("a", "(", o(true, false, true), None).unwrap_err(),
            "정규식 오류"
        );
        // 캡처 치환은 정규식 모드에서만, 단어 단위는 $도 글자 그대로
        let r = find_replacements(
            "홍 길동, 김 철수",
            r"(\S+) (\S+)",
            o(true, false, true),
            Some("$2 $1"),
        )
        .unwrap();
        let s = "홍 길동, 김 철수";
        assert_eq!(
            replace_each(s, r.iter().map(|(m, w)| (*m, w.as_str()))),
            "길동, 홍 철수 김"
        );
        let r = find_replacements("a b", "a", o(true, true, false), Some("$1")).unwrap();
        assert_eq!(r[0].1, "$1");
    }

    #[test]
    fn outline_skips_code() {
        let o = outline("# A\n```\n# no\n```\n## B ##\n#bad");
        let titles: Vec<_> = o.iter().map(|x| (x.0, x.1.as_str(), x.2)).collect();
        assert_eq!(titles, vec![(1, "A", 0), (2, "B", 4)]);
        assert_eq!(o[1].3, 17);
    }

    #[test]
    fn preprocess_preserves_length_and_maps_extensions() {
        let src = ":::toggle 제목\n==형광== [[Ctrl]] `==x==`\n```\n==code==\n```\n:::\n- [ ] 할 일";
        let out = preprocess(src);
        assert_eq!(out.len(), src.len());
        assert!(out.starts_with("##### ▸ 제목\n**형광** ``Ctrl`` `==x==`"));
        assert!(out.contains("\n==code==\n"));

        // 미리보기 체크박스 토글이 원문 같은 위치에 반영된다
        let mut text = src.to_string();
        let toggled = out.replacen("- [ ]", "- [x]", 1);
        assert!(apply_preview_change(&mut text, &out, &toggled));
        assert!(text.ends_with("- [x] 할 일") && text.starts_with(":::toggle"));
    }

    #[test]
    fn exported_html_matches_preview_syntax() {
        let html = export_html(
            "a<b",
            "![a](images/p.png)\n\n==형광== [[Ctrl]]\n\n- [x] 끝\n\n> [!NOTE]\n> 메모",
        );
        assert!(html.contains("src=\"images/p.png\""), "{html}");
        assert!(html.contains("<title>a&lt;b"));
        assert!(html.contains("<strong>형광</strong> <code>Ctrl</code>"));
        assert!(html.contains("checked"));
        assert!(html.contains("markdown-alert-note"));
    }

    #[test]
    fn blocks_split_on_blank_lines_but_keep_fences_together() {
        let s =
            "# T\npara\nline2\n\n```\na\n\nb\n```\n- x\n- y\n\n:::toggle t\n\nin\n:::\n$$\nx\n$$\n";
        let got: Vec<&str> = split_blocks(s).into_iter().map(|r| &s[r]).collect();
        assert_eq!(
            got,
            vec![
                "# T",
                "para\nline2",
                "```\na\n\nb\n```",
                "- x\n- y",
                ":::toggle t\n\nin\n:::",
                "$$\nx\n$$"
            ]
        );
        assert!(split_blocks("").is_empty());
        assert_eq!(split_blocks("a\n\n\n\nb"), vec![0..1, 5..6]);
        assert_eq!(heading_level("## x"), Some(2));
        assert_eq!(heading_level("#tag"), None);
    }

    // 헤드리스 UI 테스트: 실제 앱에 클릭·키 입력을 보내 노션식 블록 편집을 검증한다
    #[test]
    fn canvas_block_editing_flow() {
        use egui_kittest::kittest::Queryable;
        let dir = std::env::temp_dir().join("mdeditor_ui_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("doc.md");
        std::fs::write(
            &file,
            "# 제목

첫 문단

- [ ] 할 일
",
        )
        .unwrap();
        let (f2, d2) = (file.clone(), dir.clone());
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1000.0, 800.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, Some(f2), &d2));
        let text = |h: &egui_kittest::Harness<App>| h.state().doc().unwrap().text.clone();
        h.run_steps(3);

        // 1. 렌더링된 문단을 클릭하면 그 블록이 편집 상태가 되고 클릭 지점에 커서가 놓인다
        h.get_by_label("첫 문단").click();
        h.run_steps(3);
        assert_eq!(
            h.state().active,
            Some(
                "# 제목

"
                .len()
            )
        );

        // 2. 입력은 클릭한 위치에 들어가며, End 후 입력하면 블록 끝에 붙는다
        h.event(egui::Event::Text("!".into()));
        h.run_steps(3);
        assert_eq!(
            text(&h),
            "# 제목

첫 !문단

- [ ] 할 일
"
        );
        h.key_press(egui::Key::Backspace);
        h.run_steps(2);
        h.key_press(egui::Key::End);
        h.run_steps(2);
        h.event(egui::Event::Text("!".into()));
        h.run_steps(3);
        assert_eq!(
            text(&h),
            "# 제목

첫 문단!

- [ ] 할 일
"
        );

        // 3. Enter는 블록을 나누고 새 빈 블록을 연다
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        assert_eq!(
            text(&h),
            "# 제목

첫 문단!



- [ ] 할 일
"
        );
        assert_eq!(h.state().blank, 2);

        // 4. 슬래시 메뉴: /제목 → ↓ → Enter = 제목 2
        h.event(egui::Event::Text("/제목".into()));
        h.run_steps(3);
        assert_eq!(h.state().slash_query().as_deref(), Some("제목"));
        h.key_press(egui::Key::ArrowDown);
        h.run_steps(2);
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        h.event(egui::Event::Text("소제목".into()));
        h.run_steps(3);
        assert_eq!(
            text(&h),
            "# 제목

첫 문단!

## 소제목

- [ ] 할 일
"
        );

        // 5. Esc로 편집을 끝내고, 렌더링된 체크박스 클릭은 원문에 반영된다
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        assert_eq!(h.state().active, None);
        h.get_by_role(egui::accesskit::Role::CheckBox).click();
        h.run_steps(3);
        assert!(
            text(&h).ends_with(
                "- [x] 할 일
"
            ),
            "{}",
            text(&h)
        );

        // 6. 빈 블록을 비운 채 떠나면 넣었던 빈 줄이 되돌려진다
        h.get_by_label("첫 문단!").click();
        h.run_steps(3);
        h.key_press(egui::Key::End);
        h.run_steps(2);
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        assert_eq!(
            text(&h),
            "# 제목

첫 문단!

## 소제목

- [x] 할 일
"
        );

        // 7. 제목 맨 앞 Backspace는 노션처럼 먼저 일반 문단으로 바꾸고, 한 번 더 누르면 앞 블록과 합친다
        h.get_by_label("소제목").click();
        h.run_steps(3);
        h.key_press(egui::Key::Home);
        h.run_steps(2);
        h.key_press(egui::Key::Backspace);
        h.run_steps(3);
        assert_eq!(
            text(&h),
            "# 제목

첫 문단!

소제목

- [x] 할 일
"
        );
        h.key_press(egui::Key::Backspace);
        h.run_steps(3);
        assert_eq!(
            text(&h),
            "# 제목

첫 문단!소제목

- [x] 할 일
"
        );

        // 8. Ctrl+Z는 블록 합치기 같은 구조 편집도 되돌리고, Ctrl+Y로 다시 실행한다
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
        h.run_steps(3);
        assert_eq!(
            text(&h),
            "# 제목

첫 문단!

소제목

- [x] 할 일
"
        );
        assert!(h.state().active.is_some());
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Y);
        h.run_steps(3);
        assert!(text(&h).contains("첫 문단!소제목"), "{}", text(&h));
        // 끝까지 되돌리면 처음 파일 내용으로 돌아온다
        for _ in 0..20 {
            h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
            h.run_steps(2);
        }
        assert_eq!(
            text(&h),
            "# 제목

첫 문단

- [ ] 할 일
"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn block_selection_and_deletion_flow() {
        use egui_kittest::kittest::Queryable;
        let dir = std::env::temp_dir().join("mdeditor_ui_test_sel_del");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("doc.md");
        std::fs::write(
            &file,
            "# 제목\n\n첫째 줄\n둘째 줄\n\n중간 문단\n\n마지막 문단\n",
        )
        .unwrap();
        let (f2, d2) = (file.clone(), dir.clone());
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1000.0, 800.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, Some(f2), &d2));
        let text = |h: &egui_kittest::Harness<App>| h.state().doc().unwrap().text.clone();
        h.run_steps(3);

        // 1. 여러 줄 블록에서 첫째 줄만 지웠을 때 활성 블록에서 튕기지 않고 그대로 편집 유지
        let multi = "# 제목\n\n".len();
        h.state_mut().go(multi, multi + "첫째 줄".len());
        h.run_steps(2);
        // 첫째 줄("첫째 줄") 전체 선택 후 Backspace
        h.state_mut().pending_sel = Some((char_len("# 제목\n\n"), char_len("# 제목\n\n첫째 줄")));
        h.run_steps(2);
        h.key_press(egui::Key::Backspace);
        h.run_steps(3);
        assert_eq!(text(&h), "# 제목\n\n둘째 줄\n\n중간 문단\n\n마지막 문단\n");
        assert_eq!(h.state().active, Some(multi));

        // 2. 다중 블록 선택(block_sel) 후 Delete 키로 한 번에 삭제
        let mid_start = "# 제목\n\n".len();
        let mid_end = "# 제목\n\n둘째 줄\n\n중간 문단".len();
        h.state_mut().deactivate();
        h.state_mut().block_sel = Some(mid_start..mid_end);
        h.run_steps(2);
        h.key_press(egui::Key::Delete);
        h.run_steps(3);
        assert_eq!(text(&h), "# 제목\n\n마지막 문단\n");

        // 3. 비활성 블록 위에서 마우스 드래그로 범위 지정 후 Backspace 삭제
        h.state_mut().deactivate();
        h.run_steps(2);
        let node_rect = h.get_by_label("마지막 문단").rect();
        let start_pos = egui::pos2(node_rect.left() + 2.0, node_rect.center().y);
        let end_pos = egui::pos2(node_rect.right() + 30.0, node_rect.center().y);
        h.input_mut()
            .events
            .push(egui::Event::PointerMoved(start_pos));
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: start_pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
        h.input_mut()
            .events
            .push(egui::Event::PointerMoved(end_pos));
        h.step();
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: end_pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(3);
        assert!(h.state().active.is_some());
        h.key_press(egui::Key::Backspace);
        h.run_steps(3);
        // 블록 안의 텍스트가 지워지고 Esc를 누르면 빈 줄까지 깔끔히 정리된다
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        assert_eq!(text(&h), "# 제목\n");

        // 4. Ctrl+A로 전체 문서 블록지정 후 Backspace로 전체 삭제
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        h.run_steps(2);
        assert_eq!(h.state().block_sel, Some(0..text(&h).len()));
        h.key_press(egui::Key::Backspace);
        h.run_steps(3);
        assert_eq!(text(&h), "");
        assert_eq!(h.state().active, Some(0));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn memo_mode_opens_scratch_and_persists_draft() {
        #[derive(Default)]
        struct MemStorage(HashMap<String, String>);
        impl eframe::Storage for MemStorage {
            fn get_string(&self, key: &str) -> Option<String> {
                self.0.get(key).cloned()
            }
            fn set_string(&mut self, key: &str, value: String) {
                self.0.insert(key.to_string(), value);
            }
            fn remove_string(&mut self, key: &str) {
                self.0.remove(key);
            }
            fn flush(&mut self) {}
        }

        let dir = std::env::temp_dir().join("mdeditor_ui_test_memo");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("existing.md"), "# 기존 파일\n").unwrap();

        let mut store = MemStorage::default();
        store
            .0
            .insert("root".into(), dir.to_string_lossy().into_owned());
        store.0.insert("file".into(), "existing.md".into());

        // 1. 인자 없이 실행하면 기존 파일이 있어도 항상 새 문서(임시 메모)로 시작한다
        let d1 = dir.clone();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1000.0, 800.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, None, &d1));
        h.run_steps(2);
        assert!(h.state().is_scratch());
        assert_eq!(h.state().doc().unwrap().text, "");
        assert_eq!(h.state().active, Some(0));

        // 2. 저장 없이 메모를 작성하고 종료(App::save)해도 확인 창 없이 draft에 유지된다
        h.event(egui::Event::Text("임시 메모 테스트".into()));
        h.run_steps(2);
        assert_eq!(h.state().doc().unwrap().text, "임시 메모 테스트");
        assert!(!h.state().any_dirty());
        eframe::App::save(h.state_mut(), &mut store);
        assert_eq!(
            store.0.get("draft").map(String::as_str),
            Some("임시 메모 테스트")
        );
        assert_eq!(store.0.get("file").map(String::as_str), Some(""));

        // 3. 재실행 시 저장하지 않았던 메모가 그대로 복원된다 (기존 storage 시뮬레이션)
        let saved_draft = store.0.get("draft").cloned().unwrap();
        let d2 = dir.clone();
        let mut h2 = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1000.0, 800.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| {
                let mut app = App::new(cc, None, &d2);
                app.open_scratch(saved_draft);
                app
            });
        h2.run_steps(2);
        assert!(h2.state().is_scratch());
        assert_eq!(h2.state().doc().unwrap().text, "임시 메모 테스트");

        // 4. Ctrl+N으로 새 편집 탭 추가 후 Ctrl+W로 닫으면 이전 메모 탭으로 돌아온다
        h2.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::N);
        h2.run_steps(2);
        assert_eq!(h2.state().tabs.len(), 2);
        assert!(h2.state().is_scratch());
        assert_eq!(h2.state().doc().unwrap().text, "");
        h2.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::W);
        h2.run_steps(2);
        assert_eq!(h2.state().tabs.len(), 1);
        assert_eq!(h2.state().doc().unwrap().text, "임시 메모 테스트");

        // 5. 파일로 저장(save_scratch_as)하면 실제 .md 파일이 생성되고 draft는 비워진다
        h2.state_mut().save_scratch_as("saved_memo").unwrap();
        assert!(!h2.state().is_scratch());
        assert_eq!(h2.state().current.as_deref(), Some("saved_memo.md"));
        assert_eq!(
            std::fs::read_to_string(dir.join("saved_memo.md")).unwrap(),
            "임시 메모 테스트"
        );
        assert_eq!(h2.state().draft_text(), "");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn patch_history_and_fast_find_work_accurately() {
        let mut doc = Doc::scratch("초기 텍스트".into());
        doc.text.push('A');
        doc.refresh_meta();
        doc.text.push('B');
        doc.refresh_meta();
        // 연속된 단일 문자 타이핑은 하나의 실행 취소 단위로 병합
        assert_eq!(doc.undo.len(), 1);
        doc.step_history(true);
        assert_eq!(doc.text, "초기 텍스트");
        doc.step_history(false);
        assert_eq!(doc.text, "초기 텍스트AB");

        // 대소문자 구분 / ASCII 대소문자 무시 / 유니코드 검색 검증
        let s = "Hello 한글 HELLO world hello";
        assert_eq!(find_all(s, "Hello", true), vec![(0, 5)]);
        assert_eq!(find_all(s, "hello", false), vec![(0, 5), (9, 14), (21, 26)]);
        assert_eq!(find_all(s, "한글", false), vec![(6, 8)]);
    }

    #[test]
    fn multi_tab_editing_switching_and_closing() {
        use egui_kittest::kittest::Queryable;
        let dir = std::env::temp_dir().join("mdeditor_ui_test_multitab");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("note.md"), "파일 탭 내용\n").unwrap();

        let d1 = dir.clone();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1000.0, 800.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, None, &d1));
        h.run_steps(2);

        // 첫 번째 탭 편집
        h.event(egui::Event::Text("첫 탭 메모".into()));
        h.run_steps(2);
        let tab1 = h.state().current.clone().unwrap();

        // Ctrl+T로 두 번째 새 탭 추가 후 편집
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::T);
        h.run_steps(2);
        assert_eq!(h.state().tabs.len(), 2);
        let tab2 = h.state().current.clone().unwrap();
        assert_ne!(tab1, tab2);
        h.event(egui::Event::Text("둘째 탭 메모".into()));
        h.run_steps(2);

        // 실제 파일 탭 열기 -> 총 3개 탭
        h.state_mut().open_file("note.md");
        h.run_steps(2);
        assert_eq!(h.state().tabs.len(), 3);
        assert_eq!(h.state().current.as_deref(), Some("note.md"));
        assert_eq!(h.state().doc().unwrap().text, "파일 탭 내용\n");

        // 탭 바 UI에서 첫 번째 탭 제목("메모")을 클릭하거나 Ctrl+1로 전환
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num1);
        h.run_steps(2);
        assert_eq!(h.state().current.as_deref(), Some(tab1.as_str()));
        assert_eq!(h.state().doc().unwrap().text, "첫 탭 메모");

        // Ctrl+Tab으로 다음 탭(tab2) 순환
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Tab);
        h.run_steps(2);
        assert_eq!(h.state().current.as_deref(), Some(tab2.as_str()));
        assert_eq!(h.state().doc().unwrap().text, "둘째 탭 메모");

        // 수정된 파일 탭 닫기 -> 자동 저장 후 탭 닫힘
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num3);
        h.run_steps(2);
        h.state_mut().doc_mut().unwrap().text = "수정된 파일\n".into();
        h.state_mut().doc_mut().unwrap().refresh_meta();
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::W);
        h.run_steps(2);
        assert!(h.state().dialog.is_none());
        assert_eq!(h.state().tabs.len(), 2);
        assert_eq!(
            std::fs::read_to_string(dir.join("note.md")).unwrap(),
            "수정된 파일\n"
        );

        // 저장할 수 없는 경로의 dirty 파일 탭을 닫으면 저장 확인 창(After::CloseTab) 표시
        h.state_mut().open_file("note.md");
        h.run_steps(2);
        assert_eq!(h.state().tabs.len(), 3);
        h.state_mut().doc_mut().unwrap().text = "미저장 충돌\n".into();
        h.state_mut().doc_mut().unwrap().refresh_meta();
        std::fs::remove_file(dir.join("note.md")).unwrap();
        std::fs::create_dir(dir.join("note.md")).unwrap();
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::W);
        h.run_steps(2);
        assert!(matches!(
            h.state().dialog,
            Some(Dialog::Unsaved(After::CloseTab(_)))
        ));
        h.get_by_label("저장 안 함").click();
        h.run_steps(3);
        assert!(h.state().dialog.is_none());
        assert_eq!(h.state().tabs.len(), 2);
        assert_ne!(h.state().current.as_deref(), Some("note.md"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_document_enter_creates_visible_blank_lines_and_navigates() {
        let dir = std::env::temp_dir().join("mdeditor_ui_test_blank_enter");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let d1 = dir.clone();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1000.0, 800.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, None, &d1));
        h.run_steps(2);

        assert_eq!(h.state().doc().unwrap().text, "");
        assert_eq!(h.state().active, Some(0));

        // 아무 내용 없이 Enter -> 빈 줄 2개 슬롯 생성 (0..0, 2..2), 커서는 2..2로 이동
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        assert_eq!(h.state().doc().unwrap().text, "\n\n");
        assert_eq!(h.state().active, Some(2));
        assert_eq!(h.state().doc_slots(), vec![0..0, 2..2]);

        // 한 번 더 Enter -> 빈 줄 하나 더 늘고 커서는 4..4로 이동
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        assert_eq!(h.state().doc().unwrap().text, "\n\n\n\n");
        assert_eq!(h.state().active, Some(4));
        assert_eq!(
            collect_doc_slots("\n\n\n\n", &[], None),
            vec![0..0, 2..2, 4..4]
        );

        // 빈 줄 아래에서 텍스트 입력 시 위의 빈 줄은 유지됨
        h.event(egui::Event::Text("셋째 줄".into()));
        h.run_steps(2);
        assert_eq!(h.state().doc().unwrap().text, "\n\n\n\n셋째 줄");
        assert_eq!(h.state().active, Some(4));

        // ArrowUp으로 빈 줄 슬롯(2..2, 0..0)에 직접 진입하고 ArrowDown으로 복귀
        h.key_press(egui::Key::Home);
        h.run_steps(1);
        h.key_press(egui::Key::ArrowUp);
        h.run_steps(2);
        assert_eq!(h.state().active, Some(2));
        h.key_press(egui::Key::ArrowUp);
        h.run_steps(2);
        assert_eq!(h.state().active, Some(0));
        h.key_press(egui::Key::ArrowDown);
        h.run_steps(2);
        assert_eq!(h.state().active, Some(2));

        // 빈 줄에서 Backspace로 빈 줄 하나 삭제
        h.key_press(egui::Key::Backspace);
        h.run_steps(2);
        assert_eq!(h.state().doc().unwrap().text, "\n\n셋째 줄");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cursor_navigation_cross_block_selection_and_reorder() {
        let dir = std::env::temp_dir().join("mdeditor_ui_test_cursor_ux");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("nav.md");
        std::fs::write(&file, "첫째 블록\n둘째 줄\n\n두번째 블록\n\n세번째 블록\n").unwrap();

        let (f2, d2) = (file.clone(), dir.clone());
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1000.0, 800.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, Some(f2), &d2));
        h.run_steps(3);

        // Ctrl+Home -> 첫 블록 시작(0)
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Home);
        h.run_steps(2);
        assert_eq!(h.state().active, Some(0));
        assert_eq!(h.state().last_sel, Some((0, 0)));

        // Ctrl+End -> 마지막 블록 끝
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::End);
        h.run_steps(2);
        let b3_start = h.state().doc().unwrap().blocks[2].start;
        assert_eq!(h.state().active, Some(b3_start));

        // Ctrl+Home 후 첫 Shift+ArrowDown은 블록 내 2행으로, 두 번째 Shift+ArrowDown은 다음 블록까지 선택 확장
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Home);
        h.run_steps(2);
        h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::ArrowDown);
        h.run_steps(2);
        assert_eq!(h.state().block_sel, None);
        h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::ArrowDown);
        h.run_steps(2);
        let blocks = h.state().doc().unwrap().blocks.clone();
        assert_eq!(h.state().block_sel, Some(blocks[0].start..blocks[1].end));
        h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::ArrowDown);
        h.run_steps(2);
        assert_eq!(h.state().block_sel, Some(blocks[0].start..blocks[2].end));
        h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::ArrowUp);
        h.run_steps(2);
        assert_eq!(h.state().block_sel, Some(blocks[0].start..blocks[1].end));

        // block_sel 상태에서 Tab / Shift+Tab 일괄 들여쓰기·내어쓰기
        h.key_press(egui::Key::Tab);
        h.run_steps(2);
        assert!(h.state().doc().unwrap().text.starts_with("  첫째 블록"));
        h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Tab);
        h.run_steps(2);
        assert!(h.state().doc().unwrap().text.starts_with("첫째 블록"));

        // Alt+ArrowDown으로 첫 블록을 두 번째 블록 아래로 이동
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Home);
        h.run_steps(2);
        h.key_press_modifiers(egui::Modifiers::ALT, egui::Key::ArrowDown);
        h.run_steps(2);
        let moved_blocks: Vec<String> = {
            let d = h.state().doc().unwrap();
            d.blocks
                .iter()
                .map(|r| d.text[r.clone()].to_string())
                .collect()
        };
        assert_eq!(
            moved_blocks,
            vec!["두번째 블록", "첫째 블록\n둘째 줄", "세번째 블록"]
        );

        // ArrowLeft로 블록 시작에서 이전 블록 끝으로 이동, ArrowRight로 다시 다음 블록 시작으로 이동
        let moved_start = h.state().active.unwrap();
        let moved_chars = char_len(&h.state().doc().unwrap().text[..moved_start]);
        assert_eq!(h.state().last_sel, Some((moved_chars, moved_chars)));
        h.key_press(egui::Key::ArrowLeft);
        h.run_steps(2);
        assert_eq!(h.state().active, Some(0));

        h.key_press(egui::Key::ArrowRight);
        h.run_steps(2);
        assert_eq!(h.state().active, Some(moved_start));

        // Ctrl+D로 마지막 블록 복제
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::End);
        h.run_steps(2);
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::D);
        h.run_steps(2);
        assert_eq!(h.state().doc().unwrap().blocks.len(), 4);
        assert!(h
            .state()
            .doc()
            .unwrap()
            .text
            .ends_with("세번째 블록\n\n세번째 블록\n"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn block_kind_conversion_and_todo_toggle() {
        let (s, sel) = convert_lines_kind("일반 문단", (2, 2), BlockKind::Heading(1));
        assert_eq!((s.as_str(), sel), ("# 일반 문단", (4, 4)));
        // 이미 제목 1인 줄에 다시 Heading(1)을 적용하면 일반 문단으로 토글 해제
        let (s, sel) = convert_lines_kind(&s, sel, BlockKind::Heading(1));
        assert_eq!((s.as_str(), sel), ("일반 문단", (2, 2)));
        // 제목 2 -> 인용구 -> 할 일 목록 전환
        let (s, sel) = convert_lines_kind(&s, sel, BlockKind::Heading(2));
        assert_eq!(s, "## 일반 문단");
        let (s, sel) = convert_lines_kind(&s, sel, BlockKind::Quote);
        assert_eq!(s, "> 일반 문단");
        let (s, sel) = convert_lines_kind(&s, sel, BlockKind::Todo);
        assert_eq!(s, "- [ ] 일반 문단");
        // Ctrl+Enter 토글: 미완료 -> 완료 -> 미완료
        let (s, sel) = toggle_todo_lines(&s, sel);
        assert_eq!(s, "- [x] 일반 문단");
        let (s, _) = toggle_todo_lines(&s, sel);
        assert_eq!(s, "- [ ] 일반 문단");
        // 다중 줄 번호 목록 변환
        let (s, _) = convert_lines_kind("첫줄\n둘째줄", (0, 6), BlockKind::Numbered);
        assert_eq!(s, "1. 첫줄\n2. 둘째줄");
    }

    #[test]
    fn context_menu_and_shortcuts_flow() {
        use egui_kittest::kittest::Queryable;
        let dir = std::env::temp_dir().join("mdeditor_ui_test_ctx_shortcuts");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("ctx.md");
        std::fs::write(&file, "첫 문단\n\n둘째 문단\n").unwrap();

        let (f2, d2) = (file.clone(), dir.clone());
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1000.0, 800.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, Some(f2), &d2));
        h.run_steps(3);

        // 1. 비활성 블록("둘째 문단")을 우클릭하면 해당 블록이 활성화되고 컨텍스트 메뉴가 열린다
        let r2 = h.get_by_label("둘째 문단").rect();
        let click_pos = r2.center();
        h.input_mut()
            .events
            .push(egui::Event::PointerMoved(click_pos));
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: click_pos,
            button: egui::PointerButton::Secondary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: click_pos,
            button: egui::PointerButton::Secondary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(2);
        assert!(matches!(
            h.state().context_menu,
            Some(ContextMenu::Canvas(_))
        ));
        assert_eq!(h.state().active, Some("첫 문단\n\n".len()));
        assert!(h.query_by_label("잘라내기").is_some());
        assert!(h.query_by_label("블록 전환").is_some());
        assert!(h.query_by_label("삽입").is_some());
        assert!(h.query_by_label("블록 삭제").is_some());

        // 하위 메뉴("블록 전환" -> "코드 블록으로 전환") 열기·닫기 확인
        h.state_mut().canvas_sub = Some(CanvasSub::TurnInto);
        h.run_steps(2);
        assert!(h.query_by_label("코드 블록으로 전환").is_some());

        // Esc로 우클릭 메뉴(하위 메뉴 포함)만 먼저 닫히고 활성 블록은 유지된다
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        assert!(h.state().context_menu.is_none());
        assert!(h.state().canvas_sub.is_none());
        assert_eq!(h.state().active, Some("첫 문단\n\n".len()));

        // 2. 단축키로 블록 유형 전환 (Ctrl+Alt+1 -> 제목 1, Ctrl+Enter -> 할 일 토글)
        let alt_cmd = egui::Modifiers::COMMAND | egui::Modifiers::ALT;
        let shift_cmd = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
        h.key_press_modifiers(alt_cmd, egui::Key::Num1);
        h.run_steps(2);
        assert_eq!(h.state().doc().unwrap().text, "첫 문단\n\n# 둘째 문단\n");

        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Enter);
        h.run_steps(2);
        assert_eq!(
            h.state().doc().unwrap().text,
            "첫 문단\n\n- [ ] 둘째 문단\n"
        );
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Enter);
        h.run_steps(2);
        assert_eq!(
            h.state().doc().unwrap().text,
            "첫 문단\n\n- [x] 둘째 문단\n"
        );

        // 3. 인라인 서식 단축키: Ctrl+Shift+S(취소선), Ctrl+Shift+H(형광펜), Ctrl+E(코드), Ctrl+\(서식 지우기)
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Home);
        h.run_steps(2);
        h.state_mut().pending_sel = Some((0, 4)); // "첫 문단" 전체 선택
        h.run_steps(2);
        h.key_press_modifiers(shift_cmd, egui::Key::S);
        h.run_steps(2);
        assert!(h.state().doc().unwrap().text.starts_with("~~첫 문단~~"));

        h.key_press_modifiers(shift_cmd, egui::Key::H);
        h.run_steps(2);
        assert!(h.state().doc().unwrap().text.starts_with("~~==첫 문단==~~"));

        // 전체 첫 줄 선택 후 Ctrl+\ 로 서식 지우기
        h.state_mut().pending_sel = Some((0, char_len("~~==첫 문단==~~")));
        h.run_steps(2);
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Backslash);
        h.run_steps(2);
        assert!(h.state().doc().unwrap().text.starts_with("첫 문단\n"));

        // 4. 보기 전환 단축키: Ctrl+Shift+L(목차), Ctrl+Alt+W(전체 너비), Ctrl+Alt+F(글꼴 크기)
        assert!(!h.state().toc_open);
        h.key_press_modifiers(shift_cmd, egui::Key::L);
        h.run_steps(2);
        assert!(h.state().toc_open);

        let prev_fw = h.state().full_width;
        h.key_press_modifiers(alt_cmd, egui::Key::W);
        h.run_steps(2);
        assert_eq!(h.state().full_width, !prev_fw);

        // 5. Ctrl+Shift+D로 현재 블록 삭제
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::End);
        h.run_steps(2);
        h.key_press_modifiers(shift_cmd, egui::Key::D);
        h.run_steps(2);
        assert_eq!(h.state().doc().unwrap().text, "첫 문단\n");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn incremental_block_edit_and_leading_space_stay_consistent() {
        let check_sync = |doc: &Doc| {
            let full = Doc::scratch(doc.text.clone());
            assert_eq!(doc.chars, full.chars);
            assert_eq!(doc.lines, full.lines);
            assert_eq!(doc.blocks, full.blocks);
            assert_eq!(doc.outline, full.outline);
            assert_eq!(doc.prev, doc.text);
        };

        // 1. 공백 문자만 입력된 줄도 빈 줄 구분자가 아닌 실제 블록으로 유지
        assert_eq!(split_blocks(" "), vec![0..1]);

        // 2. 일반 문단 연속 타이핑(한글·ASCII) + 제목·코드 블록 전환 + 빈 줄 분할 시 증분 메타 정합성
        let mut doc = Doc::scratch(
            "# 제목 1\n\n첫째 문단\n\n- 항목 1\n\n```\ncode\n```\n\n## 제목 2\n\n꼬리 문단".into(),
        );
        check_sync(&doc);
        let r1 = doc.blocks[1].clone();
        doc.apply_block_edit(r1, "첫째 문단!");
        check_sync(&doc);
        let r1 = doc.blocks[1].clone();
        doc.apply_block_edit(r1, "첫째 문단!가");
        check_sync(&doc);
        let r1 = doc.blocks[1].clone();
        doc.apply_block_edit(r1, "첫째 문단!가나다");
        check_sync(&doc);
        let r1 = doc.blocks[1].clone();
        doc.apply_block_edit(r1, "첫째 문단!가나다\n\n새 블록");
        check_sync(&doc);
        let r0 = doc.blocks[0].clone();
        doc.apply_block_edit(r0, "### 바뀐 제목");
        check_sync(&doc);

        // 실행 취소로 원문까지 정확히 복원되는지 확인
        while doc.step_history(true).is_some() {
            check_sync(&doc);
        }
        assert_eq!(
            doc.text,
            "# 제목 1\n\n첫째 문단\n\n- 항목 1\n\n```\ncode\n```\n\n## 제목 2\n\n꼬리 문단"
        );

        // 3. UI 연속 타이핑(빈 블록 첫 스페이스 입력 포함) 시 한 글자도 누락되지 않는다
        let dir = std::env::temp_dir().join("mdeditor_ui_test_fast_typing");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let d1 = dir.clone();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1000.0, 800.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, None, &d1));
        h.run_steps(2);

        h.event(egui::Event::Text(" ".into()));
        h.run_steps(2);
        assert_eq!(h.state().doc().unwrap().text, " ");
        for ch in ["연", "속", "타", "이", "핑", " ", "테스트", "!", "123"] {
            h.event(egui::Event::Text(ch.into()));
            h.step();
        }
        h.run_steps(2);
        assert_eq!(h.state().doc().unwrap().text, " 연속타이핑 테스트!123");
        check_sync(h.state().doc().unwrap());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ctrl_click_multi_caret_and_alt_drag_column_edit() {
        use egui_kittest::kittest::Queryable;
        let dir = std::env::temp_dir().join("mdeditor_multi_caret_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = "- 하나\n- 둘\n- 셋";
        let file = dir.join("multi.md");
        std::fs::write(&file, src).unwrap();
        let (f2, d2) = (file.clone(), dir.clone());
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(900.0, 600.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, Some(f2), &d2));
        h.run_steps(3);
        h.state_mut().go(0, 0);
        h.run_steps(3);
        let text = |h: &egui_kittest::Harness<'_, App>| h.state().doc().unwrap().text.clone();
        let rect = h
            .get_by_role(egui::accesskit::Role::MultilineTextInput)
            .rect();
        // 각 줄 끝(오른쪽 끝)을 누른다
        let line_end = |i: usize| {
            let row_h = rect.height() / 3.0;
            egui::pos2(rect.right() - 4.0, rect.top() + row_h * (i as f32 + 0.5))
        };
        let press = |h: &mut egui_kittest::Harness<'_, App>, pos, pressed, m: egui::Modifiers| {
            h.input_mut().events.push(egui::Event::ModifiersChanged(m));
            h.input_mut().events.push(egui::Event::PointerMoved(pos));
            h.input_mut().events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: m,
            });
            h.step();
        };
        let click = |h: &mut egui_kittest::Harness<'_, App>, pos, m: egui::Modifiers| {
            press(h, pos, true, m);
            press(h, pos, false, m);
            h.input_mut()
                .events
                .push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
            h.run_steps(2);
        };

        // 1. 클릭 + Ctrl+클릭 두 번 → 세 줄 끝에 커서, 타이핑·Backspace가 모든 줄에 들어간다
        click(&mut h, line_end(0), egui::Modifiers::NONE);
        click(&mut h, line_end(1), egui::Modifiers::COMMAND);
        click(&mut h, line_end(2), egui::Modifiers::COMMAND);
        assert_eq!(h.state().extra_carets.as_ref().map(|e| e.1.len()), Some(2));
        h.event(egui::Event::Text("X".into()));
        h.run_steps(2);
        assert_eq!(text(&h), "- 하나X\n- 둘X\n- 셋X");
        h.event(egui::Event::Text("Y".into()));
        h.run_steps(2);
        h.key_press(egui::Key::Backspace);
        h.run_steps(2);
        assert_eq!(text(&h), "- 하나X\n- 둘X\n- 셋X");
        // 한글 조합도 모든 커서에 (Preedit → Commit)
        for ime in [
            egui::ImeEvent::Preedit {
                text: "ㄱ".into(),
                active_range_chars: None,
            },
            egui::ImeEvent::Preedit {
                text: "가".into(),
                active_range_chars: None,
            },
            egui::ImeEvent::Commit("가".into()),
        ] {
            h.event(egui::Event::Ime(ime));
            h.run_steps(2);
        }
        assert_eq!(text(&h), "- 하나X가\n- 둘X가\n- 셋X가");
        // Esc는 추가 커서만 끄고 편집은 계속
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        assert!(h.state().extra_carets.is_none());
        assert!(h.state().active.is_some());
        h.event(egui::Event::Text("!".into()));
        h.run_steps(2);
        assert_eq!(text(&h), "- 하나X가\n- 둘X가\n- 셋X가!");

        // 2. Alt+드래그(첫 줄 → 셋째 줄)로 세로 열 커서
        let rect = h
            .get_by_role(egui::accesskit::Role::MultilineTextInput)
            .rect();
        let x = rect.left() + 400.0; // 모든 줄보다 오른쪽 → 각 줄 끝
        let row_h = rect.height() / 3.0;
        let (a, b) = (
            egui::pos2(x.min(rect.right() - 4.0), rect.top() + row_h * 0.5),
            egui::pos2(x.min(rect.right() - 4.0), rect.top() + row_h * 2.5),
        );
        press(&mut h, a, true, egui::Modifiers::ALT);
        for k in 1..=4 {
            let pos = a + (b - a) * (k as f32 / 4.0);
            h.input_mut().events.push(egui::Event::PointerMoved(pos));
            h.step();
        }
        press(&mut h, b, false, egui::Modifiers::ALT);
        h.input_mut()
            .events
            .push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        h.run_steps(2);
        assert_eq!(h.state().extra_carets.as_ref().map(|e| e.1.len()), Some(2));
        h.event(egui::Event::Text("#".into()));
        h.run_steps(2);
        assert_eq!(text(&h), "- 하나X가#\n- 둘X가#\n- 셋X가!#");
        // 일반 클릭은 추가 커서를 끈다
        click(&mut h, line_end(0), egui::Modifiers::NONE);
        assert!(h.state().extra_carets.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn click_caret_softwrap_and_block_arrow_navigation() {
        use egui_kittest::kittest::Queryable;
        let dir = std::env::temp_dir().join("mdeditor_ui_test_caret_perf");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let long_wrap =
            "가나다라마바사 아자차카타파하 0123456789 ABCDEFGHIJKLMNOPQRSTUVWXYZ ".repeat(3);
        let long_trimmed = long_wrap.trim_end().to_string();
        let file = dir.join("perf.md");
        let initial = format!("첫째 블록\n\n{long_trimmed}\n\n셋째 블록\n");
        std::fs::write(&file, &initial).unwrap();

        let (f2, d2) = (file.clone(), dir.clone());
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(900.0, 760.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, Some(f2), &d2));
        h.run_steps(3);

        // 1. 비활성 블록 클릭 시 단 1프레임 활성화만에 블록 끝이 아닌 클릭 위치에 커서가 배치된다
        let label_rect = h.get_by_label("셋째 블록").rect();
        let mid_click = egui::pos2(label_rect.left() + 14.0, label_rect.center().y);
        h.input_mut()
            .events
            .push(egui::Event::PointerMoved(mid_click));
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: mid_click,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: mid_click,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.step(); // 클릭 처리
        h.step(); // block_editor 첫 렌더링 프레임
        let third_start = h.state().doc().unwrap().blocks[2].start;
        assert_eq!(h.state().active, Some(third_start));
        assert!(h.state().pending_click_pos.is_none());
        let third_base = char_len(&h.state().doc().unwrap().text[..third_start]);
        let sel_after_one_frame = h.state().last_sel.unwrap();
        assert!(
            sel_after_one_frame.0 > third_base
                && sel_after_one_frame.0 < third_base + char_len("셋째 블록"),
            "expected middle caret in first frame, got {:?} (base={third_base})",
            sel_after_one_frame
        );

        // 2. 긴 자동 줄바꿈 블록에서 연속 타이핑 직후에도 cursor_info.doc_rev가 유지되어 ↑키가 블록 내 윗줄로 이동한다
        let second_start = h.state().doc().unwrap().blocks[1].start;
        let second_end = h.state().doc().unwrap().blocks[1].end;
        h.state_mut().go(second_start, second_end);
        h.run_steps(2);
        h.event(egui::Event::Text("!".into()));
        h.step();
        {
            let rev = h.state().doc().unwrap().rev;
            let info = h.state().cursor_info.unwrap();
            assert_eq!(info.doc_rev, rev);
            assert!(!info.on_first_row);
        }
        // 첫 번째 ArrowUp: 같은 블록의 윗줄로 이동 (이웃 블록으로 튕기지 않음)
        h.key_press(egui::Key::ArrowUp);
        h.run_steps(2);
        assert_eq!(h.state().active, Some(second_start));

        // 3. 블록 시작까지 올라가 ArrowUp 한 번 더 누르면 첫째 블록으로 이동 + active_char_cache 일치
        while !h.state().cursor_info.is_some_and(|i| i.on_first_row) {
            h.key_press(egui::Key::ArrowUp);
            h.run_steps(2);
        }
        h.key_press(egui::Key::ArrowUp);
        h.run_steps(2);
        assert_eq!(h.state().active, Some(0));
        assert_eq!(h.state().active_char_cache.0, 0);
        assert_eq!(h.state().active_char_cache.1, h.state().doc().unwrap().rev);
        assert_eq!(h.state().active_char_cache.2, 0);

        // Home + ArrowLeft은 문서 맨 앞이므로 블록 유지, End + ArrowRight는 다음 블록으로 이동
        h.key_press(egui::Key::Home);
        h.run_steps(1);
        h.key_press(egui::Key::ArrowLeft);
        h.run_steps(2);
        assert_eq!(h.state().active, Some(0));
        h.key_press(egui::Key::End);
        h.run_steps(1);
        h.key_press(egui::Key::ArrowRight);
        h.run_steps(2);
        assert_eq!(h.state().active, Some(second_start));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn code_block_triggers_indent_exit_and_view_modes() {
        use egui_kittest::kittest::Queryable;
        let dir = std::env::temp_dir().join("mdeditor_ui_test_code_and_view");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let d1 = dir.clone();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1200.0, 800.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, None, &d1));
        h.run_steps(3);

        // 1. 빈 문서에는 더 이상 "빠른 시작" 안내 카드가 표시되지 않는다
        assert!(h.query_by_label("빠른 시작").is_none());

        // 2. ``` + Space 입력 시 즉시 코드 블록이 열리고 커서가 본문 줄에 놓인다
        for ch in ["`", "`", "`", " "] {
            h.event(egui::Event::Text(ch.into()));
            h.step();
        }
        h.run_steps(2);
        assert_eq!(h.state().doc().unwrap().text, "```\n\n```");
        assert_eq!(h.state().active, Some(0));
        // 커서는 4번째 문자(둘째 줄 시작)에 위치한다
        assert_eq!(h.state().last_sel, Some((4, 4)));

        // 3. 새 탭에서 ```rust + Enter 시 언어 태그를 유지한 채 코드 블록이 열린다
        h.state_mut().new_tab();
        h.run_steps(2);
        for ch in ["`", "`", "`", "r", "u", "s", "t"] {
            h.event(egui::Event::Text(ch.into()));
            h.step();
        }
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        assert_eq!(h.state().doc().unwrap().text, "```rust\n\n```");

        // 코드 본문 입력 시 들여쓰기(2칸)가 Enter 후에도 자동 유지된다
        for ch in [" ", " ", "l", "e", "t", " ", "x", " ", "=", " ", "1", ";"] {
            h.event(egui::Event::Text(ch.into()));
            h.step();
        }
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        assert_eq!(
            h.state().doc().unwrap().text,
            "```rust\n  let x = 1;\n  \n```"
        );

        // 편집 중엔 ```가 숨겨져 있다: 마지막 줄에서 ↓를 누르면 코드 블록을 빠져나와 아래에 새 블록을 만든다
        h.key_press(egui::Key::ArrowDown);
        h.run_steps(2);
        h.event(egui::Event::Text("다음 문단".into()));
        h.run_steps(2);
        assert_eq!(
            h.state().doc().unwrap().text,
            "```rust\n  let x = 1;\n  \n```\n\n다음 문단"
        );

        // 4. 노션식 가운데 <-> 전체 너비 & 집중 크게 보기(F11) 전환 확인
        assert!(!h.state().full_width);
        assert!(h.query_by_label("가운데").is_some());
        h.key_press_modifiers(
            egui::Modifiers::COMMAND | egui::Modifiers::ALT,
            egui::Key::W,
        );
        h.run_steps(2);
        assert!(h.state().full_width);
        assert!(h.query_by_label("전체 너비").is_some());

        assert!(!h.state().focus_mode);
        h.key_press(egui::Key::F11);
        h.run_steps(2);
        assert!(h.state().focus_mode);
        assert!(h.query_by_label("기본 보기 (F11)").is_some());
        h.key_press(egui::Key::F11);
        h.run_steps(2);
        assert!(!h.state().focus_mode);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn korean_ime_enter_visual_table_toc_and_font() {
        use egui_kittest::kittest::Queryable;

        // 1. 마크다운 표 파서/직렬화 정합성(이스케이프 파이프·정렬 포함)
        let raw_tbl = "| 이름 | 설명 |\n| :--- | ---: |\n| A\\|B | 값 1 |";
        let tbl = parse_md_table(raw_tbl).unwrap();
        assert_eq!(tbl.headers, vec!["이름", "설명"]);
        assert_eq!(tbl.aligns, vec![TableAlign::Left, TableAlign::Right]);
        assert_eq!(tbl.rows, vec![vec!["A|B", "값 1"]]);
        let rt = serialize_md_table(&tbl);
        assert_eq!(parse_md_table(&rt), Some(tbl));

        let dir = std::env::temp_dir().join("mdeditor_ui_test_ime_table_toc");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let d1 = dir.clone();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1200.0, 800.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, None, &d1));
        h.run_steps(3);

        // 2. 한글 IME 조합 중("하이"에서 "이" 입력 중) Enter를 눌러도 "이"가 새 줄로 밀리지 않는다
        // 케이스 A: ImeEvent::Preedit("이") 후 Commit("이") + Key::Enter 가 같은 프레임에 도착
        h.event(egui::Event::Ime(egui::ImeEvent::Commit("하".into())));
        h.step();
        h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "이".into(),
            active_range_chars: None,
        }));
        h.step();
        assert_eq!(h.state().doc().unwrap().text, "하이");
        assert!(h.state().ime_composing);

        h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
            text: String::new(),
            active_range_chars: None,
        }));
        h.event(egui::Event::Ime(egui::ImeEvent::Commit("이".into())));
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        assert_eq!(
            h.state().doc().unwrap().text,
            "하이\n\n",
            "조합 완료 + Enter 시 '이'가 윗줄에 남고 새 줄로 내려가야 함"
        );

        // 케이스 B: Commit 없이 ImeEvent::Preedit("이") 상태 그대로 Key::Enter 가 들어온 경우
        h.event(egui::Event::Ime(egui::ImeEvent::Commit("하".into())));
        h.step();
        h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "이".into(),
            active_range_chars: None,
        }));
        h.step();
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        assert_eq!(
            h.state().doc().unwrap().text,
            "하이\n\n하이\n\n",
            "Preedit 유지 중 Enter 시에도 조합 글자가 삭제되거나 밀리지 않아야 함"
        );

        // 3. 표 직접 편집 UI (+ 행 추가, 정렬 전환, 마크다운 보기 / 비활성 뷰 모드 전환, TOC 왼쪽 배치 확인)
        h.state_mut()
            .insert_block_template("| 열1 | 열2 |\n| --- | --- |\n| a | b |", 2);
        h.run_steps(2);
        assert!(h.state().is_visual_table_active());
        assert!(h.query_by_label("+ 행").is_some());
        assert!(h.query_by_label("마크다운 보기").is_some());
        assert!(h.query_by_label("정렬: 기본").is_some());
        h.get_by_label("정렬: 기본").click();
        h.run_steps(2);
        assert!(h.state().doc().unwrap().text.contains(":---"));
        h.get_by_label("+ 행").click();
        h.run_steps(2);
        assert!(h.state().doc().unwrap().text.contains("|  |  |"));
        h.get_by_label("완료 (Esc)").click();
        h.run_steps(2);
        assert!(!h.state().is_visual_table_active());
        assert!(h.query_by_label("열1").is_some());

        // 다시 표를 활성화해 '표 삭제'로 통째로 삭제한다
        h.get_by_label("열1").click();
        h.run_steps(2);
        assert!(h.state().is_visual_table_active());
        h.get_by_label("표 삭제").click();
        h.run_steps(2);
        assert!(!h.state().doc().unwrap().text.contains("| 열1 |"));

        // 4. TOC 창이 화면 왼쪽에 배치되고 글꼴 선택이 동작한다
        h.state_mut().toc_open = true;
        h.run_steps(2);
        let toc_rect = h.get_by_label("문서 목차 (TOC)").rect();
        assert!(
            toc_rect.left() < 280.0,
            "TOC는 왼쪽 패널에 있어야 함: {toc_rect:?}"
        );

        assert!(!theme::available_fonts().is_empty());
        let ctx = h.ctx.clone();
        h.state_mut().apply_font(&ctx, "malgun".into());
        h.run_steps(2);
        assert_eq!(h.state().font_family, "malgun");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bold_text_uses_bold_font_in_view_edit_and_tables() {
        use egui_kittest::kittest::Queryable;
        let p = theme::pal(false);
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);

        // 1. 뷰 모드 LayoutJob: 일반/굵게/밑줄굵게/한국어 조사 결합 굵게 모두 theme::bold() 적용
        let view_job = ui::inline_md_view_job(
            "보통 **굵게** __밑줄굵게__ **완료!**입니다 \\*\\*그대로\\*\\*",
            15.0,
            &p,
            false,
        );
        assert_eq!(view_job.text, "보통 굵게 밑줄굵게 완료!입니다 **그대로**");
        let is_bold_sec = |needle: &str| {
            view_job.sections.iter().any(|s| {
                let slice =
                    &view_job.text[usize::from(s.byte_range.start)..usize::from(s.byte_range.end)];
                slice.contains(needle) && s.format.font_id.family == theme::bold()
            })
        };
        assert!(is_bold_sec("굵게"), "{:?}", view_job.sections);
        assert!(is_bold_sec("밑줄굵게"), "{:?}", view_job.sections);
        assert!(is_bold_sec("완료!"), "{:?}", view_job.sections);
        assert!(!is_bold_sec("보통"));
        assert!(!is_bold_sec("그대로"));

        // ==강조==는 보기·편집 화면 모두 굵게 + 같은 강조 배경, **굵게**는 배경 없음
        let bg_of = |job: &egui::text::LayoutJob, needle: &str| {
            job.sections
                .iter()
                .find(|s| {
                    &job.text[usize::from(s.byte_range.start)..usize::from(s.byte_range.end)]
                        == needle
                })
                .map(|s| {
                    (
                        s.format.background,
                        s.format.font_id.family == theme::bold(),
                    )
                })
        };
        let src = "가 ==강조== 나 **굵게** 다";
        let view = ui::inline_md_view_job(src, 15.0, &p, false);
        let edit = ui::md_job(&ctx, src, 15.0, &p, false);
        assert_eq!(bg_of(&view, "강조"), bg_of(&edit, "강조"));
        assert_ne!(bg_of(&view, "강조").unwrap().0, egui::Color32::TRANSPARENT);
        assert_eq!(
            bg_of(&view, "굵게"),
            Some((egui::Color32::TRANSPARENT, true))
        );

        // 2. 편집 모드 LayoutJob: 바이트 100% 보존 + 내부 텍스트에 theme::bold() 적용
        let src = "앞 **편집굵게** __또굵게__ 뒤";
        let edit_job = ui::md_job(&ctx, src, 15.0, &p, false);
        assert_eq!(edit_job.text, src);
        assert!(edit_job.sections.iter().any(|s| {
            &edit_job.text[usize::from(s.byte_range.start)..usize::from(s.byte_range.end)]
                == "편집굵게"
                && s.format.font_id.family == theme::bold()
        }));
        assert!(edit_job.sections.iter().any(|s| {
            &edit_job.text[usize::from(s.byte_range.start)..usize::from(s.byte_range.end)]
                == "또굵게"
                && s.format.font_id.family == theme::bold()
        }));

        // 3. 실제 UI에서 굵은 글씨 문단과 표 셀이 기호 없이 굵게 렌더링된다
        let dir = std::env::temp_dir().join("mdeditor_ui_test_bold_fix");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("bold.md");
        std::fs::write(
            &file,
            "여기는 **굵은 글씨!**입니다\n\n| 구분 | 설명 |\n| --- | --- |\n| A | **셀굵게** |\n",
        )
        .unwrap();
        let (f2, d2) = (file.clone(), dir.clone());
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1000.0, 800.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, Some(f2), &d2));
        h.run_steps(3);
        assert!(h.query_by_label("여기는 굵은 글씨!입니다").is_some());
        assert!(h.query_by_label("셀굵게").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn incremental_block_edit_multiline_and_code_blocks() {
        let check_sync = |doc: &Doc| {
            let full = Doc::scratch(doc.text.clone());
            assert_eq!(doc.chars, full.chars);
            assert_eq!(doc.lines, full.lines);
            assert_eq!(doc.blocks, full.blocks);
            assert_eq!(doc.outline, full.outline);
            assert_eq!(doc.prev, doc.text);
        };

        let mut doc = Doc::scratch(
            "# 시작\n\n- 첫째 항목\n- 둘째 항목\n\n```python\n# 코드 안 주석\nx = 1\n```\n\n## 끝 제목\n\n> 인용 첫 줄\n> 인용 둘째 줄"
                .into(),
        );
        check_sync(&doc);

        // 다중 줄 목록 편집 (in-place 증분)
        let list_r = doc.blocks[1].clone();
        doc.apply_block_edit(list_r, "- 첫째 항목!\n- 둘째 항목\n- 셋째 항목");
        check_sync(&doc);

        // 닫힌 코드 블록 내부에서 '#' 주석 추가/수정 (목차 불변, in-place 증분)
        let code_r = doc.blocks[2].clone();
        doc.apply_block_edit(
            code_r,
            "```python\n# 코드 안 주석\n# 또 다른 주석\nx = 2\n```",
        );
        check_sync(&doc);

        // 인용구 편집 및 블록 분할
        let quote_r = doc.blocks[4].clone();
        doc.apply_block_edit(quote_r, "> 인용 수정\n\n새 문단");
        check_sync(&doc);

        // 닫는 펜스를 지워 뒤 블록을 삼키는 경우도 전체 재분할 폴백으로 정합성 유지
        let code_r = doc.blocks[2].clone();
        doc.apply_block_edit(code_r, "```python\n# 코드 안 주석\nx = 2");
        check_sync(&doc);
    }

    #[test]
    fn replace_all_and_heading_slug_accuracy() {
        let s = "한글 abc ABC 한글 abc 끝";
        let m = find_all(s, "abc", false);
        assert_eq!(replace_all(s, &m, "XYZ!"), "한글 XYZ! XYZ! 한글 XYZ! 끝");
        assert_eq!(replace_all(s, &m, ""), "한글   한글  끝");
        assert_eq!(replace_all(s, &[], "X"), s);

        assert_eq!(heading_slug("두 번째 제목!"), "두-번째-제목");
        assert_eq!(heading_slug("Hello World 123"), "hello-world-123");
    }

    #[test]
    fn native_renderer_handles_links_footnotes_math_and_anchors() {
        use egui_kittest::kittest::Queryable;
        let dir = std::env::temp_dir().join("mdeditor_ui_test_links_footnotes_math");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let main_file = dir.join("main.md");
        let other_file = dir.join("sub").join("other.md");
        std::fs::write(&other_file, "# 다른 문서\n\n열림 확인\n").unwrap();
        std::fs::write(
            &main_file,
            "# 첫 제목\n\n[다른 문서 열기](sub/other.md) · [둘째로](#두-번째-제목)\n\n문장[^1]과 인라인 수식 $E=mc^2$ 입니다.\n\n[^1]: 각주 설명입니다\n\n$$\na^2 + b^2 = c^2\n$$\n\n:::toggle 토글\n내용\n:::\n\n## 두 번째 제목\n\n끝 문단\n",
        )
        .unwrap();

        let (f2, d2) = (main_file.clone(), dir.clone());
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1000.0, 800.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, Some(f2), &d2));
        h.run_steps(3);

        // 1. 각주 참조·정의 및 인라인 수식이 네이티브 렌더러에 표시됨
        assert!(h.query_by_label("[1]:").is_some());
        assert!(
            h.query_by_label_contains("문장[1]과 인라인 수식 E=mc^2")
                .is_some(),
            "각주 참조 및 인라인 수식이 렌더링되어야 함"
        );

        // 2. 로컬 .md 링크 클릭 시 앱 내부 탭으로 열림
        let link_node = h.get_by_label_contains("다른 문서 열기");
        let lrect = link_node.rect();
        let click_pos = egui::pos2(lrect.left() + 20.0, lrect.center().y);
        h.input_mut()
            .events
            .push(egui::Event::PointerMoved(click_pos));
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: click_pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: click_pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(3);
        assert_eq!(h.state().current.as_deref(), Some("sub/other.md"));
        assert!(h.query_by_label("열림 확인").is_some());

        // 3. 원래 탭으로 돌아가 수식 블록 안 Ctrl+Enter로 바로 아래 새 문단 (편집 중엔 $$ 줄이 숨겨져 있음)
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num1);
        h.run_steps(2);
        let math_start = h
            .state()
            .doc()
            .unwrap()
            .blocks
            .iter()
            .find(|b| h.state().doc().unwrap().text[(*b).clone()].starts_with("$$"))
            .unwrap()
            .clone();
        h.state_mut().go(math_start.start, math_start.end);
        h.run_steps(2);
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Enter);
        h.run_steps(2);
        h.event(egui::Event::Text("수식 다음 줄".into()));
        h.run_steps(2);
        let t = h.state().doc().unwrap().text.clone();
        assert!(t.contains("$$\na^2 + b^2 = c^2\n$$\n\n수식 다음 줄"), "{t}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn image_block_edits_like_view() {
        use egui_kittest::kittest::Queryable;
        assert_eq!(image_line("![그림](images/p.png)"), Some("images/p.png"));
        assert_eq!(image_line("글 ![a](b)"), None);
        assert_eq!(hidden_prefix("![a](b.png)"), "![a](b.png)".len());

        let dir = std::env::temp_dir().join("mdeditor_ui_test_image_edit");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("images")).unwrap();
        image::RgbaImage::from_pixel(120, 60, image::Rgba([200, 30, 30, 255]))
            .save(dir.join("images/p.png"))
            .unwrap();
        let img = "![그림](images/p.png)";
        let file = dir.join("img.md");
        std::fs::write(&file, format!("앞 문단\n\n{img}\n\n뒤 문단\n")).unwrap();
        let (f2, d2) = (file.clone(), dir.clone());
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(900.0, 600.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, Some(f2), &d2));
        let settle = |h: &mut egui_kittest::Harness<'_, App>| {
            for _ in 0..40 {
                h.step();
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        };
        settle(&mut h);
        let view_y = h.get_by_label("뒤 문단").rect().top();

        // 이미지 블록 편집: 원문 대신 같은 크기의 이미지 → 아래 문단 위치가 그대로
        let start = "앞 문단\n\n".len();
        h.state_mut().go(start, start + img.len());
        settle(&mut h);
        assert_eq!(h.state().active, Some(start));
        assert!(h.query_by_label(img).is_none());
        let edit_y = h.get_by_label("뒤 문단").rect().top();
        assert!((edit_y - view_y).abs() < 1.0, "view {view_y} edit {edit_y}");
        assert!(
            h.state().status.starts_with("이미지: images/p.png"),
            "{}",
            h.state().status
        );

        // 글자를 치면 이미지는 그대로, 아래 새 문단으로
        h.input_mut().events.push(egui::Event::Text("가".into()));
        settle(&mut h);
        assert!(h
            .state()
            .doc()
            .unwrap()
            .text
            .contains(&format!("{img}\n\n가")));

        // 이미지 위 Backspace → 노션처럼 이미지 블록을 통째로 지운다
        h.state_mut().go(start, start + img.len());
        settle(&mut h);
        h.key_press(egui::Key::Backspace);
        settle(&mut h);
        let text = &h.state().doc().unwrap().text;
        assert!(!text.contains(img), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn large_document_virtualization_and_cache_perf() {
        let dir = std::env::temp_dir().join("mdeditor_ui_test_large_doc");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut big = String::with_capacity(200_000);
        for i in 0..1000 {
            big.push_str(&format!("## 섹션 {i}\n\n본문 **강조 {i}** 내용입니다.\n\n```rust\nlet v_{i} = {i};\n```\n\n"));
        }
        let file = dir.join("big.md");
        std::fs::write(&file, &big).unwrap();
        let block_count = split_blocks(&big).len();
        assert_eq!(block_count, 3000);

        let (f2, d2) = (file.clone(), dir.clone());
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1000.0, 800.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, Some(f2), &d2));
        h.run_steps(3);

        // 첫 화면에서 3,000개 전체를 렌더링하지 않고 화면 주변 블록만 가상화하여 측정·캐시한다
        let measured = h.state().block_heights.len();
        assert!(
            measured > 0 && measured < 100,
            "3,000블록 문서 초기 표시 시 뷰포트 밖 블록은 생략되어야 함 (measured={measured})"
        );
        assert!(!h.state().code_galley_cache.is_empty());
        assert!(!h.state().inline_galley_cache.is_empty());

        // 탭 전환 후 돌아와도 block_heights가 유지된다
        h.state_mut().new_tab();
        h.run_steps(2);
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num1);
        h.run_steps(2);
        assert!(h.state().block_heights.len() >= measured);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // 편집 키가 마크다운 구조를 깨거나 원문을 드러내지 않는다 (코드 블록 언어칸 입력 버그 등)
    #[test]
    fn editing_keys_keep_markdown_structure() {
        use egui_kittest::kittest::Queryable;
        assert_eq!(
            code_body_range("  ```python\n  print(1)\n  ```"),
            Some((14, 22))
        );
        assert_eq!(
            renumber_block_at("1. a\n2. \n2. b", 8),
            ("1. a\n2. \n3. b".to_string(), 8)
        );

        let dir = std::env::temp_dir().join("mdeditor_ui_test_structure");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let d1 = dir.clone();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(900.0, 600.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, None, &d1));
        h.run_steps(3);

        // ``` 뒤에 바로 글자를 쳐도 코드 카드(언어칸)로 숨겨지지 않는다
        for c in "```print".chars() {
            h.event(egui::Event::Text(c.to_string()));
            h.step();
        }
        h.run_steps(2);
        assert!(h.query_by_label("코드 블록").is_none());

        // (문서, 블록 시작, 커서 바이트, 키, 기대 결과)
        let cases: &[(&str, &str, &str, egui::Key, &str)] = &[
            (
                "빈 코드 블록 입력",
                "```\n```",
                "```\n",
                egui::Key::A,
                "```\nQ\n```",
            ),
            (
                "빈 코드 블록 Backspace",
                "앞\n\n```\n\n```",
                "앞\n\n```\n",
                egui::Key::Backspace,
                "앞\n\n",
            ),
            (
                "굵게 안 Enter",
                "가 **굵게** 나",
                "가 **굵",
                egui::Key::Enter,
                "가 **굵**\n\n**게** 나",
            ),
            (
                "링크 안 Enter",
                "[링크](u)",
                "[링",
                egui::Key::Enter,
                "[링](u)\n\n[크](u)",
            ),
            (
                "인용 중간 Enter",
                "> 인용 줄\n> 둘째",
                "> 인용",
                egui::Key::Enter,
                "> 인용\n>  줄\n> 둘째",
            ),
            (
                "제목 맨 앞 Enter",
                "## 제목",
                "## ",
                egui::Key::Enter,
                "\n\n## 제목",
            ),
            (
                "번호 목록 Enter",
                "1. 첫째\n2. 둘째",
                "1. 첫째",
                egui::Key::Enter,
                "1. 첫째\n2. \n3. 둘째",
            ),
            ("구분선 Delete", "---\n\n뒤", "---", egui::Key::Delete, "뒤"),
            (
                "이미지 앞 Delete",
                "앞\n\n![a](b.png)",
                "앞",
                egui::Key::Delete,
                "앞",
            ),
            (
                "구분선 뒤 Backspace",
                "---\n\n뒤",
                "---\n\n",
                egui::Key::Backspace,
                "뒤",
            ),
            (
                "제목 앞 Delete",
                "앞\n\n## 제목",
                "앞",
                egui::Key::Delete,
                "앞제목",
            ),
            (
                "한 줄 인용 앞 Delete",
                "앞\n\n> 인용",
                "앞",
                egui::Key::Delete,
                "앞인용",
            ),
            (
                "여러 줄 인용 앞 Delete",
                "앞\n\n> 인용\n> 둘",
                "앞",
                egui::Key::Delete,
                "앞\n\n> 인용\n> 둘",
            ),
        ];
        for &(name, doc, caret, key, want) in cases {
            h.state_mut().set_text(doc.to_string(), (0, 0));
            h.run_steps(2);
            let start = caret.rfind("\n\n").map_or(0, |i| i + 2);
            h.state_mut().go(start, caret.len());
            h.run_steps(3);
            if key == egui::Key::A {
                h.event(egui::Event::Text("Q".into()));
            } else {
                h.key_press(key);
            }
            h.run_steps(3);
            assert_eq!(h.state().doc().unwrap().text, want, "{name}");
        }

        // Shift+↑로 블록 선택이 된 뒤 글자를 쳐도 블록이 통째로 사라지지 않는다
        let doc = "앞 문단\n\n```python\nprint(1)\n```";
        h.state_mut().set_text(doc.to_string(), (0, 0));
        h.run_steps(2);
        let code = doc.find("```").unwrap();
        h.state_mut().go(code, code + "```python\npr".len());
        h.run_steps(3);
        h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::ArrowUp);
        h.run_steps(3);
        h.event(egui::Event::Text("Q".into()));
        h.run_steps(3);
        assert_eq!(h.state().doc().unwrap().text, doc);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pct_roundtrip_and_html_export_is_inert() {
        for p in ["a b/한글 #1%/x.md", "plain/path.md", "100%25"] {
            assert_eq!(pct_decode(&pct_encode(p)), p);
        }
        assert_eq!(pct_decode("끝%"), "끝%"); // 잘린 이스케이프는 그대로 둔다
        let html = md_to_html(
            "<script>alert(1)</script>\n\n<img src=x onerror=alert(1)>\n\n[x](javascript:alert(1))\n",
        );
        assert!(!html.contains("<script"), "{html}");
        assert!(!html.contains("<img"), "{html}");
        assert!(!html.contains("javascript:"), "{html}");
    }

    // 무작위 마크다운 조각에도 파서가 패닉하지 않고 블록이 UTF-8 경계·순서를 지킨다
    #[test]
    fn parsers_survive_random_markdown() {
        const PARTS: &[&str] = &[
            "# ",
            "## 제목",
            "**",
            "*",
            "~~",
            "==",
            "`",
            "```",
            "~~~",
            "$$",
            ":::toggle 접기",
            ":::",
            "> ",
            "> [!NOTE]",
            "- ",
            "1. ",
            "- [ ] ",
            "  ",
            "\t",
            "\n",
            "\n\n",
            "\r\n",
            "|a|b|",
            "|---|:-:|",
            "[l](u)",
            "![i](p.png)",
            "[[Ctrl]]",
            "가나다",
            "😀",
            "é",
            "text",
            " ",
            "---",
            "[^1]",
            "[^1]: n",
            "<b>",
            "\u{feff}",
        ];
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..400 {
            let n = (next() % 40) as usize;
            let src: String = (0..n)
                .map(|_| PARTS[(next() % PARTS.len() as u64) as usize])
                .collect();
            let mut end = 0;
            for r in split_blocks(&src) {
                assert!(
                    r.start >= end && r.start <= r.end && r.end <= src.len(),
                    "{src:?}"
                );
                assert!(
                    src.is_char_boundary(r.start) && src.is_char_boundary(r.end),
                    "{src:?}"
                );
                end = r.end;
                let b = &src[r];
                let _ = (block_inline_spans(b), hidden_runs(b), visible_bounds(b));
                if let Some(t) = parse_md_table(b) {
                    let _ = serialize_md_table(&t);
                }
            }
            for l in src.lines() {
                let _ = (inline_spans(l), detect_line_kind(l), list_marker(l));
            }
            let _ = (outline(&src), md_to_html(&src), find_all(&src, "가", false));
        }
    }

    fn conflict_app(dir: &Path) -> egui_kittest::Harness<'static, App> {
        let d = dir.to_path_buf();
        let file = dir.join("a.md");
        egui_kittest::Harness::builder()
            .with_size(egui::vec2(800.0, 600.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, Some(file), &d))
    }

    #[test]
    fn save_asks_instead_of_overwriting_external_change() {
        let dir = std::env::temp_dir().join("mdeditor_ui_test_conflict");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.md");
        std::fs::write(&file, "원본\n").unwrap();
        let bump = |f: &Path| {
            let h = std::fs::OpenOptions::new().write(true).open(f).unwrap();
            h.set_modified(std::time::SystemTime::now() + Duration::from_secs(30))
                .unwrap();
        };

        let mut h = conflict_app(&dir);
        h.run_steps(2);
        {
            let doc = h.state_mut().docs.get_mut("a.md").unwrap();
            doc.text = "내 편집\n".into();
            doc.refresh_meta();
        }
        // 다른 프로그램이 내용을 바꿈 → 저장하면 안 되고 대화상자가 떠야 한다
        std::fs::write(&file, "외부 편집\n").unwrap();
        bump(&file);
        assert!(!h.state_mut().save_quiet("a.md"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "외부 편집\n");
        assert!(matches!(h.state().dialog, Some(Dialog::Conflict(_))));

        // 내용은 같고 시각만 바뀐 경우(클라우드 동기화 등)는 그냥 저장한다
        h.state_mut().dialog = None;
        std::fs::write(&file, "원본\n").unwrap();
        bump(&file);
        assert!(h.state_mut().save_quiet("a.md"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "내 편집\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // v0.13.1 검토에서 찾은 버그들의 회귀 테스트
    #[test]
    fn review_fixes_print_saveas_scratch_close_rename_sync() {
        let dir = std::env::temp_dir().join("mdeditor_ui_test_review_fixes");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.md"), "에이\n").unwrap();
        std::fs::write(dir.join("b.md"), "비\n").unwrap();
        let bump = |f: &Path| {
            let h = std::fs::OpenOptions::new().write(true).open(f).unwrap();
            h.set_modified(std::time::SystemTime::now() + Duration::from_secs(30))
                .unwrap();
        };
        let mut h = conflict_app(&dir);
        h.run_steps(2);

        // 1. 인쇄용 임시 HTML(file://)은 링크 허용 목록을 통과하고, 다른 file://·위험 스킴은 막힌다
        assert!(external_url_allowed(&print_url()));
        assert!(external_url_allowed("mailto:a@b.c"));
        assert!(external_url_allowed("https://example.com"));
        assert!(!external_url_allowed(
            "file:///C:/Windows/System32/calc.exe"
        ));
        assert!(!external_url_allowed("ms-msdt://x"));

        // 2. 다른 폴더로 "다른 이름 저장": 다른 탭을 저장하지 못하면 탭을 버리지 않고 멈춘다
        h.state_mut().open_file("b.md");
        {
            let d = h.state_mut().docs.get_mut("b.md").unwrap();
            d.text = "비 편집\n".into();
            d.refresh_meta();
        }
        std::fs::write(dir.join("b.md"), "외부 편집\n").unwrap();
        bump(&dir.join("b.md"));
        h.state_mut().switch_to_tab("a.md");
        h.state_mut().dialog = None;
        let outside = dir.with_extension("out").join("x.md");
        assert!(h.state_mut().save_as_path(&outside).is_err());
        assert!(!outside.exists());
        assert_eq!(h.state().docs["b.md"].text, "비 편집\n");
        assert_eq!(h.state().current.as_deref(), Some("a.md"));
        h.state_mut().dialog = None;

        // 3. 내용 있는 임시 메모 탭은 닫기 전에 묻고, 빈 메모 탭은 바로 닫는다
        h.state_mut().new_tab();
        let memo = h.state().current.clone().unwrap();
        {
            let d = h.state_mut().doc_mut().unwrap();
            d.text = "중요한 메모".into();
            d.refresh_meta();
        }
        h.state_mut().close_tab(&memo);
        assert!(matches!(
            h.state().dialog,
            Some(Dialog::Unsaved(After::CloseTab(_)))
        ));
        assert!(h.state().tabs.contains(&memo));
        h.state_mut().dialog = None;
        h.state_mut().new_tab();
        let empty = h.state().current.clone().unwrap();
        h.state_mut().close_tab(&empty);
        assert!(h.state().dialog.is_none() && !h.state().tabs.contains(&empty));

        // 4. 대소문자만 바꾸는 이름 변경
        h.state_mut().rename("a.md", "A.md").unwrap();
        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"A.md".to_string()), "{names:?}");

        // 5. 수정 시각만 바뀌면 다시 읽지 않는다 (실행 취소 기록 유지)
        {
            let d = h.state_mut().docs.get_mut("A.md").unwrap();
            d.text = "에이 편집\n".into();
            d.refresh_meta();
        }
        assert!(h.state_mut().save_quiet("A.md"));
        bump(&dir.join("A.md"));
        h.state_mut().sync_from_disk();
        let d = &h.state().docs["A.md"];
        assert!(!d.undo.is_empty());
        assert_eq!(d.mtime, modified(&dir.join("A.md")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // 붙여넣기 정리(탭 → 공백, • → -)는 일반 블록에만. 코드 블록에는 원문 그대로.
    #[test]
    fn paste_keeps_code_verbatim_and_normalizes_prose() {
        let dir = std::env::temp_dir().join("mdeditor_ui_test_paste_scope");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let d1 = dir.clone();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1200.0, 800.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, None, &d1));
        h.run_steps(3);
        for ch in ["`", "`", "`", " "] {
            h.event(egui::Event::Text(ch.into()));
            h.step();
        }
        h.run_steps(2);
        h.event(egui::Event::Paste("all:\r\n\tgo build\n• x".into()));
        h.run_steps(2);
        assert_eq!(
            h.state().doc().unwrap().text,
            "```\nall:\n\tgo build\n• x\n```"
        );

        h.state_mut().new_tab();
        h.run_steps(2);
        h.event(egui::Event::Paste("• 항목".into()));
        h.run_steps(2);
        assert_eq!(h.state().doc().unwrap().text, "- 항목");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_scratch_tab_is_kept_in_drafts() {
        let dir = std::env::temp_dir().join("mdeditor_ui_test_drafts");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.clone();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(800.0, 600.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, None, &d));
        h.run_steps(2);
        h.event(egui::Event::Text("첫째".into()));
        h.run_steps(2);
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::N);
        h.run_steps(2);
        h.event(egui::Event::Text("둘째".into()));
        h.run_steps(2);
        let joined = h.state().drafts_joined();
        let mut parts: Vec<&str> = joined.split('\u{1}').collect();
        assert_eq!(parts[0], "둘째"); // 현재 탭이 맨 앞
        parts.sort();
        assert_eq!(parts, ["둘째", "첫째"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clear_inline_formatting_and_structural_block_conversion() {
        // 1. 인라인 서식 지우기: 서식 기호와 링크·키캡은 벗기되 수식 곱셈(3 * 4)이나 이스케이프(\*)는 보존
        assert_eq!(
            clear_inline_formatting(
                "**굵게** *기울임* ~~취소~~ ==강조== `코드` [[Ctrl]] [구글](https://g.com)"
            ),
            "굵게 기울임 취소 강조 코드 Ctrl 구글"
        );
        assert_eq!(
            clear_inline_formatting("수식 3 * 4 = 12 와 \\*별표\\* 및 ~~==중첩==~~"),
            "수식 3 * 4 = 12 와 \\*별표\\* 및 중첩"
        );

        let dir = std::env::temp_dir().join("mdeditor_ui_test_review_fixes");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::create_dir_all(dir.join("images")).unwrap();
        image::RgbaImage::from_pixel(16, 16, image::Rgba([0, 150, 255, 255]))
            .save(dir.join("images/root.png"))
            .unwrap();
        let sub_file = dir.join("sub/note.md");
        std::fs::write(&sub_file, "```rust\nlet a = 1;\n```\n").unwrap();

        let d2 = dir.clone();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(900.0, 600.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| {
                let mut app = App::new(cc, None, &d2);
                app.open_file("sub/note.md");
                app
            });
        h.run_steps(2);

        // 2. 닫힌 코드 블록에서 제목 1(Ctrl+Alt+1) 전환 시 펜스가 안전하게 벗겨지고 제목으로 전환
        h.state_mut().go(0, 8);
        h.run_steps(2);
        let ctx = h.ctx.clone();
        h.state_mut().convert_block(&ctx, BlockKind::Heading(1));
        assert_eq!(h.state().doc().unwrap().text, "# let a = 1;\n");

        // 3. 하위 폴더 문서(sub/note.md)에서 루트 images/root.png 삽입 시 중복 복사 없이 ../images/root.png 상대 경로 사용
        h.state_mut()
            .insert_image_file(&ctx, &dir.join("images/root.png"));
        assert!(
            h.state()
                .doc()
                .unwrap()
                .text
                .contains("![root](../images/root.png)"),
            "{}",
            h.state().doc().unwrap().text
        );
        assert!(!dir.join("sub/images/root.png").exists());

        // 4. 백그라운드 붙여넣기 완료 시 탭이 바뀌어 있어도 원래 탭에 이미지가 들어간다
        h.state_mut().new_tab();
        h.run_steps(1);
        let scratch_tab = h.state().current.clone().unwrap();
        h.state_mut()
            .pasted
            .lock()
            .unwrap()
            .push(("sub/note.md".to_string(), Ok("paste-123.png".to_string())));
        h.state_mut().poll_pasted(&ctx);
        assert_eq!(h.state().current.as_deref(), Some(scratch_tab.as_str()));
        assert_eq!(h.state().docs[&scratch_tab].text, "");
        assert!(h.state().docs["sub/note.md"]
            .text
            .contains("![image](images/paste-123.png)"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn spellcheck_modal_fix_ignore_and_toggle() {
        let dir = std::env::temp_dir().join("mdeditor_ui_test_spellcheck");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let d2 = dir.clone();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(960.0, 680.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| {
                let mut app = App::new(cc, None, &d2);
                app.set_text("Hello teh world.\n\ntest  spacing.\n".to_string(), (0, 0));
                app
            });
        h.run_steps(2);

        // 1. Shift+F7로 실시간 밑줄 켜기/끄기 전환
        assert!(h.state().spellcheck);
        h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::F7);
        h.run_steps(2);
        assert!(!h.state().spellcheck);
        h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::F7);
        h.run_steps(2);
        assert!(h.state().spellcheck);

        // 2. F7로 맞춤법 검사 모달 열기
        // 전체 검사는 작업 스레드에서 돈다: 결과가 올 때까지 프레임을 돌린다
        let settle = |h: &mut egui_kittest::Harness<App>| {
            for _ in 0..500 {
                h.step();
                if matches!(
                    h.state().dialog,
                    Some(Dialog::Spellcheck {
                        checking: false,
                        ..
                    })
                ) {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            panic!("spellcheck worker did not finish");
        };
        h.key_press(egui::Key::F7);
        h.run_steps(2);
        assert!(matches!(h.state().dialog, Some(Dialog::Spellcheck { .. })));
        settle(&mut h);
        if let Some(Dialog::Spellcheck { issues, .. }) = &h.state().dialog {
            assert!(issues.iter().any(|i| i.word == "teh"));
        }

        // 3. 단어 무시 및 자동 교정 동작 확인
        h.state_mut().ignore_spell_word("teh");
        h.run_steps(2);
        settle(&mut h);
        let issues = if let Some(Dialog::Spellcheck { issues, .. }) = &h.state().dialog {
            assert!(issues.iter().all(|i| i.word != "teh"));
            assert!(issues.iter().any(|i| i.kind == spell::IssueKind::Spacing));
            issues.clone()
        } else {
            panic!("Spellcheck modal should remain open");
        };

        let fixed = h.state_mut().apply_all_spell_fixes(issues);
        assert_eq!(fixed, 1);
        assert_eq!(
            h.state().doc().unwrap().text,
            "Hello teh world.\n\ntest spacing.\n"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bullet_menu_converts_selected_blocks_without_blank_line_bullets() {
        // 1. 순수 연산: 여러 블록 사이 빈 줄은 건너뛰고 글머리/번호를 적용·해제한다
        let src = "첫 번째 항목\n\n두 번째 항목";
        let (bulleted, sel) = convert_lines_kind(src, (0, char_len(src)), BlockKind::Bullet);
        assert_eq!(bulleted, "- 첫 번째 항목\n\n- 두 번째 항목");
        assert_eq!(sel, (0, char_len(&bulleted)));
        let (restored, _) = convert_lines_kind(&bulleted, sel, BlockKind::Bullet);
        assert_eq!(restored, src);

        let (numbered, _) = convert_lines_kind(src, (0, char_len(src)), BlockKind::Numbered);
        assert_eq!(numbered, "1. 첫 번째 항목\n\n2. 두 번째 항목");

        // 2. UI 테스트: 다중 블록 선택(block_sel) 후 Fmt::Block(Bullet) 적용 및 토글
        let dir = std::env::temp_dir().join("mdeditor_ui_test_bullet_menu");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let d2 = dir.clone();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(960.0, 680.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| {
                let mut app = App::new(cc, None, &d2);
                app.set_text("항목 A\n\n항목 B\n".to_string(), (0, 0));
                app
            });
        h.run_steps(2);
        let ctx = h.ctx.clone();
        let len = h.state().doc().unwrap().text.trim_end().len();
        h.state_mut().active = None;
        h.state_mut().block_sel = Some(0..len);
        h.state_mut().format(&ctx, Fmt::Block(BlockKind::Bullet));
        h.run_steps(2);
        assert_eq!(h.state().doc().unwrap().text, "- 항목 A\n\n- 항목 B\n");
        // 다시 누르면 토글 해제
        h.state_mut().format(&ctx, Fmt::Block(BlockKind::Bullet));
        h.run_steps(2);
        assert_eq!(h.state().doc().unwrap().text, "항목 A\n\n항목 B\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bullet_wrapped_edit_preserves_hanging_indent_and_spacing() {
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        let p = theme::pal(false);
        let body = 15.0;
        let wrap = 220.0;
        let text = "- 아주 긴 글머리 항목이 화면 너비를 넘어서 여러 줄로 자동 줄바꿈되는 경우에도 둘째 줄이 글머리 아래로 밀리지 않아야 합니다\n- 다음 항목";
        let g = ui::edit_galley_for_test(&ctx, text, body, wrap, &p);
        assert_eq!(g.text(), text);
        // 첫 번째 항목은 여러 줄로 자동 줄바꿈되어야 하고, 모든 줄이 LIST_STEP(28px)에서 시작해야 한다
        assert!(
            g.rows.len() >= 3,
            "expected wrapped rows, got {}",
            g.rows.len()
        );
        let x0 = g.rows[0].pos.x;
        assert!((x0 - 28.0).abs() < 1.0, "expected indent ~28, got {x0}");
        for (idx, row) in g.rows.iter().enumerate() {
            assert!(
                (row.pos.x - x0).abs() < 1.0,
                "row {idx} pos.x={} != {x0}",
                row.pos.x
            );
        }
        // 같은 항목 안의 줄바꿈 간격은 body*1.6(24px)이고, 다음 항목과는 LIST_ROW_GAP(+6px)만큼 띄워진다
        let intra_dy = g.rows[1].pos.y - g.rows[0].pos.y;
        let last_first_item = g
            .rows
            .iter()
            .position(|r| r.ends_with_newline)
            .expect("first item ends with newline");
        let inter_dy = g.rows[last_first_item + 1].pos.y - g.rows[last_first_item].pos.y;
        assert!(
            (intra_dy - body * 1.6).abs() <= 1.5,
            "intra-item dy={intra_dy} should match body*1.6 ({})",
            body * 1.6
        );
        assert!(
            (inter_dy - (body * 1.6 + 6.0)).abs() <= 1.5,
            "inter-item dy={inter_dy} should include LIST_ROW_GAP ({})",
            body * 1.6 + 6.0
        );
    }

    #[test]
    fn clicking_tab_text_switches_tab() {
        use egui_kittest::kittest::Queryable;
        let dir = std::env::temp_dir().join("mdeditor_ui_test_tab_text_click");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("alpha.md");
        let b = dir.join("beta.md");
        std::fs::write(&a, "A 내용\n").unwrap();
        std::fs::write(&b, "B 내용\n").unwrap();

        let (a2, d2) = (a.clone(), dir.clone());
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(960.0, 680.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| {
                let mut app = App::new(cc, Some(a2), &d2);
                app.open_file("beta.md");
                app
            });
        h.run_steps(3);
        assert_eq!(h.state().current.as_deref(), Some("beta.md"));

        // 탭의 아이콘이 아닌 'alpha.md' 텍스트 자체를 클릭해 탭 전환이 되는지 검증
        h.get_by_label("alpha.md").click();
        h.run_steps(2);
        assert_eq!(h.state().current.as_deref(), Some("alpha.md"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn shift_tab_outdent_and_numbered_block_assignment() {
        // 1. indent_lines: 탭 문자('\t') 역들여쓰기 및 다중 블록 빈 줄 보존
        assert_eq!(
            indent_lines("\t항목", (1, 1), true),
            ("항목".into(), (0, 0))
        );
        let multi = "첫 블록\n\n둘째 블록";
        let (indented, sel) = indent_lines(multi, (0, char_len(multi)), false);
        assert_eq!(indented, "  첫 블록\n\n  둘째 블록");
        let (outdented, _) = indent_lines(&indented, sel, true);
        assert_eq!(outdented, multi);

        // 2. UI 테스트: 다중 블록 연번(번호 목록) 할당·토글 + 한글 조합 중 Shift+Tab 역들여쓰기
        let dir = std::env::temp_dir().join("mdeditor_ui_test_shift_tab_numbered");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let d2 = dir.clone();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(960.0, 680.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| {
                let mut app = App::new(cc, None, &d2);
                app.set_text("첫째\n\n둘째\n\n셋째\n".to_string(), (0, 0));
                app
            });
        h.run_steps(2);
        let ctx = h.ctx.clone();
        let len = h.state().doc().unwrap().text.trim_end().len();
        h.state_mut().active = None;
        h.state_mut().block_sel = Some(0..len);
        h.state_mut().format(&ctx, Fmt::Block(BlockKind::Numbered));
        h.run_steps(2);
        assert_eq!(
            h.state().doc().unwrap().text,
            "1. 첫째\n\n2. 둘째\n\n3. 셋째\n"
        );
        // 다시 누르면 토글 해제
        h.state_mut().format(&ctx, Fmt::Block(BlockKind::Numbered));
        h.run_steps(2);
        assert_eq!(h.state().doc().unwrap().text, "첫째\n\n둘째\n\n셋째\n");

        // 3. 한글 조합 중(ime_composing = true)이어도 Shift+Tab으로 하위 목록 역들여쓰기 동작
        h.state_mut()
            .set_text("- 부모\n  - 자식".to_string(), (11, 11));
        h.run_steps(2);
        h.state_mut().ime_composing = true;
        h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Tab);
        h.run_steps(2);
        assert_eq!(h.state().doc().unwrap().text, "- 부모\n- 자식");

        // 4. 숫자 목록 아래에 불릿 포인트 계층 들여쓰기 (붙어 있는 줄 & 빈 줄로 나뉜 블록 모두)
        let s1 = "1. 첫째\n- 하위 불릿";
        let (nested1, sel1) = indent_list(s1, (char_len(s1), char_len(s1)), false).unwrap();
        assert_eq!(nested1, "1. 첫째\n   - 하위 불릿");
        let (unnested1, _) = indent_list(&nested1, sel1, true).unwrap();
        assert_eq!(unnested1, s1);

        let s2 = "1. 첫째\n\n- 하위 불릿";
        let (nested2, _) = indent_list(s2, (char_len(s2), char_len(s2)), false).unwrap();
        assert_eq!(nested2, "1. 첫째\n   - 하위 불릿");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hardening_utf8_panics_and_xss_sanitization() {
        // 1. 표 구분선에 콜론 하나(':')가 들어가도 패닉하지 않는다
        assert_eq!(parse_table_sep(":"), None);
        assert_eq!(parse_md_table("| a |\n| : |"), None);

        // 2. 전각 공백(\u{3000}) / NBSP(\u{00A0}) 뒤 오타 검사 시 UTF-8 경계 패닉이 없다
        let s = "앞\u{3000}됬습니다 \u{00A0}되요";
        let issues = spell::check_block(s, &std::collections::HashSet::new());
        assert!(issues.iter().any(|i| i.word == "됬"));
        assert!(issues.iter().any(|i| i.word == "되요"));

        // 3. HTML 내보내기가 data:text/html, vbscript:, 제어문자 난독화 javascript: 링크를 차단한다
        let html = md_to_html(
            "[a](data:text/html,<script>alert(1)</script>) [b](vbscript:msgbox) [c](java\tscript:alert(1))",
        );
        assert!(!html.contains("data:text/html"), "{html}");
        assert!(!html.contains("vbscript:"), "{html}");
        assert!(!html.contains("javascript:"), "{html}");
    }

    #[test]
    fn empty_bullet_before_numbered_line_can_be_focused_and_deleted() {
        let s = "- 표준 제품 기능\n- \n6. AI Governance";
        let empty_bullet_caret = char_len("- 표준 제품 기능\n- ");
        // 빈 불릿("- ") 뒤 커서가 다음 줄("6. ")로 튕기지 않고 제자리에 머문다
        assert_eq!(
            clamp_hidden(s, empty_bullet_caret, None),
            empty_bullet_caret
        );

        // Backspace로 빈 불릿 줄이 깔끔하게 삭제된다
        let (after_bs, c_bs) = backspace_list_marker(s, empty_bullet_caret).unwrap();
        assert_eq!(after_bs, "- 표준 제품 기능\n6. AI Governance");
        assert_eq!(c_bs, char_len("- 표준 제품 기능"));

        // Delete로도 다음 줄("6. ")을 깨지 않고 빈 불릿 줄만 삭제된다
        let (after_del, c_del) = delete_empty_list_line(s, empty_bullet_caret).unwrap();
        assert_eq!(after_del, "- 표준 제품 기능\n6. AI Governance");
        assert_eq!(c_del, char_len("- 표준 제품 기능\n6. "));

        // 다음 줄("6. AI Governance")의 내용 시작에서 Backspace를 눌러도 현재 줄 기호("6. ")는 유지한 채 바로 위 빈 불릿("- ")만 삭제된다
        let next_line_start_caret = char_len("- 표준 제품 기능\n- \n6. ");
        let (after_next_bs, c_next_bs) = backspace_list_marker(s, next_line_start_caret).unwrap();
        assert_eq!(after_next_bs, "- 표준 제품 기능\n6. AI Governance");
        assert_eq!(c_next_bs, char_len("- 표준 제품 기능\n6. "));

        // 바로 윗줄("- 표준 제품 기능") 끝에서 Delete를 눌러도 아래 빈 불릿("- ")만 깔끔하게 삭제된다
        let prev_line_end_caret = char_len("- 표준 제품 기능");
        let (after_prev_del, c_prev_del) = delete_empty_list_line(s, prev_line_end_caret).unwrap();
        assert_eq!(after_prev_del, "- 표준 제품 기능\n6. AI Governance");
        assert_eq!(c_prev_del, prev_line_end_caret);

        // 다중 줄 블록의 첫 줄이 빈 불릿("- \n6. AI Governance")일 때 Backspace로 빈 첫 줄 전체가 삭제된다
        let top_empty = "- \n6. AI Governance";
        let (after_top_bs, c_top_bs) = backspace_list_marker(top_empty, 2).unwrap();
        assert_eq!(after_top_bs, "6. AI Governance");
        assert_eq!(c_top_bs, 3);

        // 줄머리 기호가 포함된(0..len) 단일 줄 선택에 서식을 씌워도 기호 뒤 텍스트만 감싼다
        let (wrapped, _) = wrap_selection("- 항목", (0, 4), "**", "**", "굵게");
        assert_eq!(wrapped, "- **항목**");
    }

    #[test]
    fn drag_block_selection_while_scrolling_up_preserves_bottom_anchor() {
        let dir = std::env::temp_dir().join("mdeditor_ui_test_drag_scroll_up");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut doc_text = String::new();
        for i in 1..=25 {
            doc_text.push_str(&format!("블록 {i}: 스크롤 테스트용 문단 내용입니다.\n\n"));
        }
        let d2 = dir.clone();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(800.0, 360.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| {
                let mut app = App::new(cc, None, &d2);
                app.set_text(doc_text.clone(), (0, 0));
                app
            });
        h.run_steps(3);
        // 먼저 문서 맨 끝(블록 25)이 화면 아래쪽에 보이도록 커서 이동 후 비활성화
        let last_block = h.state().doc().unwrap().blocks.last().unwrap().clone();
        h.state_mut().go(last_block.start, last_block.end);
        h.run_steps(4);
        h.state_mut().deactivate();
        h.run_steps(2);

        let (anchor_block, bottom_pos) = h
            .state()
            .canvas_rects
            .iter()
            .rev()
            .find(|(r, rect)| !r.is_empty() && rect.bottom() <= 340.0 && rect.top() >= 180.0)
            .map(|(r, rect)| (r.clone(), rect.center()))
            .expect("visible bottom block in viewport");
        let top_edge_pos = egui::pos2(bottom_pos.x, 86.0);
        // 아래쪽 블록에서 마우스 버튼을 누르고 위쪽 가장자리로 드래그하며 여러 프레임 위로 자동 스크롤
        h.input_mut()
            .events
            .push(egui::Event::PointerMoved(bottom_pos));
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: bottom_pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
        for _ in 0..18 {
            h.input_mut()
                .events
                .push(egui::Event::PointerMoved(top_edge_pos));
            h.step();
        }
        let sel = h
            .state()
            .block_sel
            .clone()
            .expect("multi-block selection active while dragging up");
        // 위로 스크롤하며 드래그해도 처음 드래그를 시작한 뒤쪽(아래쪽) 블록 끝(anchor_block.end)이 끊기지 않고 유지되어야 한다
        assert_eq!(
            sel.end, anchor_block.end,
            "bottom anchor was cut off during upward scroll: sel={sel:?}, anchor={anchor_block:?}"
        );
        assert!(
            sel.start < anchor_block.start,
            "selection should extend upward: sel={sel:?}, anchor={anchor_block:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tab_indents_text_and_builds_subitems_in_all_cases() {
        // 1. 단독·첫 불릿/넘버링 항목에서도 Tab으로 하위 단계(depth 1, 2) 진입 및 Shift+Tab 복귀
        let mut st = Vec::new();
        assert_eq!(list_item_step(&mut st, "  - 하위"), Some((1, None)));
        let mut st = Vec::new();
        assert_eq!(list_item_step(&mut st, "    - 손자"), Some((2, None)));
        let mut st = Vec::new();
        assert_eq!(list_item_step(&mut st, "  1. 번호하위"), Some((1, Some(1))));
        // 번호 항목("1. 부모") 아래 2칸 들여쓰기된 하위 불릿("  - 하위")이 있어도 부모 스택이 끊기지 않는다
        let mut st = Vec::new();
        let steps: Vec<_> = "1. 부모\n  - 하위\n2. 둘째부모"
            .split('\n')
            .map(|l| list_item_step(&mut st, l).unwrap())
            .collect();
        assert_eq!(steps, [(0, Some(1)), (1, None), (0, Some(2))]);

        // 숨긴 불릿("- ") 뒤나 굵게("**") 안쪽 맨 앞에서 연번("1. ", "4. ") 입력 시 즉시 번호 목록으로 서식화된다
        assert_eq!(
            normalize_typed_line_marker("- 4. 보안 설계", 5),
            Some(("4. 보안 설계".into(), 3))
        );
        assert_eq!(
            normalize_typed_line_marker("4. - 보안 설계", 3),
            Some(("4. 보안 설계".into(), 3))
        );
        assert_eq!(
            normalize_typed_line_marker("**1. 보안 설계**", 5),
            Some(("1. **보안 설계**".into(), 5))
        );

        // 4. 볼드(**) 설정 시 공백 포함 선택·부분 선택 해제·복수 볼드 병합·다중 줄 선택에서 ** 기호가 노출되지 않는다
        assert_eq!(
            wrap_selection("보안 및 역할 ", (0, 8), "**", "**", "굵게").0,
            "**보안 및 역할** "
        );
        assert_eq!(
            wrap_selection("**보안 및 역할 기반**", (2, 6), "**", "**", "굵게").0,
            "보안 및 **역할 기반**"
        );
        assert_eq!(
            wrap_selection("**보안 및 역할 기반**", (7, 9), "**", "**", "굵게").0,
            "**보안 및** 역할 **기반**"
        );
        assert_eq!(
            wrap_selection("앞 **보안** 뒤", (0, 10), "**", "**", "굵게").0,
            "**앞 보안 뒤**"
        );
        assert_eq!(
            wrap_selection("**보안** 및 **역할**", (0, 15), "**", "**", "굵게").0,
            "**보안 및 역할**"
        );
        assert_eq!(
            wrap_selection("- 첫째\n- 둘째", (0, 9), "**", "**", "굵게").0,
            "- **첫째**\n- **둘째**"
        );
        // 인라인 코드가 포함된 볼드+조사 조합 및 괄호 볼드+조사 다중 조합도 보기 화면 LayoutJob에서 **가 그대로 노출되지 않는다
        let pal = theme::pal(false);
        let job =
            crate::ui::inline_md_view_job("**보안(`RBAC`)**은 **중요 **하다", 16.0, &pal, false);
        assert!(
            !job.text.contains("**"),
            "exposed ** in view job: {}",
            job.text
        );
        let job2 = crate::ui::inline_md_view_job("- **항목(A)**은 설명**(B)**", 16.0, &pal, false);
        assert!(
            !job2.text.contains("**"),
            "exposed ** in mispaired CJK view job: {}",
            job2.text
        );

        // 5. 외부 챗봇 결과 붙여넣기 정규화: 굵게 감싼 연번, 유니코드 불릿, 구분선·제목·표 블록 분리
        let raw_chat = "### 1. 보안 개요\n**1. 보안 및 역할 기반 접근 제어 설계**\n\t• **역할(RBAC)**은 권한을 분리합니다.\n--- \n| 구분 | 설명 |\n| --- | --- |\n| A | B |\n마무리 문단";
        let norm_chat = normalize_pasted_markdown(raw_chat);
        assert_eq!(
            norm_chat,
            "### 1. 보안 개요\n\n1. **보안 및 역할 기반 접근 제어 설계**\n  - **역할(RBAC)**은 권한을 분리합니다.\n\n--- \n\n| 구분 | 설명 |\n| --- | --- |\n| A | B |\n\n마무리 문단"
        );

        // 2. 빈 줄(\n\n)로 나뉜 다중 불릿/넘버링 블록 선택 후 Tab 시 빈 줄을 병합하며 하위 항목으로 전환
        let doc = "1. 부모\n\n2. 자식1\n\n3. 자식2\n\n4. 다음부모";
        let c_start = char_len("1. 부모\n\n");
        let c_end = char_len("1. 부모\n\n2. 자식1\n\n3. 자식2");
        let (indented, _) = indent_list(doc, (c_start, c_end), false).unwrap();
        assert_eq!(
            indented, "1. 부모\n   1. 자식1\n   2. 자식2\n\n2. 다음부모",
            "다중 목록 블록 Tab 시 하위 항목으로 합쳐지고 뒤쪽 번호 목록도 다시 매겨져야 한다"
        );

        // 3. UI에서 일반 문단(커서가 줄 끝·중간 또는 단일 줄 선택 상태) 및 불릿에 Tab/Shift+Tab 입력 검증
        let dir = std::env::temp_dir().join("mdeditor_ui_test_tab_indent");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let d2 = dir.clone();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(800.0, 500.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| {
                let mut app = App::new(cc, None, &d2);
                app.set_text("일반 문단 텍스트".into(), (9, 9));
                app
            });
        h.run_steps(2);
        // 줄 끝에서 Tab 2번 → 글자 중간 공백이 아니라 줄 앞 4칸 들여쓰기가 되어야 한다
        h.key_press(Key::Tab);
        h.step();
        assert_eq!(h.state().doc().unwrap().text, "  일반 문단 텍스트");
        h.key_press(Key::Tab);
        h.step();
        assert_eq!(h.state().doc().unwrap().text, "    일반 문단 텍스트");
        // Shift+Tab 1번 → 2칸 내어쓰기
        h.key_press_modifiers(Modifiers::SHIFT, Key::Tab);
        h.step();
        assert_eq!(h.state().doc().unwrap().text, "  일반 문단 텍스트");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn format_painter_copies_inline_and_block_format() {
        // 굵게 + 제목 2 서식을 복사해서 일반 글에 입힌다
        let src = "## **보안** 설계\n\n평문 입니다";
        let sample = sample_format(src, (5, 5)); // 굵게 안쪽 커서
        assert_eq!(sample.inline, vec![Inline::Bold]);
        assert_eq!(sample.block, BlockKind::Heading(2));
        assert_eq!(describe_format(&sample), "굵게 · 제목 2");

        let target = "평문 입니다";
        let (out, sel) = apply_format(target, (0, 2), &sample).unwrap();
        assert_eq!(out, "## **평문** 입니다");
        let (a, b) = sel;
        assert_eq!(out.chars().skip(a).take(b - a).collect::<String>(), "평문");

        // 글자 서식이 없는 견본은 선택한 글자의 굵게를 벗긴다
        let plain = FormatSample {
            inline: Vec::new(),
            block: BlockKind::Paragraph,
        };
        let (out, _) = apply_format("앞 **강조** 뒤", (4, 6), &plain).unwrap();
        assert_eq!(out, "앞 강조 뒤");

        // 목록 줄 서식 복사 → 커서만 있어도 줄 유형이 바뀐다
        let list = sample_format("- 항목", (3, 3));
        assert_eq!(list.block, BlockKind::Bullet);
        let (out, _) = apply_format("일반 문장", (2, 2), &list).unwrap();
        assert_eq!(out, "- 일반 문장");

        // 이미 같은 서식이면 변경 없음
        assert!(apply_format("- 항목", (3, 3), &list).is_none());

        // 여러 줄 선택에는 줄마다 서식을 입힌다
        let bold = FormatSample {
            inline: vec![Inline::Bold],
            block: BlockKind::Paragraph,
        };
        let (out, _) = apply_format("첫째\n둘째", (0, 5), &bold).unwrap();
        assert_eq!(out, "**첫째**\n**둘째**");
    }

    #[test]
    fn korean_ime_types_in_save_scratch_dialog() {
        let dir = std::env::temp_dir().join("mdeditor_ui_test_dialog_ime");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let d2 = dir.clone();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(800.0, 500.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| {
                let mut app = App::new(cc, None, &d2);
                app.dialog = Some(Dialog::SaveScratch(String::new()));
                app
            });
        h.run_steps(3);
        // "한글" 조합: ㅎ → 하 → 한 → (확정) 한, ㄱ → 글
        for (pre, commit) in [
            ("ㅎ", None),
            ("하", None),
            ("한", Some("한")),
            ("ㄱ", None),
            ("그", None),
            ("글", Some("글")),
        ] {
            match commit {
                Some(c) => h.event(egui::Event::Ime(egui::ImeEvent::Commit(c.into()))),
                None => h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
                    text: pre.into(),
                    active_range_chars: None,
                })),
            }
            h.step();
            // 매 프레임 포커스를 다시 요청해 IME 조합을 끊으면 안 된다
            let interrupted = h
                .ctx
                .output(|o| o.ime.is_some_and(|i| i.should_interrupt_composition));
            assert!(!interrupted, "IME 조합이 매 프레임 끊긴다 (preedit {pre})");
        }
        h.run_steps(2);
        match h.state().dialog.as_ref() {
            Some(Dialog::SaveScratch(name)) => assert_eq!(name, "한글"),
            _ => panic!("저장 대화상자가 닫혔다"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tab_key_all_scenarios_no_panic() {
        let dir = std::env::temp_dir().join("mdeditor_ui_test_tab_all");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let cases: &[(&str, Sel)] = &[
            ("", (0, 0)),
            ("- ", (2, 2)),
            ("- 항목", (4, 4)),
            ("- 부모\n- 자식", (7, 7)),
            ("1. ", (3, 3)),
            ("1. 부모\n2. 자식", (8, 8)),
            ("1. 부모\n\n2. 자식", (10, 10)),
            ("- 부모\n\n- 자식", (8, 8)),
            ("# 제목\n\n본문", (7, 7)),
            ("```rust\nfn a() {}\n```", (10, 10)),
            ("> 인용", (4, 4)),
            ("- [ ] 할일", (6, 6)),
            ("가나다\n라마바", (5, 5)),
            ("가나다", (0, 3)),
            ("10. 열번째\n11. 열한번째", (14, 14)),
            ("| a | b |\n| --- | --- |\n| 1 | 2 |", (2, 2)),
        ];
        for (idx, (doc_text, sel)) in cases.iter().enumerate() {
            let d2 = dir.clone();
            let txt = (*doc_text).to_string();
            let s = *sel;
            let mut h = egui_kittest::Harness::builder()
                .with_size(egui::vec2(900.0, 600.0))
                .with_accessibility_check(false)
                .build_eframe(move |cc| {
                    let mut app = App::new(cc, None, &d2);
                    if !txt.is_empty() {
                        app.set_text(txt, s);
                    }
                    app
                });
            h.run_steps(2);
            h.key_press(Key::Tab);
            h.run_steps(2);
            h.key_press(Key::Tab);
            h.run_steps(2);
            h.key_press_modifiers(Modifiers::SHIFT, Key::Tab);
            h.run_steps(2);
            h.key_press_modifiers(Modifiers::SHIFT, Key::Tab);
            h.run_steps(2);
            assert!(
                h.state().current.is_some(),
                "case {idx} ({doc_text:?}) closed tab unexpectedly"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tab_merged_block_sel_clears_block_sel_and_never_panics() {
        // 빈 줄로 나뉜 하위 번호 목록 블록들을 block_sel로 잡은 뒤 Tab으로 상위 목록에 병합할 때
        // 이전 바이트 범위가 남아 status_bar(selected_chars)에서 UTF-8 경계 패닉이 나지 않아야 한다.
        let dir = std::env::temp_dir().join("mdeditor_ui_test_tab_block_sel_merge");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let d2 = dir.clone();
        let initial = "1. 부모\n\n2. 한국어\n\n3. 테스트";
        let sel_start = initial.find("2. ").unwrap();
        let sel_end = initial.len();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(900.0, 600.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| {
                let mut app = App::new(cc, None, &d2);
                app.set_text(initial.into(), (0, 0));
                app.pending_sel = None;
                app.active = None;
                app.block_sel = Some(sel_start..sel_end);
                app.block_sel_anchor = Some(sel_start);
                app.block_sel_head = Some(initial.rfind("3. ").unwrap());
                app
            });
        h.run_steps(2);
        h.key_press(Key::Tab);
        h.run_steps(3);
        assert_eq!(
            h.state().doc().unwrap().text,
            "1. 부모\n   1. 한국어\n   2. 테스트"
        );
        assert_eq!(h.state().block_sel, None);

        // Shift+Tab / 비정상 바이트 경계에서도 selected_chars()가 패닉하지 않는다
        h.key_press_modifiers(Modifiers::SHIFT, Key::Tab);
        h.run_steps(2);
        h.state_mut().block_sel = Some((sel_start + 4)..(sel_end - 1));
        let _ = h.state().selected_chars();
        h.run_steps(2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn line_numbers_toggle_and_render_test() {
        let dir = std::env::temp_dir().join("mdeditor_ui_test_line_numbers");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let d2 = dir.clone();
        let initial = "# 제목\n\n첫째 줄\n둘째 줄\n\n\n마지막 줄";
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(960.0, 640.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| {
                let mut app = App::new(cc, None, &d2);
                app.set_text(initial.into(), (0, 0));
                app.pending_sel = None;
                app.active = Some(initial.find("첫째 줄").unwrap());
                app
            });
        assert!(!h.state().line_numbers);
        h.run_steps(2);

        // Ctrl+Alt+N 으로 줄 번호 켜고 끄기
        let alt_cmd = Modifiers::COMMAND | Modifiers::ALT;
        h.key_press_modifiers(alt_cmd, Key::N);
        h.run_steps(3);
        assert!(h.state().line_numbers);

        // 전체 너비 모드에서도 줄 번호 여백과 함께 정상 렌더링된다
        h.key_press_modifiers(alt_cmd, Key::W);
        h.run_steps(2);
        assert!(h.state().full_width);
        assert!(h.state().line_numbers);

        // 다시 Ctrl+Alt+N 으로 끄기
        h.key_press_modifiers(alt_cmd, Key::N);
        h.run_steps(2);
        assert!(!h.state().line_numbers);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_as_path_switches_folder_and_saves_anywhere() {
        let base = std::env::temp_dir().join("mdeditor_test_save_as_path");
        let _ = std::fs::remove_dir_all(&base);
        let dir1 = base.join("ws1");
        let dir2 = base.join("ws2");
        std::fs::create_dir_all(&dir1).unwrap();
        let d1 = dir1.clone();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(960.0, 640.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| {
                let mut app = App::new(cc, None, &d1);
                app.set_text("# 메모\n\n내용".into(), (0, 0));
                app
            });
        h.run_steps(2);

        // 1. 작업 폴더 밖(dir2/외부/note.md)으로 저장하면 폴더가 생성·전환되고 파일이 저장된다
        let ext_path = dir2.join("외부").join("note");
        let rel = h.state_mut().save_as_path(&ext_path).unwrap();
        assert_eq!(rel, "note.md");
        assert_eq!(h.state().current.as_deref(), Some("note.md"));
        assert!(dir2.join("외부").join("note.md").is_file());
        assert_eq!(h.state().root, canon(&dir2.join("외부")).unwrap());

        // 2. 이미 저장된 파일을 같은 폴더 하위 경로(sub/copy.md)로 다른 이름 저장
        let sub_path = h.state().root.join("sub").join("copy.md");
        let rel2 = h.state_mut().save_as_path(&sub_path).unwrap();
        assert_eq!(rel2, "sub/copy.md");
        assert_eq!(h.state().current.as_deref(), Some("sub/copy.md"));
        assert!(dir2.join("외부").join("sub").join("copy.md").is_file());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn spellcheck_modal_fix_keeps_remaining_positions() {
        use egui_kittest::kittest::Queryable;
        let dir = std::env::temp_dir().join("mdeditor_ui_test_spell_shift");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let d2 = dir.clone();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(960.0, 900.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| {
                let mut app = App::new(cc, None, &d2);
                app.set_text(
                    "the the cat

Hello teh world.
"
                    .to_string(),
                    (0, 0),
                );
                app
            });
        h.run_steps(2);
        h.state_mut().open_spellcheck();
        for _ in 0..500 {
            h.step();
            if matches!(
                h.state().dialog,
                Some(Dialog::Spellcheck {
                    checking: false,
                    ..
                })
            ) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        h.run_steps(8); // 결과가 들어와 커진 모달이 자리를 잡을 때까지
        let pos = h.get_by_label("중복 삭제").rect().center();
        h.input_mut().events.push(egui::Event::PointerMoved(pos));
        h.step();
        for pressed in [true, false] {
            h.input_mut().events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            h.step();
        }
        assert_eq!(
            h.state().doc().unwrap().text,
            "the cat

Hello teh world.
"
        );
        // 확인 검사가 오기 전이든 후든 남은 항목은 문서의 같은 단어를 가리킨다
        for _ in 0..3 {
            let Some(Dialog::Spellcheck { issues, .. }) = &h.state().dialog else {
                panic!("modal closed");
            };
            let text = &h.state().doc().unwrap().text;
            assert!(issues.iter().any(|i| i.word == "teh"));
            for i in issues.iter().filter(|i| i.word == "teh") {
                assert_eq!(&text[i.range.clone()], "teh");
            }
            h.step();
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn heading_fold_hides_section_skips_arrows_and_reveals() {
        let dir = std::env::temp_dir().join("mdeditor_ui_test_fold");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("doc.md");
        let src = "# A\n\none\n\n## B\n\ntwo\n\n# C\n\nthree\n";
        std::fs::write(&file, src).unwrap();
        let (f2, d2) = (file.clone(), dir.clone());
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1000.0, 800.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, Some(f2), &d2));
        h.run_steps(3);
        h.state_mut().deactivate();
        h.run_steps(2);
        let c = src.find("# C").unwrap();
        let shown = |h: &egui_kittest::Harness<App>| -> Vec<usize> {
            h.state()
                .canvas_rects
                .iter()
                .filter(|(r, _)| !r.is_empty())
                .map(|(r, _)| r.start)
                .collect()
        };
        let click = |h: &mut egui_kittest::Harness<App>| {
            let row = h
                .state()
                .canvas_rects
                .iter()
                .find(|(r, _)| r.start == 0)
                .unwrap()
                .1;
            let dx = if h.state().line_numbers { 5.0 } else { 14.0 };
            let top = row.top() + 26.0;
            let pos = egui::pos2(row.left() - dx, top + (row.bottom() - top).min(38.4) * 0.5);
            h.input_mut().events.push(egui::Event::PointerMoved(pos));
            h.step();
            for pressed in [true, false] {
                h.input_mut().events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
                h.step();
            }
            h.run_steps(2);
        };

        // 1. 화살표 클릭: 다음 "# " 제목 전까지 숨기고, 블록 편집으로 넘어가지 않는다
        click(&mut h);
        assert!(h.state().doc().unwrap().folds.contains("# A"));
        assert_eq!(shown(&h), vec![0, c, src.find("three").unwrap()]);
        assert_eq!(h.state().active, None);
        assert_eq!(h.state().doc().unwrap().text, src);

        // 2. ↓/↑는 접힌 구간을 건너뛴다
        h.state_mut().go(0, 3);
        h.run_steps(2);
        h.key_press(egui::Key::ArrowDown);
        h.run_steps(2);
        assert_eq!(h.state().active, Some(c));
        h.key_press(egui::Key::ArrowUp);
        h.run_steps(2);
        assert_eq!(h.state().active, Some(0));
        assert!(h.state().doc().unwrap().folds.contains("# A"));

        // 3. 찾기 등으로 숨은 블록에 커서가 가면 펼친다
        let two = src.find("two").unwrap();
        h.state_mut().pending_sel = Some((two, two + 3));
        h.run_steps(3);
        assert!(h.state().doc().unwrap().folds.is_empty());
        assert_eq!(h.state().active, Some(two));

        // 4. 접힌 구간 안에서 편집 중에 접으면 편집을 끝내고 접힌 채로 둔다
        click(&mut h);
        assert!(h.state().doc().unwrap().folds.contains("# A"));
        assert_eq!(h.state().active, None);
        click(&mut h);
        assert!(h.state().doc().unwrap().folds.is_empty());
        assert_eq!(shown(&h).len(), 6);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // 큰 문서 입력 지연 측정 (기본 실행 제외):
    // cargo test --release perf_typing -- --ignored --nocapture
    #[test]
    #[ignore]
    fn perf_typing_large_doc() {
        let dir = std::env::temp_dir().join("mdeditor_perf_typing");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut src = String::new();
        for i in 0..200 {
            src.push_str(&format!(
                "## 섹션 {i}\n\n본문 문단입니다. **굵게** 와 `코드` 그리고 [링크](https://example.com) 가 섞인 한국어 문장이 이어집니다. The quick brown fox jumps over the lazy dog.\n\n- 목록 항목 하나\n- 목록 항목 둘\n  - 하위 항목\n\n1. 번호 하나\n2. 번호 둘\n\n> 인용문입니다.\n\n| 열1 | 열2 |\n| --- | --- |\n| a | b |\n\n```rust\nfn main() {{ println!(\"hi\"); }}\n```\n\n- [ ] 할 일\n\n"
            ));
        }
        // 아주 긴 문단 하나 (한 블록 안 레이아웃 비용)
        src.push_str(&"긴 문단의 문장입니다. Long paragraph sentence. ".repeat(600));
        src.push('\n');
        let file = dir.join("big.md");
        std::fs::write(&file, &src).unwrap();
        let (f2, d2) = (file.clone(), dir.clone());
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1200.0, 900.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, Some(f2), &d2));
        h.run_steps(5);
        let lines = src.lines().count();
        let blocks = h.state().doc().unwrap().blocks.len();
        eprintln!(
            "PERF doc: {lines} lines, {blocks} blocks, {} bytes",
            src.len()
        );

        let stats = |name: &str, mut v: Vec<f64>| {
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let p = |q: f64| v[((v.len() - 1) as f64 * q) as usize];
            eprintln!(
                "PERF {name:<28} median {:6.2} ms  p95 {:6.2} ms  max {:6.2} ms",
                p(0.5),
                p(0.95),
                v[v.len() - 1]
            );
        };
        let idle = |h: &mut egui_kittest::Harness<App>| -> Vec<f64> {
            (0..30)
                .map(|_| {
                    let t = Instant::now();
                    h.step();
                    t.elapsed().as_secs_f64() * 1000.0
                })
                .collect()
        };
        let typing = |h: &mut egui_kittest::Harness<App>, at: usize, label: &str| {
            h.state_mut().go(at, at);
            h.state_mut().scroll_to = Some(at);
            h.run_steps(5);
            let mut v = Vec::new();
            for i in 0..60 {
                h.event(egui::Event::Text(
                    if i % 2 == 0 { "가" } else { "a" }.into(),
                ));
                let t = Instant::now();
                h.step();
                v.push(t.elapsed().as_secs_f64() * 1000.0);
            }
            stats(label, v);
        };

        stats("idle frame (top)", idle(&mut h));
        let mid = src.find("## 섹션 100").unwrap() + "## 섹션 100\n\n".len();
        typing(&mut h, mid, "typing: middle paragraph");
        let first = "## 섹션 0\n\n".len();
        typing(&mut h, first, "typing: first paragraph");
        let long = src.rfind("긴 문단").unwrap();
        typing(&mut h, long, "typing: 26k-char paragraph");
        h.state_mut().spellcheck = false;
        typing(&mut h, long, "  ... live spellcheck off");
        typing(&mut h, mid, "  ... middle, spellcheck off");
        h.state_mut().spellcheck = true;
        stats("idle frame (bottom)", idle(&mut h));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fold_boundary_keys_never_drop_hidden_text() {
        let dir = std::env::temp_dir().join("mdeditor_fold_sweep");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = "# A\n\none\n\n## B\n\ntwo\n\n# C\n\nthree\n";
        let file = dir.join("doc.md");
        std::fs::write(&file, src).unwrap();
        let (f2, d2) = (file.clone(), dir.clone());
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1000.0, 800.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, Some(f2), &d2));
        h.run_steps(3);
        let c = src.find("# C").unwrap();
        let keys = [
            (egui::Key::Backspace, egui::Modifiers::NONE),
            (egui::Key::Delete, egui::Modifiers::NONE),
            (egui::Key::Enter, egui::Modifiers::NONE),
            (egui::Key::ArrowDown, egui::Modifiers::NONE),
            (egui::Key::ArrowRight, egui::Modifiers::NONE),
            (egui::Key::ArrowLeft, egui::Modifiers::NONE),
            (egui::Key::ArrowUp, egui::Modifiers::NONE),
        ];
        for (bs, caret) in [(0, 3), (c, c), (c, c + 3)] {
            for (key, m) in keys {
                h.state_mut().deactivate();
                h.state_mut().set_text(src.to_string(), (0, 0));
                h.state_mut().doc_mut().unwrap().folds = ["# A".to_string()].into();
                h.state_mut().go(bs, caret);
                h.run_steps(3);
                h.key_press_modifiers(m, key);
                h.run_steps(3);
                let t = &h.state().doc().unwrap().text;
                assert!(
                    t.contains("one") && t.contains("## B") && t.contains("two"),
                    "{key:?} at {bs}/{caret}: {t:?}"
                );
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn live_spellcheck_is_async_debounced_and_deferred_during_ime() {
        let dir = std::env::temp_dir().join("mdeditor_live_spell_async");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("doc.md");
        std::fs::write(&file, "첫 줄\n").unwrap();
        let (f2, d2) = (file.clone(), dir.clone());
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(900.0, 700.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, Some(f2), &d2));
        h.state_mut().go(0, 3);
        h.run_steps(3);
        // 초기 포커스 검사 완료 대기
        for _ in 0..40 {
            if h.state().spell_block_cache.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
            h.step();
        }
        assert!(h.state().spell_block_cache.is_some());
        let initial_tickets = h.state().live_spell_ticket;

        // 1. IME 조합 중(Preedit)에는 동기/비동기 검사 모두 보류된다
        h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "됬".into(),
            active_range_chars: Some(0..1),
        }));
        h.step();
        assert!(h.state().ime_composing);
        assert_eq!(h.state().live_spell_ticket, initial_tickets);
        assert!(h.state().live_spell_pending.is_none());

        // 2. Commit 직후에는 디바운스 대기 중이라 아직 워커로 제출되지 않지만,
        //    우클릭 메뉴용 active_spell_issues_sync는 즉시 결과를 돌려준다.
        h.event(egui::Event::Ime(egui::ImeEvent::Commit("됬습니다".into())));
        h.step();
        assert!(!h.state().ime_composing);
        assert!(h.state().live_spell_pending.is_some());
        assert_eq!(h.state().live_spell_ticket, initial_tickets);

        let block_text = h.state().doc().unwrap().text.clone();
        let sync_issues = h
            .state_mut()
            .active_spell_issues_sync(block_text.trim_end(), 0);
        assert!(sync_issues.iter().any(|i| i.word == "됬"));

        // 3. 다시 오타가 있는 새 내용 입력 후 디바운스를 넘기면 비동기 워커가 캐시를 채운다
        h.event(egui::Event::Text(" 몇일".into()));
        h.step();
        assert!(h.state().live_spell_pending.is_some());
        assert_eq!(h.state().live_spell_ticket, initial_tickets);
        std::thread::sleep(LIVE_SPELL_DEBOUNCE + Duration::from_millis(40));
        for _ in 0..60 {
            h.step();
            if h.state()
                .spell_block_cache
                .as_ref()
                .is_some_and(|(_, _, _, v)| v.iter().any(|i| i.word == "몇일"))
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(h.state().live_spell_ticket > initial_tickets);
        assert!(h
            .state()
            .spell_block_cache
            .as_ref()
            .is_some_and(|(_, _, _, v)| v.iter().any(|i| i.word == "몇일")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn status_bar_current_line_cache_matches_exact_line() {
        let dir = std::env::temp_dir().join("mdeditor_line_cache");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = "첫째 블록\n둘째 줄\n\n셋째 블록 첫줄\n셋째 블록 둘째줄\n\n마지막 블록\n";
        let file = dir.join("doc.md");
        std::fs::write(&file, src).unwrap();
        let (f2, d2) = (file.clone(), dir.clone());
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(900.0, 700.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, Some(f2), &d2));
        h.run_steps(2);

        let expected = |state: &App| {
            let d = state.doc().unwrap();
            let (_, caret) = state.last_sel.unwrap();
            let b = byte_idx(&d.text, caret.min(d.chars));
            1 + d.text[..b].bytes().filter(|&x| x == b'\n').count()
        };

        for pos in [
            0,
            src.find("둘째 줄").unwrap() + 3,
            src.find("셋째 블록 둘째줄").unwrap() + 3,
            src.find("마지막 블록").unwrap() + 3,
        ] {
            h.state_mut().go(pos, pos);
            h.run_steps(2);
            let got = h.state_mut().current_line_number();
            assert_eq!(got, Some(expected(h.state())), "pos={pos}");
            // 2차 호출은 O(1) status_line_cache 적중
            assert_eq!(h.state_mut().current_line_number(), got);
        }

        // 활성 블록 안 타이핑 및 줄바꿈 후에도 캐시가 정확한 줄 번호를 낸다
        h.event(egui::Event::Text("\n새 줄 한글".into()));
        h.run_steps(2);
        let got = h.state_mut().current_line_number();
        assert_eq!(got, Some(expected(h.state())));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn background_auto_save_writes_handles_concurrent_edits_and_conflicts() {
        let dir = std::env::temp_dir().join("mdeditor_bg_autosave");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("note.md");
        std::fs::write(&file, "초기\n").unwrap();
        let (f2, d2) = (file.clone(), dir.clone());
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(800.0, 600.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, Some(f2), &d2));
        h.run_steps(2);

        // 1. 일반 백그라운드 저장: 완료 후 dirty 해제
        {
            let doc = h.state_mut().docs.get_mut("note.md").unwrap();
            doc.text = "백그라운드 저장\n".into();
            doc.refresh_meta();
        }
        let ctx = h.ctx.clone();
        h.state_mut().submit_auto_save(&ctx, "note.md");
        assert!(h.state().auto_save_in_flight.contains("note.md"));
        h.state_mut().wait_auto_save();
        assert!(!h.state().auto_save_in_flight.contains("note.md"));
        assert!(!h.state().docs.get("note.md").unwrap().dirty());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "백그라운드 저장\n");

        // 2. 저장 진행 중 추가 편집이 있으면 dirty 상태를 유지한다
        {
            let doc = h.state_mut().docs.get_mut("note.md").unwrap();
            doc.text = "저장 1단계\n".into();
            doc.refresh_meta();
        }
        h.state_mut().submit_auto_save(&ctx, "note.md");
        {
            let doc = h.state_mut().docs.get_mut("note.md").unwrap();
            doc.text = "저장 2단계\n".into();
            doc.edited_at = Some(Instant::now());
            doc.refresh_meta();
        }
        h.state_mut().wait_auto_save();
        assert!(h.state().docs.get("note.md").unwrap().dirty());

        // 3. 외부 변경 시 Conflict 대화상자를 띄우고 파일을 덮어쓰지 않는다
        assert!(h.state_mut().save_quiet("note.md"));
        std::fs::write(&file, "외부 수정\n").unwrap();
        let fh = std::fs::OpenOptions::new().write(true).open(&file).unwrap();
        fh.set_modified(std::time::SystemTime::now() + Duration::from_secs(30))
            .unwrap();
        drop(fh);
        {
            let doc = h.state_mut().docs.get_mut("note.md").unwrap();
            doc.text = "내 미저장 수정\n".into();
            doc.refresh_meta();
        }
        h.state_mut().submit_auto_save(&ctx, "note.md");
        h.state_mut().wait_auto_save();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "외부 수정\n");
        assert!(matches!(h.state().dialog, Some(Dialog::Conflict(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hanging_line_cache_reuses_unchanged_list_lines() {
        let dir = std::env::temp_dir().join("mdeditor_hanging_cache");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = "- 첫째 항목입니다\n- 둘째 항목입니다\n- 셋째 항목입니다\n";
        let file = dir.join("list.md");
        std::fs::write(&file, src).unwrap();
        let (f2, d2) = (file.clone(), dir.clone());
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(900.0, 700.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, Some(f2), &d2));
        h.state_mut().go(0, src.find("둘째").unwrap());
        h.run_steps(3);
        let entries_before = h.state().hanging_line_cache.1.len();
        assert!(entries_before >= 3, "entries={entries_before}");

        // 둘째 줄만 고쳤을 때 기존 첫째/셋째 줄 캐시는 유지되고 새 줄만 1개 추가된다
        h.event(egui::Event::Text("가".into()));
        h.run_steps(2);
        let entries_after = h.state().hanging_line_cache.1.len();
        assert_eq!(entries_after, entries_before + 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn todo_checklist_toggles_in_edit_and_view_modes_and_supports_typing_shortcuts() {
        use egui_kittest::kittest::Queryable;

        // 1. 타이핑 단축 입력([], [ ], [x], - [], * [], + []) 및 블록 유형 변환 검증
        for (input, expect_prefix) in [
            ("[] 할 일", "- [ ] 할 일"),
            ("[ ] 할 일", "- [ ] 할 일"),
            ("[x] 완료", "- [x] 완료"),
            ("[X] 완료", "- [x] 완료"),
            ("- [] 할 일", "- [ ] 할 일"),
            ("* [] 할 일", "* [ ] 할 일"),
            ("+ [] 할 일", "+ [ ] 할 일"),
        ] {
            let caret = input.find(']').unwrap() + 2;
            let (out, _) = textops::normalize_typed_line_marker(input, caret)
                .unwrap_or_else(|| panic!("단축 입력이 정규화되어야 함: {input:?}"));
            assert_eq!(out, expect_prefix);
        }
        assert_eq!(
            textops::detect_line_kind("* [X] 완료됨").1,
            Some(textops::BlockKind::Todo)
        );
        assert_eq!(
            textops::toggle_todo_lines("일반 줄", (0, 0)).0,
            "- [ ] 일반 줄"
        );
        assert_eq!(
            textops::toggle_todo_lines("* [x] 끝남", (0, 0)).0,
            "- [ ] 끝남"
        );
        assert_eq!(
            textops::convert_lines_kind("* [x] 끝남", (0, 0), textops::BlockKind::Todo).0,
            "끝남"
        );

        // 2. 보기 모드와 편집 모드 양쪽에서 체크박스 클릭으로 [ ] <-> [x] 전환
        let dir = std::env::temp_dir().join("mdeditor_todo_checklist_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = "- [ ] 첫 번째 할 일\n- [x] 두 번째 완료";
        let file = dir.join("todo.md");
        std::fs::write(&file, src).unwrap();
        let (f2, d2) = (file.clone(), dir.clone());
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(960.0, 680.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, Some(f2), &d2));
        h.run_steps(3);

        // 보기 모드: 첫 번째 체크박스([ ]) 클릭 -> [x]
        {
            let boxes = h.get_all_by_role(egui::accesskit::Role::CheckBox);
            assert_eq!(boxes.count(), 2);
        }
        h.get_all_by_role(egui::accesskit::Role::CheckBox)
            .next()
            .unwrap()
            .click();
        h.run_steps(3);
        assert_eq!(
            h.state().doc().unwrap().text,
            "- [x] 첫 번째 할 일\n- [x] 두 번째 완료"
        );

        // 편집 모드로 진입 후 두 번째 체크박스([x]) 클릭 -> [ ]
        h.state_mut().go(0, src.len());
        h.run_steps(3);
        assert!(h.state().active.is_some());
        h.get_all_by_role(egui::accesskit::Role::CheckBox)
            .nth(1)
            .unwrap()
            .click();
        h.run_steps(3);
        assert_eq!(
            h.state().doc().unwrap().text,
            "- [x] 첫 번째 할 일\n- [ ] 두 번째 완료"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn line_numbers_show_every_line_of_lists_and_multiline_blocks() {
        let dir = std::env::temp_dir().join("mdeditor_multiline_linenum_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let d2 = dir.clone();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(960.0, 680.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| {
                let mut app = App::new(cc, None, &d2);
                app.line_numbers = true;
                app
            });
        h.run_steps(2);

        let check_offsets =
            |h: &mut egui_kittest::Harness<App>, block: &str, expected: &[usize]| {
                let p = theme::pal(false);
                let test_ctx = egui::Context::default();
                theme::install_fonts(&test_ctx);
                let mut offsets = Vec::new();
                let mut out_frame = test_ctx.run_ui(egui::RawInput::default(), |ui| {
                    ui.allocate_ui(egui::vec2(640.0, 600.0), |ui| {
                        let out = h.state_mut().render_block(
                            ui,
                            &p,
                            block,
                            0..block.len(),
                            0,
                            15.0,
                            640.0,
                            "file:///",
                        );
                        offsets = out.line_ys;
                    });
                });
                out_frame.textures_delta.clear();
                offsets.sort_by_key(|&(off, _)| off);
                offsets.dedup_by_key(|x| x.0);
                let got: Vec<usize> = offsets.iter().map(|&(off, _)| off).collect();
                assert_eq!(got, expected, "block={block:?}, offsets={offsets:?}");
                for w in offsets.windows(2) {
                    assert!(
                        w[1].1 > w[0].1,
                        "줄 번호 Y 좌표는 증가해야 함: {:?} in {block:?}",
                        offsets
                    );
                }
            };

        check_offsets(&mut h, "- 첫째\n  - 중첩\n- 둘째\n- 셋째", &[0, 1, 2, 3]);
        check_offsets(&mut h, "1. 하나\n2. 둘\n3. 셋", &[0, 1, 2]);
        check_offsets(&mut h, "- [ ] 할 일\n* [x] 완료", &[0, 1]);
        check_offsets(&mut h, "첫 줄\n둘째 줄\n셋째 줄", &[0, 1, 2]);

        // 실제 App 캔버스에서 편집 모드 + 보기 모드 양쪽 렌더링도 패닉 없이 완료된다
        let doc_text = "- 첫째\n  - 중첩\n- 둘째\n\n1. 번호1\n2. 번호2";
        h.state_mut().set_text(doc_text.into(), (0, 0));
        h.state_mut().pending_sel = None;
        h.state_mut().go(0, 2);
        h.run_steps(3);
        assert!(h.state().line_numbers);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn font_switch_clears_view_galley_caches_on_next_frame_after_atlas_reload() {
        use egui_kittest::kittest::Queryable;

        let dir = std::env::temp_dir().join("mdeditor_font_switch_galley_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = "# **제목**\n\n본문 **강조** 문단입니다.\n\n```rust\nlet x = 1;\n```\n";
        let file = dir.join("font.md");
        std::fs::write(&file, src).unwrap();
        let (f2, d2) = (file.clone(), dir.clone());
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1024.0, 720.0))
            .with_accessibility_check(false)
            .build_eframe(move |cc| App::new(cc, Some(f2), &d2));
        h.run_steps(3);

        assert!(h.state().active.is_none());
        let inline_before: Vec<usize> = h
            .state()
            .inline_galley_cache
            .values()
            .map(|(g, _)| std::sync::Arc::as_ptr(g) as usize)
            .collect();
        let code_before: Vec<usize> = h
            .state()
            .code_galley_cache
            .values()
            .map(|(g, _)| std::sync::Arc::as_ptr(g) as usize)
            .collect();
        assert!(!inline_before.is_empty());
        assert!(!code_before.is_empty());

        let (target_id, target_label) = theme::available_fonts()
            .iter()
            .find(|(id, _)| id != "auto")
            .expect("기본 이외의 글꼴 프리셋이 있어야 함")
            .clone();

        // Frame N: 상태바 글꼴 메뉴(canvas 이전 실행)로 글꼴을 바꾸고, 다음 프레임 아틀라스 로드 후 캐시가 비워진다
        h.get_by_label("글꼴: 기본").click();
        h.run_steps(2);
        h.get_by_label(target_label.as_str()).click();
        h.run_steps(2);
        if h.state().clear_galleys_next_frame {
            h.step();
        }
        assert!(!h.state().clear_galleys_next_frame);
        assert_eq!(h.state().font_family, target_id);
        let inline_after: Vec<usize> = h
            .state()
            .inline_galley_cache
            .values()
            .map(|(g, _)| std::sync::Arc::as_ptr(g) as usize)
            .collect();
        let code_after: Vec<usize> = h
            .state()
            .code_galley_cache
            .values()
            .map(|(g, _)| std::sync::Arc::as_ptr(g) as usize)
            .collect();
        assert!(!inline_after.is_empty());
        assert!(!code_after.is_empty());
        assert!(
            inline_after.iter().all(|p| !inline_before.contains(p)),
            "이전 아틀라스 기준 인라인 Galley가 남아 있으면 안 됨"
        );
        assert!(
            code_after.iter().all(|p| !code_before.contains(p)),
            "이전 아틀라스 기준 코드 Galley가 남아 있으면 안 됨"
        );

        // 가운데 맞춤 <-> 전체 너비 전환 후에도 이전 아틀라스 Galley가 재사용되지 않는다
        h.state_mut().full_width = true;
        h.run_steps(2);
        h.state_mut().full_width = false;
        h.run_steps(2);
        assert!(
            h.state()
                .inline_galley_cache
                .values()
                .all(|(g, _)| !inline_before.contains(&(std::sync::Arc::as_ptr(g) as usize))),
            "가운데 맞춤 복귀 후에도 이전 아틀라스 인라인 Galley가 남아 있으면 안 됨"
        );
        assert!(
            h.state()
                .code_galley_cache
                .values()
                .all(|(g, _)| !code_before.contains(&(std::sync::Arc::as_ptr(g) as usize))),
            "가운데 맞춤 복귀 후에도 이전 아틀라스 코드 Galley가 남아 있으면 안 됨"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
