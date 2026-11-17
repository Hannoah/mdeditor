#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// MDEditor — 순수 로컬 네이티브 Markdown 편집기 (eframe/egui). 브라우저·웹뷰·로컬 서버 없음.
// 실행: mdeditor [폴더|파일]
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui;
use egui::text::{CCursor, CCursorRange};
use egui::{Key, Modifiers};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

mod mermaid;
#[cfg(windows)] // 설치·제거 마법사는 Windows 전용 (PowerShell 엔진)
mod setup;
mod theme;
mod ui;

const AUTO_SAVE_DELAY: Duration = Duration::from_secs(2);
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

fn main() -> eframe::Result {
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
            let mut w = eframe::WgpuConfiguration::default().with_surface_config(eframe::SurfaceConfig {
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
fn save_atomic(path: &Path, body: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(body)?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
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
    view_active: Option<usize>,
    view_blank: usize,
    view_block_sel: Option<Range<usize>>,
    view_block_sel_anchor: Option<usize>,
    view_block_sel_head: Option<usize>,
    view_last_sel: Option<Sel>,
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
            view_active: None,
            view_blank: 0,
            view_block_sel: None,
            view_block_sel_anchor: None,
            view_block_sel_head: None,
            view_last_sel: None,
        };
        doc.refresh_meta();
        if doc.text.is_empty() {
            doc.view_active = Some(0);
            doc.view_last_sel = Some((0, 0));
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
            view_active: empty.then_some(0),
            view_blank: 0,
            view_block_sel: None,
            view_block_sel_anchor: None,
            view_block_sel_head: None,
            view_last_sel: empty.then_some((0, 0)),
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

// ---------- 편집 연산 (문자 인덱스 기준, 순수 함수) ----------

type Sel = (usize, usize); // (시작, 끝) 문자 인덱스, 시작 <= 끝

fn byte_idx(s: &str, ci: usize) -> usize {
    s.char_indices().nth(ci).map_or(s.len(), |(b, _)| b)
}

fn char_len(s: &str) -> usize {
    s.chars().count()
}

fn line_start(s: &str, ci: usize) -> usize {
    let b = byte_idx(s, ci);
    s[..b].rfind('\n').map_or(0, |i| char_len(&s[..=i]))
}

fn line_end(s: &str, ci: usize) -> usize {
    let b = byte_idx(s, ci);
    s[b..]
        .find('\n')
        .map_or(char_len(s), |i| ci + char_len(&s[b..b + i]))
}

fn replace_range(s: &str, (a, b): Sel, with: &str) -> String {
    let (ba, bb) = (byte_idx(s, a), byte_idx(s, b));
    format!("{}{}{}", &s[..ba], with, &s[bb..])
}

// 선택 영역을 prefix/suffix로 감싼다. 선택이 없으면 placeholder를 넣고 선택해 둔다.
// 이미 같은 기호로 감싸져 있으면 벗긴다(토글). 링크는 바로 주소를 칠 수 있게 URL 자리를 선택한다.
fn wrap_selection(s: &str, (a, b): Sel, pre: &str, suf: &str, placeholder: &str) -> (String, Sel) {
    let (ba, bb) = (byte_idx(s, a), byte_idx(s, b));
    let (before, sel, after) = (&s[..ba], &s[ba..bb], &s[bb..]);
    let (pl, sl) = (char_len(pre), char_len(suf));
    if a != b {
        // 기울임(*)이 굵게(**)의 별표를 벗기지 않게 한다
        let bold_not_italic = pre == "*" && before.ends_with("**") && !before.ends_with("***");
        if before.ends_with(pre) && after.starts_with(suf) && !bold_not_italic {
            let out = format!(
                "{}{sel}{}",
                &before[..before.len() - pre.len()],
                &after[suf.len()..]
            );
            return (out, (a - pl, b - pl));
        }
        let inner_bold = pre == "*" && sel.starts_with("**") && !sel.starts_with("***");
        if sel.len() >= pre.len() + suf.len()
            && sel.starts_with(pre)
            && sel.ends_with(suf)
            && !inner_bold
        {
            let core = &sel[pre.len()..sel.len() - suf.len()];
            return (format!("{before}{core}{after}"), (a, b - pl - sl));
        }
    }
    let inner = if a == b { placeholder } else { sel };
    let out = format!("{before}{pre}{inner}{suf}{after}");
    let start = a + pl;
    let end = start + char_len(inner);
    match suf.strip_prefix("](").and_then(|u| u.strip_suffix(')')) {
        Some(url) if a != b => (out, (end + 2, end + 2 + char_len(url))),
        _ => (out, (start, end)),
    }
}

// Tab / Shift+Tab: 선택된 줄 전체를 2칸 들여쓰기·내어쓰기
fn indent_lines(s: &str, (a, b): Sel, outdent: bool) -> (String, Sel) {
    let (ls, le) = (line_start(s, a), line_end(s, b));
    let block = &s[byte_idx(s, ls)..byte_idx(s, le)];
    let mut first_delta: isize = 0;
    let lines: Vec<String> = block
        .split('\n')
        .enumerate()
        .map(|(i, l)| {
            let (new, delta) = if outdent {
                let n = l.len() - l.trim_start_matches(' ').len();
                let n = n.min(2);
                (l[n..].to_string(), -(n as isize))
            } else {
                (format!("  {l}"), 2)
            };
            if i == 0 {
                first_delta = delta;
            }
            new
        })
        .collect();
    let new_block = lines.join("\n");
    let out = replace_range(s, (ls, le), &new_block);
    let new_le = ls + char_len(&new_block);
    let na = (a as isize + first_delta).max(ls as isize) as usize;
    let nb = if a == b {
        na
    } else {
        (b as isize + (new_le as isize - le as isize)) as usize
    };
    (out, (na, nb))
}

// 목록 줄 분석: (들여쓰기, 다음 줄에 쓸 기호, 현재 기호 길이(문자), 내용이 비었는지)
fn list_marker(line: &str) -> Option<(String, String, usize, bool)> {
    let indent_len = line.len() - line.trim_start_matches(' ').len();
    let (indent, rest) = line.split_at(indent_len);
    for task in ["- [ ] ", "- [x] ", "- [X] ", "* [ ] ", "* [x] "] {
        if let Some(body) = rest.strip_prefix(task) {
            let next = format!("{} [ ] ", &task[..1]);
            return Some((indent.into(), next, char_len(task), body.trim().is_empty()));
        }
    }
    for bullet in ["- ", "* ", "+ "] {
        if let Some(body) = rest.strip_prefix(bullet) {
            return Some((indent.into(), bullet.into(), 2, body.trim().is_empty()));
        }
    }
    let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && digits < 10 {
        let after = &rest[digits..];
        for sep in [". ", ") "] {
            if let Some(body) = after.strip_prefix(sep) {
                let n: u64 = rest[..digits].parse().ok()?;
                let next = format!("{}{}", n + 1, sep);
                return Some((indent.into(), next, digits + 2, body.trim().is_empty()));
            }
        }
    }
    None
}

// 힙 할당 없이 줄 앞 목록 기호의 바이트 길이만 계산한다 (md_job 레이아웃 전용)
fn list_marker_bytes(line: &str) -> Option<usize> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    let rest = &line[indent..];
    for task in ["- [ ] ", "- [x] ", "- [X] ", "* [ ] ", "* [x] "] {
        if rest.starts_with(task) {
            return Some(indent + task.len());
        }
    }
    for bullet in ["- ", "* ", "+ "] {
        if rest.starts_with(bullet) {
            return Some(indent + bullet.len());
        }
    }
    let digits = rest.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits > 0 && digits < 10 {
        let after = &rest[digits..];
        if after.starts_with(". ") || after.starts_with(") ") {
            return Some(indent + digits + 2);
        }
    }
    None
}

// Enter: 목록이면 다음 기호를 이어 쓰고, 빈 항목이면 목록을 끝낸다. 목록이 아니면 None.
fn smart_enter(s: &str, cursor: usize) -> Option<(String, usize)> {
    let ls = line_start(s, cursor);
    let line = &s[byte_idx(s, ls)..byte_idx(s, line_end(s, cursor))];
    let (indent, next, marker_len, empty) = list_marker(line)?;
    let marker_end = ls + char_len(&indent) + marker_len;
    if cursor < marker_end {
        return None; // 기호 앞/중간에서 Enter는 일반 줄바꿈
    }
    if empty {
        let out = replace_range(s, (ls, line_end(s, cursor)), "");
        return Some((out, ls));
    }
    let ins = format!("\n{indent}{next}");
    let out = replace_range(s, (cursor, cursor), &ins);
    Some((out, cursor + char_len(&ins)))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BlockKind {
    Paragraph,
    Heading(usize),
    Todo,
    Bullet,
    Numbered,
    Quote,
    Code,
    Callout,
    Toggle,
}

// 줄 앞의 마크다운 블록 기호 분석: (들여쓰기 바이트 길이, 현재 종류, 기호 바이트 길이)
fn detect_line_kind(line: &str) -> (usize, Option<BlockKind>, usize) {
    let indent = line.len() - line.trim_start_matches(' ').len();
    let rest = &line[indent..];
    if let Some(lv) = heading_level(rest) {
        let m = (lv + 1).min(rest.len());
        return (indent, Some(BlockKind::Heading(lv)), m);
    }
    for task in ["- [ ] ", "- [x] ", "- [X] ", "* [ ] ", "* [x] ", "* [X] "] {
        if rest.starts_with(task) {
            return (indent, Some(BlockKind::Todo), task.len());
        }
    }
    for bullet in ["- ", "* ", "+ "] {
        if rest.starts_with(bullet) {
            return (indent, Some(BlockKind::Bullet), bullet.len());
        }
    }
    let digits = rest.bytes().take_while(|b| b.is_ascii_digit()).count();
    if (1..10).contains(&digits) {
        for sep in [". ", ") "] {
            if rest[digits..].starts_with(sep) {
                return (indent, Some(BlockKind::Numbered), digits + sep.len());
            }
        }
    }
    if let Some(after) = rest.strip_prefix('>') {
        let m = 1 + usize::from(after.starts_with(' '));
        return (indent, Some(BlockKind::Quote), m);
    }
    (indent, None, 0)
}

// 선택된 줄들의 블록 유형(제목·목록·할일·인용·일반 문단)을 전환한다 (이미 같은 유형이면 일반 문단으로 토글)
fn convert_lines_kind(s: &str, (a, b): Sel, kind: BlockKind) -> (String, Sel) {
    let (ls, le) = (line_start(s, a), line_end(s, b));
    let block = &s[byte_idx(s, ls)..byte_idx(s, le)];
    let lines_vec: Vec<&str> = block.split('\n').collect();
    let all_same = kind != BlockKind::Paragraph
        && lines_vec
            .iter()
            .filter(|l| !l.trim().is_empty() || lines_vec.len() == 1)
            .all(|l| detect_line_kind(l).1 == Some(kind))
        && lines_vec
            .iter()
            .any(|l| !l.trim().is_empty() || lines_vec.len() == 1);
    let target = if all_same { BlockKind::Paragraph } else { kind };
    let mut first_delta: isize = 0;
    let out_lines: Vec<String> = lines_vec
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let (indent, _, marker) = detect_line_kind(l);
            let body = &l[indent + marker..];
            let keep_indent = if matches!(target, BlockKind::Heading(_)) {
                ""
            } else {
                &l[..indent]
            };
            let prefix = match target {
                BlockKind::Paragraph => String::new(),
                BlockKind::Heading(n) => format!("{} ", "#".repeat(n.clamp(1, 6))),
                BlockKind::Todo => "- [ ] ".to_string(),
                BlockKind::Bullet => "- ".to_string(),
                BlockKind::Numbered => format!("{}. ", i + 1),
                BlockKind::Quote => "> ".to_string(),
                _ => String::new(),
            };
            let new_line = format!("{keep_indent}{prefix}{body}");
            if i == 0 {
                first_delta = char_len(&new_line) as isize - char_len(l) as isize;
            }
            new_line
        })
        .collect();
    let new_block = out_lines.join("\n");
    let out = replace_range(s, (ls, le), &new_block);
    let new_le = ls + char_len(&new_block);
    let na = (a as isize + first_delta).max(ls as isize) as usize;
    let nb = if a == b {
        na
    } else {
        (b as isize + (new_le as isize - le as isize)).max(na as isize) as usize
    };
    (out, (na.min(new_le), nb.min(new_le)))
}

// Ctrl+Enter: 할 일 체크박스 완료/미완료 토글 (일반 줄이면 할 일 항목으로 전환)
fn toggle_todo_lines(s: &str, (a, b): Sel) -> (String, Sel) {
    let (ls, le) = (line_start(s, a), line_end(s, b));
    let block = &s[byte_idx(s, ls)..byte_idx(s, le)];
    let mut first_delta: isize = 0;
    let out_lines: Vec<String> = block
        .split('\n')
        .enumerate()
        .map(|(i, l)| {
            let indent = l.len() - l.trim_start_matches(' ').len();
            let (ind, rest) = l.split_at(indent);
            let new_line = if let Some(body) = rest
                .strip_prefix("- [ ] ")
                .or_else(|| rest.strip_prefix("* [ ] "))
            {
                format!("{ind}- [x] {body}")
            } else if let Some(body) = rest
                .strip_prefix("- [x] ")
                .or_else(|| rest.strip_prefix("- [X] "))
                .or_else(|| rest.strip_prefix("* [x] "))
                .or_else(|| rest.strip_prefix("* [X] "))
            {
                format!("{ind}- [ ] {body}")
            } else {
                let (_, _, marker) = detect_line_kind(l);
                format!("{ind}- [ ] {}", &l[indent + marker..])
            };
            if i == 0 {
                first_delta = char_len(&new_line) as isize - char_len(l) as isize;
            }
            new_line
        })
        .collect();
    let new_block = out_lines.join("\n");
    let out = replace_range(s, (ls, le), &new_block);
    let new_le = ls + char_len(&new_block);
    let na = (a as isize + first_delta).max(ls as isize) as usize;
    let nb = if a == b {
        na
    } else {
        (b as isize + (new_le as isize - le as isize)).max(na as isize) as usize
    };
    (out, (na.min(new_le), nb.min(new_le)))
}

// 찾기: 모든 일치 구간(문자 인덱스). 대소문자 구분·ASCII 검색은 힙 할당 없이 바이트 단위로 고속 스캔한다.
fn find_all(s: &str, query: &str, case: bool) -> Vec<Sel> {
    if query.is_empty() || query.len() > s.len() {
        return Vec::new();
    }
    if case {
        let q_chars = char_len(query);
        let mut out = Vec::new();
        let (mut last_b, mut ci) = (0, 0);
        for (b, _) in s.match_indices(query) {
            ci += char_len(&s[last_b..b]);
            out.push((ci, ci + q_chars));
            last_b = b + query.len();
            ci += q_chars;
        }
        return out;
    }
    if query.is_ascii() {
        let qb = query.as_bytes();
        let sb = s.as_bytes();
        let q_len = qb.len();
        let mut out = Vec::new();
        let (mut i, mut last_b, mut ci) = (0, 0, 0);
        while i + q_len <= sb.len() {
            if sb[i..i + q_len].eq_ignore_ascii_case(qb)
                && s.is_char_boundary(i)
                && s.is_char_boundary(i + q_len)
            {
                ci += char_len(&s[last_b..i]);
                out.push((ci, ci + q_len));
                i += q_len;
                last_b = i;
                ci += q_len;
            } else {
                i += 1;
            }
        }
        return out;
    }
    let norm = |c: char| c.to_lowercase().next().unwrap_or(c);
    let needle: Vec<char> = query.chars().map(norm).collect();
    let hay: Vec<char> = s.chars().map(norm).collect();
    if needle.is_empty() || needle.len() > hay.len() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        if hay[i..i + needle.len()] == needle[..] {
            out.push((i, i + needle.len()));
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

fn replace_all(s: &str, matches: &[Sel], with: &str) -> String {
    if matches.is_empty() {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut prev_b = 0usize;
    let mut prev_c = 0usize;
    for &(a, b) in matches {
        let ba = prev_b + byte_idx(&s[prev_b..], a.saturating_sub(prev_c));
        let bb = ba + byte_idx(&s[ba..], b.saturating_sub(a));
        out.push_str(&s[prev_b..ba]);
        out.push_str(with);
        prev_b = bb;
        prev_c = b;
    }
    out.push_str(&s[prev_b..]);
    out
}

// 목차: (레벨, 제목, 줄 번호, 줄 시작 문자 인덱스, 줄 시작 바이트 인덱스). 코드 펜스 안은 제외.
// 일반 줄에서는 UTF-8 문자 수(char_len)를 세지 않고 제목 줄을 만났을 때만 증분 계산한다.
fn outline_with_bytes_into(s: &str, out: &mut Vec<(usize, String, usize, usize, usize)>) {
    out.clear();
    let (mut in_code, mut ci, mut bi, mut last_bi) = (false, 0, 0, 0);
    for (n, line) in s.split('\n').enumerate() {
        let t = line.trim_start();
        match t.as_bytes().first() {
            Some(b'`' | b'~') if t.starts_with("```") || t.starts_with("~~~") => {
                in_code = !in_code;
            }
            Some(b'#') if !in_code => {
                let level = t.bytes().take_while(|&c| c == b'#').count();
                if (1..=6).contains(&level) && t.as_bytes().get(level) == Some(&b' ') {
                    ci += char_len(&s[last_bi..bi]);
                    last_bi = bi;
                    out.push((
                        level,
                        t[level..].trim().trim_end_matches('#').trim().into(),
                        n,
                        ci,
                        bi,
                    ));
                }
            }
            _ => {}
        }
        bi += line.len() + 1;
    }
}

#[cfg(test)]
fn outline(s: &str) -> Vec<(usize, String, usize, usize)> {
    let mut out = Vec::new();
    outline_with_bytes_into(s, &mut out);
    out.into_iter()
        .map(|(l, t, n, ci, _)| (l, t, n, ci))
        .collect()
}

// 미리보기 전용 변환. 바이트 길이를 보존해서 체크박스 위치가 원문과 1:1로 대응한다.
//   :::toggle 제목 → ##### ▸ 제목,  ::: → 공백,  ==형광펜== → **형광펜**,  [[Ctrl]] → ``Ctrl``
// 확장 문법이 없는 일반 블록은 힙 할당 없이 원본 슬라이스(Cow::Borrowed)를 그대로 돌려준다.
fn preprocess(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.contains(":::") && !s.contains("==") && !s.contains("[[") {
        return std::borrow::Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut in_code = false;
    for (i, line) in s.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_code = !in_code;
            out.push_str(line);
        } else if in_code {
            out.push_str(line);
        } else if let Some(rest) = line.strip_prefix(":::toggle") {
            out.push_str("##### ▸"); // 9바이트 = ":::toggle"
            out.push_str(rest);
        } else if line.trim_end() == ":::" {
            for _ in 0..line.len() {
                out.push(' ');
            }
        } else if !line.contains("==") && !line.contains("[[") {
            out.push_str(line);
        } else {
            out.push_str(&preprocess_inline(line));
        }
    }
    debug_assert_eq!(out.len(), s.len());
    std::borrow::Cow::Owned(out)
}

fn preprocess_inline(line: &str) -> String {
    let b = line.as_bytes();
    let mut out: Vec<u8> = b.to_vec();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'`' {
            // 코드 스팬은 그대로 건너뛴다
            let run = b[i..].iter().take_while(|&&c| c == b'`').count();
            let fence = &line[i..i + run];
            i += run;
            if let Some(off) = line[i..].find(fence) {
                i += off + run;
            }
            continue;
        }
        let pair = |open: &[u8], close: &str, i: usize| -> Option<usize> {
            if !b[i..].starts_with(open) {
                return None;
            }
            let inner = &line[i + 2..];
            let end = inner.find(close)?;
            (end > 0 && !inner[..end].contains('`')).then_some(i + 2 + end)
        };
        if let Some(end) = pair(b"==", "==", i) {
            out[i..i + 2].copy_from_slice(b"**");
            out[end..end + 2].copy_from_slice(b"**");
            i = end + 2;
        } else if let Some(end) = pair(b"[[", "]]", i).filter(|&e| !line[i + 2..e].contains('[')) {
            out[i..i + 2].copy_from_slice(b"``");
            out[end..end + 2].copy_from_slice(b"``");
            i = end + 2;
        } else {
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| line.to_string())
}

// 미리보기에서 바뀐 부분(체크박스 [ ]↔[x])을 원문에 반영한다
fn apply_preview_change(text: &mut String, before: &str, after: &str) -> bool {
    let (b, a) = (before.as_bytes(), after.as_bytes());
    if b.len() != a.len() || b.len() != text.len() {
        return false;
    }
    let Some(first) = b.iter().zip(a).position(|(x, y)| x != y) else {
        return false;
    };
    let last = b.len()
        - 1
        - b.iter()
            .rev()
            .zip(a.iter().rev())
            .position(|(x, y)| x != y)
            .unwrap_or(0);
    let r = first..last + 1;
    if !after.is_char_boundary(r.start) || !after.is_char_boundary(r.end) {
        return false;
    }
    text.replace_range(r.clone(), &after[r]);
    true
}

// 미리보기와 같은 파서(pulldown-cmark)·같은 전처리를 써서 화면과 내보낸 결과가 일치한다
fn md_to_html(md: &str) -> String {
    use pulldown_cmark::{html, Options, Parser};
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_DEFINITION_LIST
        | Options::ENABLE_MATH
        | Options::ENABLE_GFM;
    let src = preprocess(md);
    let mut body = String::new();
    html::push_html(&mut body, Parser::new_ext(&src, opts));
    body
}

fn export_html(title: &str, md: &str) -> String {
    let body = md_to_html(md);
    let title = title
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!(
        r#"<!DOCTYPE html>
<html lang="ko">
<head>
<meta charset="UTF-8">
<title>{title} — MDEditor</title>
<style>
  body {{ font-family: -apple-system, "Segoe UI", "Malgun Gothic", Roboto, sans-serif; line-height: 1.8; max-width: 820px; margin: 48px auto; padding: 0 24px; color: #0f172a; }}
  h1, h2, h3 {{ line-height: 1.35; }}
  img {{ max-width: 100%; }}
  table {{ border-collapse: collapse; width: 100%; margin: 20px 0; }}
  th, td {{ border: 1px solid #e2e8f0; padding: 10px 14px; text-align: left; }}
  th {{ background: #f8fafc; font-weight: 600; }}
  pre {{ background: #f1f5f9; padding: 16px 20px; border-radius: 8px; overflow-x: auto; }}
  code {{ font-family: Consolas, monospace; }}
  blockquote {{ border-left: 3px solid #94a3b8; margin: 16px 0; padding: 6px 16px; background: #f8fafc; }}
  .markdown-alert-note, .markdown-alert-tip {{ border-color: #0284c7; background: #f0f9ff; }}
  .markdown-alert-important {{ border-color: #7c3aed; background: #f5f3ff; }}
  .markdown-alert-warning, .markdown-alert-caution {{ border-color: #d97706; background: #fffbeb; }}
  ul:has(> li > input[type=checkbox]) {{ list-style: none; padding-left: 8px; }}
</style>
</head>
<body>
{body}
</body>
</html>
"#
    )
}

// ---------- 캔버스 블록 ----------

fn heading_level(t: &str) -> Option<usize> {
    let n = t.chars().take_while(|&c| c == '#').count();
    ((1..=6).contains(&n) && t[n..].chars().next().is_none_or(|c| c == ' ')).then_some(n)
}

fn fence_opener_line(line: &str) -> Option<(&str, &str, &str)> {
    let body = line.trim_end_matches(['\n', '\r']);
    let indent_len = body.len() - body.trim_start().len();
    let indent = &body[..indent_len];
    let t = &body[indent_len..];
    let fence = if t.starts_with("```") {
        "```"
    } else if t.starts_with("~~~") {
        "~~~"
    } else {
        return None;
    };
    let rest = &t[3..];
    if rest.contains('`') || rest.contains('~') {
        return None;
    }
    let lang = rest.trim();
    (!lang.contains(char::is_whitespace)).then_some((indent, fence, lang))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TableAlign {
    None,
    Left,
    Center,
    Right,
}

#[derive(Clone, PartialEq, Eq, Debug)]
struct MdTable {
    headers: Vec<String>,
    aligns: Vec<TableAlign>,
    rows: Vec<Vec<String>>,
}

fn split_table_row(line: &str) -> Option<Vec<String>> {
    let t = line.trim();
    if !t.contains('|') {
        return None;
    }
    let inner = t.strip_prefix('|').unwrap_or(t);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                if next == '|' || next == '\\' {
                    cur.push(next);
                    chars.next();
                    continue;
                }
            }
        }
        if c == '|' {
            cells.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    cells.push(cur.trim().to_string());
    (!cells.is_empty()).then_some(cells)
}

fn parse_table_sep(cell: &str) -> Option<TableAlign> {
    let t = cell.trim();
    let left = t.starts_with(':');
    let right = t.ends_with(':');
    let mid = &t[usize::from(left)..t.len().saturating_sub(usize::from(right))];
    if mid.is_empty() || !mid.bytes().all(|b| b == b'-') {
        return None;
    }
    Some(match (left, right) {
        (true, true) => TableAlign::Center,
        (true, false) => TableAlign::Left,
        (false, true) => TableAlign::Right,
        (false, false) => TableAlign::None,
    })
}

fn parse_md_table(src: &str) -> Option<MdTable> {
    let mut lines = src.trim_end_matches(['\n', '\r']).lines();
    let h_line = lines.next()?;
    let s_line = lines.next()?;
    let mut headers = split_table_row(h_line)?;
    let seps_raw = split_table_row(s_line)?;
    let mut aligns = Vec::with_capacity(seps_raw.len());
    for s in seps_raw {
        aligns.push(parse_table_sep(&s)?);
    }
    let cols = headers.len().max(aligns.len());
    if cols == 0 {
        return None;
    }
    headers.resize(cols, String::new());
    aligns.resize(cols, TableAlign::None);
    let mut rows = Vec::new();
    for line in lines {
        let mut row = split_table_row(line)?;
        row.resize(cols, String::new());
        rows.push(row);
    }
    Some(MdTable {
        headers,
        aligns,
        rows,
    })
}

fn serialize_md_table(tbl: &MdTable) -> String {
    let cols = tbl.headers.len().max(1);
    let escape_cell = |s: &str| s.replace('\r', "").replace('\n', " ").replace('|', "\\|");
    let mut out = String::new();
    let push_row = |out: &mut String, cells: &[String]| {
        out.push('|');
        for i in 0..cols {
            out.push(' ');
            if let Some(c) = cells.get(i) {
                out.push_str(&escape_cell(c));
            }
            out.push_str(" |");
        }
    };
    push_row(&mut out, &tbl.headers);
    out.push('\n');
    out.push('|');
    for i in 0..cols {
        let sep = match tbl.aligns.get(i).copied().unwrap_or(TableAlign::None) {
            TableAlign::None => "---",
            TableAlign::Left => ":---",
            TableAlign::Center => ":---:",
            TableAlign::Right => "---:",
        };
        out.push(' ');
        out.push_str(sep);
        out.push_str(" |");
    }
    for row in &tbl.rows {
        out.push('\n');
        push_row(&mut out, row);
    }
    out
}

// 노션식 캔버스의 블록: 빈 줄로 나뉜 바이트 구간.
// 코드 펜스·$$·:::toggle 안의 빈 줄로는 나누지 않고, 제목 줄은 항상 혼자 한 블록이다.
fn split_blocks_into(s: &str, out: &mut Vec<Range<usize>>) {
    out.clear();
    let mut start: Option<usize> = None;
    let mut fence: Option<&str> = None; // 닫는 표식
    let (mut pos, mut last_end) = (0, 0);
    for line in s.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        let t = body.trim_start();
        let end = pos + body.len();
        if let Some(close) = fence {
            let closes = if close == "```" || close == "~~~" {
                t.starts_with(close)
            } else {
                t.trim_end() == close
            };
            last_end = end;
            if closes {
                fence = None;
                out.push(start.take().unwrap_or(pos)..end);
            }
        } else if body.is_empty() {
            if let Some(st) = start.take() {
                out.push(st..last_end);
            }
        } else {
            let opener = if t.starts_with("```") {
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
            let heading = heading_level(t).is_some();
            if opener.is_some() || heading {
                if let Some(st) = start.take() {
                    out.push(st..last_end);
                }
            }
            if opener.is_some() {
                start = Some(pos);
                fence = opener;
            } else if heading {
                out.push(pos..end);
            } else if start.is_none() {
                start = Some(pos);
            }
            last_end = end;
        }
        pos += line.len();
    }
    if let Some(st) = start {
        out.push(st..last_end);
    }
}

fn split_blocks(s: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    split_blocks_into(s, &mut out);
    out
}

// 캔버스/커서 이동용 슬롯 목록: 비어 있지 않은 블록 + 빈 줄 블록(start..start)
fn collect_doc_slots_into(
    text: &str,
    blocks: &[Range<usize>],
    active_empty: Option<usize>,
    out: &mut Vec<Range<usize>>,
) {
    fn push_gap(
        out: &mut Vec<Range<usize>>,
        active_empty: Option<usize>,
        start: usize,
        end: usize,
        step_from: usize,
        skip_end: bool,
    ) {
        let mut p = step_from;
        while p < end || (!skip_end && p == end) {
            out.push(p..p);
            p += 2;
        }
        if let Some(a) = active_empty {
            if a >= start && a <= end && (!skip_end || a < end) && !out.contains(&(a..a)) {
                let at = out.partition_point(|r| r.start < a);
                out.insert(at, a..a);
            }
        }
    }
    out.clear();
    if blocks.is_empty() {
        if text.is_empty() {
            if active_empty == Some(0) {
                out.push(0..0);
            }
        } else {
            push_gap(out, active_empty, 0, text.len(), 0, false);
        }
        return;
    }
    push_gap(out, active_empty, 0, blocks[0].start, 0, true);
    for (i, b) in blocks.iter().enumerate() {
        out.push(b.clone());
        let next_start = blocks.get(i + 1).map_or(text.len(), |n| n.start);
        let step_from = (b.end + 2).min(next_start + 1);
        push_gap(
            out,
            active_empty,
            b.end,
            next_start,
            step_from,
            i + 1 < blocks.len(),
        );
    }
}

#[cfg(test)]
fn collect_doc_slots(
    text: &str,
    blocks: &[Range<usize>],
    active_empty: Option<usize>,
) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    collect_doc_slots_into(text, blocks, active_empty, &mut out);
    out
}

fn slot_index(slots: &[Range<usize>], target: &Range<usize>) -> Option<usize> {
    slots
        .binary_search_by(|s| s.start.cmp(&target.start).then(s.end.cmp(&target.end)))
        .ok()
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
    Image {
        url_tab: bool,
        url: String,
        alt: String,
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
    focus: bool,
    cache_key: (u64, String, bool),
    cached: Vec<Sel>,
}

impl Find {
    fn matches(&mut self, doc: &Doc) -> &[Sel] {
        if self.cache_key.0 != doc.rev
            || self.cache_key.1 != self.query
            || self.cache_key.2 != self.case
        {
            self.cached = find_all(&doc.text, &self.query, self.case);
            self.cache_key = (doc.rev, self.query.clone(), self.case);
        }
        &self.cached
    }
}

#[derive(Clone, Copy)]
enum Fmt {
    Wrap(&'static str, &'static str, &'static str),
    Clear,
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
    last_sel: Option<Sel>,
    pending_sel: Option<Sel>,
    pending_byte_caret: Option<(usize, usize)>,
    pending_click_pos: Option<(usize, egui::Pos2)>,
    focus_editor: bool,
    ime_composing: bool,
    pending_enter: bool,
    raw_table_mode: bool,
    table_focus_cell: Option<(usize, usize)>,
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
    canvas_slots: Vec<Range<usize>>,
    canvas_items: Vec<(Range<usize>, bool, usize)>,
    canvas_rects: Vec<(Range<usize>, egui::Rect)>,
    edit_buf: String,
    editor_galley_cache: Option<(u64, u32, u32, bool, std::sync::Arc<egui::Galley>)>,
    code_galley_cache: CodeGalleyCache,
    inline_galley_cache: InlineGalleyCache,
    table_cache: TableParseCache,
    line_num_buf: String,
    cached_scheme: (PathBuf, String),
    cached_root_label: (PathBuf, String),
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
        let draft = get("draft").unwrap_or_default();
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
            last_sel: None,
            pending_sel: None,
            pending_byte_caret: None,
            pending_click_pos: None,
            focus_editor: false,
            ime_composing: false,
            pending_enter: false,
            raw_table_mode: false,
            table_focus_cell: None,
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
            canvas_slots: Vec::new(),
            canvas_items: Vec::new(),
            canvas_rects: Vec::new(),
            edit_buf: String::new(),
            editor_galley_cache: None,
            code_galley_cache: HashMap::new(),
            inline_galley_cache: HashMap::new(),
            table_cache: HashMap::new(),
            line_num_buf: String::new(),
            cached_scheme: (PathBuf::new(), String::new()),
            cached_root_label: (PathBuf::new(), String::new()),
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
            doc.view_active = self.active;
            doc.view_blank = self.blank;
            doc.view_block_sel = self.block_sel.clone();
            doc.view_block_sel_anchor = self.block_sel_anchor;
            doc.view_block_sel_head = self.block_sel_head;
            doc.view_last_sel = self.last_sel;
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
        self.editor_galley_cache = None;
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
                self.active = doc.view_active.filter(|&a| a <= doc.text.len());
                self.blank = doc.view_blank;
                self.block_sel = doc.view_block_sel.clone();
                self.block_sel_anchor = doc.view_block_sel_anchor;
                self.block_sel_head = doc.view_block_sel_head;
                self.last_sel = doc.view_last_sel;
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
        if !is_scratch_key(name) && self.docs.get(name).is_some_and(Doc::dirty) {
            self.save_quiet(name);
            if self.docs.get(name).is_some_and(Doc::dirty) {
                self.dialog = Some(Dialog::Unsaved(After::CloseTab(name.to_string())));
                return;
            }
        }
        self.force_close_tab(name, idx);
    }

    fn force_close_tab(&mut self, name: &str, idx: usize) {
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
        self.deactivate();
        self.root = root;
        self.docs.retain(|k, _| is_scratch_key(k));
        self.tabs.retain(|k| is_scratch_key(k));
        self.current = self.tabs.first().cloned();
        self.files.clear();
        self.block_heights.clear();
        self.status = format!("작업 폴더: {}", self.root.display());
        // 열 파일이 정해져 있으면 폴더 스캔을 미룬다 (파일 탐색기를 열 때 스캔) → 큰 폴더에서도 바로 뜬다
        let mut first = open.filter(|n| self.root.join(n).is_file());
        if first.is_none() {
            self.refresh_files();
            first = self.files.first().cloned();
        }
        if let Some(name) = first {
            self.open_file(&name);
        } else if let Some(cur) = self.current.clone() {
            self.restore_tab_view(&cur);
        }
    }

    fn refresh_files(&mut self) {
        let mut names = Vec::new();
        collect_markdown_files(&self.root, &self.root, 0, &mut names);
        names.sort_by_key(|n| n.to_lowercase());
        self.files = names;
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

    // 자동 저장·종료 시 저장: 성공은 조용히(상태 알약이 알려 준다), 실패만 알린다
    fn save_quiet(&mut self, name: &str) -> bool {
        if is_scratch_key(name) {
            return false;
        }
        let Some(doc) = self.docs.get_mut(name) else {
            return false;
        };
        if doc.scratch {
            return false;
        }
        let path = self.root.join(name);
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

    // 창으로 돌아왔을 때: 다른 프로그램이 현재 문서를 바꿨으면 다시 읽는다 (편집 중이면 경고만)
    fn sync_from_disk(&mut self) {
        let Some(name) = self.current.clone().filter(|n| !is_scratch_key(n)) else {
            return;
        };
        let path = self.root.join(&name);
        let (Some(disk), Some(doc)) = (modified(&path), self.docs.get_mut(&name)) else {
            return;
        };
        if doc.mtime == Some(disk) {
            return;
        }
        if doc.dirty() {
            doc.mtime = Some(disk);
            self.status = format!("{name}: 다른 프로그램이 파일을 바꿨습니다. 저장하면 덮어씁니다");
        } else if let Ok(fresh) = Doc::load(&path) {
            if fresh.text != doc.text {
                self.deactivate();
                self.status = format!("{name}: 바뀐 내용을 다시 불러왔습니다");
            }
            self.docs.insert(name, fresh);
        }
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
        let dirty: Vec<String> = self
            .docs
            .iter()
            .filter(|(_, d)| !d.scratch && d.dirty())
            .map(|(n, _)| n.clone())
            .collect();
        dirty.iter().filter(|n| !self.save_quiet(n)).count() == 0 // 실패해도 나머지는 계속 저장
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
        let to = validate_name(input)?;
        if to == from {
            return Ok(());
        }
        let dst = self.root.join(&to);
        if dst.exists() {
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
            self.dialog = Some(Dialog::Unsaved(after));
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
            let s = bs.start.min(doc.text.len());
            let e = bs.end.min(doc.text.len());
            return if e > s { char_len(&doc.text[s..e]) } else { 0 };
        }
        if self.active.is_some() {
            if let Some((a, b)) = self.last_sel {
                return b.saturating_sub(a);
            }
        }
        0
    }

    fn apply_font(&mut self, ctx: &egui::Context, font: String) {
        if self.font_family == font {
            return;
        }
        theme::install_fonts_with(ctx, &font);
        self.font_family = font;
        self.editor_galley_cache = None;
        self.code_galley_cache.clear();
        self.inline_galley_cache.clear();
        self.block_heights.clear();
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
            let s = bs.start.min(doc.text.len());
            let e = bs.end.min(doc.text.len());
            return (char_len(&doc.text[..s]), char_len(&doc.text[..e]));
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
        let start = sel.start.min(len);
        let end = sel.end.min(len);
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
        let start = sel.start.min(doc.text.len());
        let end = sel.end.min(doc.text.len());
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
                Some((b0..b1, anchor_block, head_block))
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

    fn format(&mut self, ctx: &egui::Context, f: Fmt) {
        if self.active_block().is_none() && self.block_sel.is_none() {
            if matches!(f, Fmt::Clear) {
                return;
            }
            self.append_block();
        }
        match f {
            Fmt::Wrap(pre, suf, ph) => {
                self.edit(ctx, |t, sel| Some(wrap_selection(t, sel, pre, suf, ph)))
            }
            Fmt::Clear => self.edit(ctx, |t, (a, b)| {
                if a == b {
                    return None;
                }
                let mut s = t[byte_idx(t, a)..byte_idx(t, b)].to_string();
                for m in ["**", "~~", "==", "[[", "]]", "`", "*"] {
                    s = s.replace(m, "");
                }
                Some((replace_range(t, (a, b), &s), (a, a + char_len(&s))))
            }),
        }
    }

    // 블록 유형 전환 (단축키 Ctrl+Alt+0~9 / Ctrl+Alt+G 또는 우클릭 컨텍스트 메뉴)
    fn convert_block(&mut self, ctx: &egui::Context, kind: BlockKind) {
        if self.current.is_none() {
            return;
        }
        if self.active_block().is_none() && self.block_sel.is_none() {
            self.append_block();
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
        self.edit(ctx, move |t, sel| Some(convert_lines_kind(t, sel, kind)));
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
            let s = bs.start.min(doc.text.len());
            let e = bs.end.min(doc.text.len());
            if s < e {
                ctx.copy_text(doc.text[s..e].to_string());
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
            self.insert(ctx, &s);
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
        let b = self.block_byte_of(&r, doc, sel.0);
        let ls = text[..b].rfind('\n').map_or(0, |i| i + 1).max(r.start);
        let le = text[b..].find('\n').map_or(text.len(), |i| b + i);
        if let Some((.., empty)) = list_marker(&text[ls..le]) {
            if !empty {
                if let Some((t2, c)) = smart_enter(&text, sel.0) {
                    self.set_text(t2, (c, c));
                    return true;
                }
                return false;
            }
            let mut t2 = text;
            if ls == r.start {
                // 첫 줄이 빈 항목이면 기호만 지우고 일반 문단으로
                t2.replace_range(ls..le, "");
                let c = char_len(&t2[..ls]);
                self.set_text(t2, (c, c));
                return true;
            }
            t2.replace_range(ls - 1..le, "");
            self.set_text(t2, sel);
            self.split(ls - 1);
            return true;
        }
        self.split(b);
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
        let pt = doc.text[prev.clone()].trim_start();
        if !r.is_empty()
            && ["```", "~~~", "$$", ":::toggle"]
                .iter()
                .any(|p| pt.starts_with(p))
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
        let nt = doc.text[next.clone()].trim_start();
        if !r.is_empty()
            && ["```", "~~~", "$$", ":::toggle"]
                .iter()
                .any(|p| nt.starts_with(p))
        {
            return false;
        }
        let mut t2 = doc.text.clone();
        t2.replace_range(r.end..next.start, "");
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
        let slots = self.doc_slots();
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
        if !right {
            if local.0 > 0 {
                return false;
            }
        } else if local.0 < char_len(&doc.text[r.clone()]) {
            return false;
        }
        let slots = self.doc_slots();
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
            let slots = self.doc_slots();
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
        let slots = self.doc_slots();
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
        let start = r.start.min(adj.start);
        let end = r.end.max(adj.end);
        self.leave_blank();
        self.active = None;
        self.block_sel = Some(start..end);
        self.block_sel_anchor = Some(r.start);
        self.block_sel_head = Some(adj.start);
        self.scroll_to = Some(adj.start);
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
        let link = match src.strip_prefix(&dir) {
            Ok(rel) if !to_slash(rel).contains(' ') => to_slash(rel),
            _ => {
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
            .map_or("image".into(), |s| s.to_string_lossy().into_owned());
        self.insert(ctx, &format!("![{alt}]({link})"));
        self.status = format!("이미지 삽입: {link}");
    }

    // 클립보드 이미지(스크린샷) → 문서 폴더/images/paste-<시각>.png
    fn insert_pasted_image(&mut self, ctx: &egui::Context, img: &egui::ColorImage) {
        if self.current.is_none() {
            self.status = "이미지를 넣을 문서를 먼저 여세요".into();
            return;
        }
        let rgba: Vec<u8> = img
            .pixels
            .iter()
            .flat_map(|c| c.to_srgba_unmultiplied())
            .collect();
        let Some(buf) = image::RgbaImage::from_raw(img.size[0] as u32, img.size[1] as u32, rgba)
        else {
            return;
        };
        let dir = self.doc_dir().join("images");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let dst = unique_path(&dir, &format!("paste-{stamp}.png"));
        let saved = std::fs::create_dir_all(&dir)
            .map_err(|e| e.to_string())
            .and_then(|_| buf.save(&dst).map_err(|e| e.to_string()));
        match saved {
            Ok(()) => {
                let name = dst
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                self.insert(ctx, &format!("![image](images/{name})"));
                self.status = format!("스크린샷 붙여넣기: images/{name}");
            }
            Err(e) => self.status = format!("이미지 저장 실패: {e}"),
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
                egui::OutputCommand::OpenUrl(u) if !u.url.contains("://") => {
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
            let path = url.split('#').next().unwrap_or("").replace("%20", " ");
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
        let now = Instant::now();
        let mut due = Vec::new();
        let mut flush_draft = false;
        for (name, doc) in &self.docs {
            if let (true, Some(t)) = (doc.dirty(), doc.edited_at) {
                let elapsed = now.duration_since(t);
                if elapsed >= AUTO_SAVE_DELAY {
                    if doc.scratch {
                        flush_draft = true;
                    } else {
                        due.push(name.clone());
                    }
                } else {
                    ctx.request_repaint_after(AUTO_SAVE_DELAY - elapsed);
                }
            }
        }
        if flush_draft {
            let draft_body = self.draft_text().to_string();
            if let Some(st) = storage {
                st.set_string("draft", draft_body);
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
            self.save_quiet(&name);
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.dialog.is_some() {
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
                            Key::F1 | Key::F2 | Key::F3 | Key::F11 => fn_key = true,
                            _ => {}
                        }
                    }
                    egui::Event::Ime(ime) => {
                        has_ime_or_text = true;
                        match ime {
                            egui::ImeEvent::Preedit { text, .. } => {
                                ime_still_composing = !text.is_empty();
                            }
                            egui::ImeEvent::Commit(_) => {
                                ime_still_composing = false;
                            }
                            _ => {}
                        }
                    }
                    egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_)
                        if has_block_sel =>
                    {
                        block_ev = true;
                    }
                    egui::Event::Text(s) if !s.is_empty() && s != "\n" && s != "\r" => {
                        has_ime_or_text = true;
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
        let shift_cmd = Modifiers::COMMAND | Modifiers::SHIFT;
        let alt_cmd = Modifiers::COMMAND | Modifiers::ALT;
        let k = |m, key| ctx.input_mut(|i| i.consume_key(m, key));
        let focused = self.editor_focused(ctx);

        if has_cmd {
            // Shift / Alt 조합을 먼저 본다 (consume_key는 추가 Shift를 무시하므로)
            if k(shift_cmd, Key::O) {
                self.context_menu = None;
                self.pick_folder();
                return;
            } else if has_alt && k(alt_cmd, Key::O) {
                self.context_menu = None;
                self.open_local_file();
                return;
            } else if k(cmd, Key::O) {
                self.context_menu = None;
                self.open_files();
                return;
            }
            if k(shift_cmd, Key::I) {
                self.context_menu = None;
                self.dialog = Some(Dialog::Image {
                    url_tab: false,
                    url: String::new(),
                    alt: String::new(),
                });
                return;
            }
            if k(shift_cmd, Key::N) {
                self.context_menu = None;
                self.dialog = Some(Dialog::NewFile(String::new()));
                return;
            }
            if k(cmd, Key::N) || k(cmd, Key::T) {
                self.context_menu = None;
                self.new_tab();
                return;
            }
            if k(shift_cmd, Key::E) {
                self.context_menu = None;
                self.export();
                return;
            }
            if k(shift_cmd, Key::L) {
                self.context_menu = None;
                self.toc_open = !self.toc_open;
                return;
            }
            if k(shift_cmd, Key::F) {
                self.context_menu = None;
                self.focus_mode = !self.focus_mode;
                return;
            }
            if k(shift_cmd, Key::C) {
                self.context_menu = None;
                self.copy_markdown_source(ctx);
                return;
            }
            if has_alt {
                if k(alt_cmd, Key::C) {
                    self.context_menu = None;
                    self.copy_html_code(ctx);
                    return;
                }
                if k(alt_cmd, Key::D) {
                    self.context_menu = None;
                    self.dark = !self.dark;
                    theme::apply(ctx, self.dark);
                    return;
                }
                if k(alt_cmd, Key::W) {
                    self.context_menu = None;
                    self.full_width = !self.full_width;
                    return;
                }
                if k(alt_cmd, Key::F) {
                    self.context_menu = None;
                    self.small_text = !self.small_text;
                    return;
                }
                // 블록 전환 및 삽입 단축키 (Ctrl+Alt+0~9, G, T, M, -)
                if !self.find.open && self.current.is_some() {
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
                            return;
                        }
                    }
                    if k(alt_cmd, Key::T) {
                        self.context_menu = None;
                        self.insert_block_template("| 제목 | 내용 |\n| --- | --- |\n|  |  |", 2);
                        return;
                    }
                    if k(alt_cmd, Key::M) {
                        self.context_menu = None;
                        self.insert_block_template("```mermaid\ngraph TD\n  A --> B\n```", 11);
                        return;
                    }
                    if k(alt_cmd, Key::Minus) {
                        self.context_menu = None;
                        self.insert_block_template("---", 3);
                        return;
                    }
                }
            }
            if k(cmd, Key::W) {
                self.context_menu = None;
                if let Some(cur) = self.current.clone() {
                    self.close_tab(&cur);
                }
                return;
            }
            if k(shift_cmd, Key::Tab) || k(cmd, Key::PageUp) {
                self.context_menu = None;
                self.cycle_tab(-1);
                return;
            } else if k(cmd, Key::Tab) || k(cmd, Key::PageDown) {
                self.context_menu = None;
                self.cycle_tab(1);
                return;
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
                    return;
                }
            }
            // 서식 단축키 중 Shift 조합을 먼저 처리 (Ctrl+Shift+S 취소선, Ctrl+Shift+H 형광펜, Ctrl+Shift+K 키캡)
            let can_edit_blocks = !self.find.open && (focused || self.block_sel.is_some());
            if can_edit_blocks {
                if k(shift_cmd, Key::S) {
                    self.context_menu = None;
                    self.format(ctx, Fmt::Wrap("~~", "~~", "취소선"));
                    return;
                }
                if k(shift_cmd, Key::H) {
                    self.context_menu = None;
                    self.format(ctx, Fmt::Wrap("==", "==", "강조"));
                    return;
                }
                if k(shift_cmd, Key::K) {
                    self.context_menu = None;
                    self.format(ctx, Fmt::Wrap("[[", "]]", "Ctrl"));
                    return;
                }
                if k(shift_cmd, Key::D) {
                    self.context_menu = None;
                    self.delete_current_blocks();
                    return;
                }
                if k(shift_cmd, Key::Enter) {
                    self.context_menu = None;
                    self.insert_block_below();
                    return;
                }
                if k(cmd, Key::Enter) {
                    self.context_menu = None;
                    self.toggle_todo(ctx);
                    return;
                }
                if k(cmd, Key::E) {
                    self.context_menu = None;
                    self.format(ctx, Fmt::Wrap("`", "`", "code"));
                    return;
                }
                if k(cmd, Key::Backslash) {
                    self.context_menu = None;
                    self.format(ctx, Fmt::Clear);
                    return;
                }
            }
            if k(cmd, Key::S) {
                self.context_menu = None;
                self.trigger_save();
                return;
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
                    return;
                }
                if k(cmd, Key::Z) {
                    self.context_menu = None;
                    self.history(true);
                    return;
                }
            }
            // Ctrl+Home / Ctrl+End: 문서 맨 처음 / 맨 끝으로 이동
            if !self.find.open && self.current.is_some() {
                if k(cmd, Key::Home) {
                    self.jump_doc_edge(false);
                    return;
                }
                if k(cmd, Key::End) {
                    self.jump_doc_edge(true);
                    return;
                }
            }
            // Ctrl+A: 비활성 상태이거나 현재 블록이 이미 전부 선택된 상태면 문서 전체 선택
            if !self.find.open && ctx.input(|i| i.key_pressed(Key::A) && i.modifiers.command_only())
            {
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
                    return;
                }
            }
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
        if let Some(bs) = self.block_sel.clone().filter(|_| !self.find.open) {
            if nav_or_edit {
                if k(Modifiers::SHIFT, Key::ArrowUp) && self.shift_arrow(ctx, true, false) {
                    return;
                }
                if k(Modifiers::SHIFT, Key::ArrowDown) && self.shift_arrow(ctx, false, false) {
                    return;
                }
                if k(Modifiers::SHIFT, Key::ArrowLeft) && self.shift_arrow(ctx, true, true) {
                    return;
                }
                if k(Modifiers::SHIFT, Key::ArrowRight) && self.shift_arrow(ctx, false, true) {
                    return;
                }
                if has_alt && k(Modifiers::ALT, Key::ArrowUp) {
                    self.move_blocks(ctx, true);
                    return;
                }
                if has_alt && k(Modifiers::ALT, Key::ArrowDown) {
                    self.move_blocks(ctx, false);
                    return;
                }
            }
            if has_cmd && k(cmd, Key::D) {
                self.duplicate_blocks();
                return;
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
                return;
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
                return;
            }
            if bs_del_tab {
                if k(Modifiers::SHIFT, Key::Tab) {
                    self.edit(ctx, |t, sel| Some(indent_lines(t, sel, true)));
                    return;
                } else if k(Modifiers::NONE, Key::Tab) {
                    self.edit(ctx, |t, sel| Some(indent_lines(t, sel, false)));
                    return;
                }
                if k(Modifiers::NONE, Key::Backspace) || k(Modifiers::NONE, Key::Delete) {
                    self.delete_block_sel();
                    return;
                }
            }
            if nav_or_edit && k(Modifiers::NONE, Key::Enter) {
                self.delete_block_sel();
                return;
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
                        egui::Event::Text(s) if !i.modifiers.command && !i.modifiers.ctrl => {
                            replace_with = Some(s.clone());
                            false
                        }
                        _ => true,
                    });
                });
                if do_copy || do_cut {
                    if let (Some(bs), Some(doc)) = (&self.block_sel, self.doc()) {
                        let s = bs.start.min(doc.text.len());
                        let e = bs.end.min(doc.text.len());
                        if s < e {
                            ctx.copy_text(doc.text[s..e].to_string());
                        }
                    }
                    if do_cut {
                        self.delete_block_sel();
                        return;
                    }
                }
                if let Some(s) = replace_with {
                    self.replace_block_sel(&s);
                    return;
                }
            }
            if has_cmd {
                if k(cmd, Key::B) {
                    self.format(ctx, Fmt::Wrap("**", "**", "굵게"));
                    return;
                }
                if k(cmd, Key::I) {
                    self.format(ctx, Fmt::Wrap("*", "*", "기울임"));
                    return;
                }
                if k(cmd, Key::K) {
                    self.format(ctx, Fmt::Wrap("[", "](https://)", "링크"));
                    return;
                }
            }
        }

        if !focused {
            if !nav_or_edit && !bs_del_tab {
                return;
            }
            // 활성 블록에 선택 영역이 있는데 포커스만 잠시 빠진 경우 Backspace/Delete 처리
            if !self.find.open && self.active.is_some() && bs_del_tab {
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
        if self.is_visual_table_active() || ime_still_composing {
            if nav_or_edit
                && (has_ime_or_text || self.ime_composing)
                && !self.is_visual_table_active()
            {
                let enter = ctx.input(|i| i.modifiers.is_none() && i.key_pressed(Key::Enter));
                if enter {
                    k(Modifiers::NONE, Key::Enter);
                    self.pending_enter = true;
                }
            }
            return;
        }
        if bs_del_tab {
            if k(Modifiers::SHIFT, Key::Tab) {
                self.edit(ctx, |t, sel| Some(indent_lines(t, sel, true)));
            } else if k(Modifiers::NONE, Key::Tab) {
                self.edit(ctx, |t, sel| {
                    let multi = t[byte_idx(t, sel.0)..byte_idx(t, sel.1)].contains('\n');
                    let line =
                        &t[byte_idx(t, line_start(t, sel.0))..byte_idx(t, line_end(t, sel.0))];
                    if multi || list_marker(line).is_some() {
                        Some(indent_lines(t, sel, false))
                    } else {
                        Some((replace_range(t, sel, "  "), (sel.0 + 2, sel.0 + 2)))
                    }
                });
            }
            let (bs, del) = ctx.input(|i| {
                (
                    i.modifiers.is_none() && i.key_pressed(Key::Backspace),
                    i.modifiers.is_none() && i.key_pressed(Key::Delete),
                )
            });
            if bs && self.backspace_at_start(ctx) {
                k(Modifiers::NONE, Key::Backspace);
            }
            if del && self.delete_at_end(ctx) {
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
        let (q, w, case) = (
            self.find.query.clone(),
            self.find.with.clone(),
            self.find.case,
        );
        self.edit(ctx, |t, sel| {
            let hit = find_all(t, &q, case).into_iter().any(|m| m == sel);
            hit.then(|| {
                let at = sel.0 + char_len(&w);
                (replace_range(t, sel, &w), (at, at))
            })
        });
        self.focus_editor = false;
        self.find_step(ctx, true);
    }

    fn replace_everything(&mut self, ctx: &egui::Context) {
        let (q, w, case) = (
            self.find.query.clone(),
            self.find.with.clone(),
            self.find.case,
        );
        let mut count = 0;
        self.edit(ctx, |t, _| {
            let m = find_all(t, &q, case);
            count = m.len();
            (!m.is_empty()).then(|| (replace_all(t, &m, &w), (0, 0)))
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
            to_slash(&self.doc_dir()).trim_start_matches('/')
        );
        let html = export_html(&stem, &doc.text)
            .replacen("<head>", &format!("<head>\n<base href=\"{base}\">"), 1)
            .replacen(
                "</body>",
                "<script>window.onload = () => window.print();</script>\n</body>",
                1,
            );
        let path = std::env::temp_dir().join("mdeditor-print.html");
        match save_atomic(&path, html.as_bytes()) {
            Ok(()) => ctx.open_url(egui::OpenUrl::new_tab(format!(
                "file:///{}",
                to_slash(&path).trim_start_matches('/')
            ))),
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

        // 7. 블록 맨 앞 Backspace는 앞 블록과 합친다
        h.get_by_label("소제목").click();
        h.run_steps(3);
        h.key_press(egui::Key::Home);
        h.run_steps(2);
        h.key_press(egui::Key::Backspace);
        h.run_steps(3);
        assert_eq!(
            text(&h),
            "# 제목

첫 문단!## 소제목

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

## 소제목

- [x] 할 일
"
        );
        assert!(h.state().active.is_some());
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Y);
        h.run_steps(3);
        assert!(text(&h).contains("첫 문단!## 소제목"), "{}", text(&h));
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

        // 닫는 ``` 줄 끝에서 Enter를 누르면 코드 블록을 빠져나와 아래에 새 블록을 만든다
        h.key_press(egui::Key::ArrowDown);
        h.key_press(egui::Key::End);
        h.key_press(egui::Key::Enter);
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

        // 3. 원래 탭으로 돌아가 $$ 닫는 줄에서 Enter 시 블록 탈출 확인
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
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        h.event(egui::Event::Text("수식 다음 줄".into()));
        h.run_steps(2);
        assert!(h
            .state()
            .doc()
            .unwrap()
            .text
            .contains("$$\na^2 + b^2 = c^2\n$$\n\n수식 다음 줄"));

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
}
