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

mod setup;
mod theme;
mod ui;

const AUTO_SAVE_DELAY: Duration = Duration::from_secs(2);
const UNDO_LIMIT: usize = 200; // 단계마다 변경 구간(Patch)만 기록해 메모리 점유를 최소화
const MAX_FILES: usize = 5000; // 거대한 폴더(예: 사용자 홈) 스캔 상한
const SCRATCH_KEY: &str = ""; // 이름 없는 임시 메모 문서 키 (실제 파일 경로는 비어 있을 수 없다)

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let exe = std::env::current_exe().unwrap_or_default();
    if let Some(mode) = setup::mode(&exe, &args) {
        return setup::run(mode, &args);
    }
    let arg = args.iter().find(|a| !a.starts_with('-')).map(PathBuf::from);
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    // 시작 메뉴·탐색기에서 인자 없이 실행하면(작업 디렉토리가 홈 또는 실행 파일 폴더)
    // 홈 폴더 전체를 뒤지는 대신 문서\MDEditor를 기본 작업 폴더로 쓴다. 터미널에서 실행하면 그 폴더.
    let launched_bare = cwd == setup::home() || exe.parent() == Some(cwd.as_path());
    let cwd = if arg.is_none() && launched_bare {
        setup::home().join("Documents").join("MDEditor")
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
    let l = name.to_lowercase();
    l.ends_with(".md") || l.ends_with(".markdown")
}

fn is_image_name(name: &str) -> bool {
    let l = name.to_lowercase();
    [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp"]
        .iter()
        .any(|e| l.ends_with(e))
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
        };
        doc.refresh_meta();
        Ok(doc)
    }

    fn scratch(text: String) -> Doc {
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
    let mut out = s.to_string();
    for &m in matches.iter().rev() {
        out = replace_range(&out, m, with);
    }
    out
}

// 목차: (레벨, 제목, 줄 번호, 줄 시작 문자 인덱스, 줄 시작 바이트 인덱스). 코드 펜스 안은 제외.
// 일반 줄에서는 UTF-8 문자 수(char_len)를 세지 않고 제목 줄을 만났을 때만 증분 계산한다.
fn outline_with_bytes_into(s: &str, out: &mut Vec<(usize, String, usize, usize, usize)>) {
    out.clear();
    let (mut in_code, mut ci, mut bi, mut last_bi) = (false, 0, 0, 0);
    for (n, line) in s.split('\n').enumerate() {
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_code = !in_code;
        } else if !in_code {
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
        } else if t.is_empty() {
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

struct App {
    root: PathBuf,
    files: Vec<String>,
    docs: HashMap<String, Doc>,
    current: Option<String>,
    dark: bool,
    full_width: bool,
    small_text: bool,
    md_cache: CommonMarkCache,
    status: String,
    toast: (String, Option<Instant>),
    find: Find,
    dialog: Option<Dialog>,
    active: Option<usize>,           // 캔버스에서 편집 중인 블록의 시작 바이트
    blank: usize,                    // 활성 블록을 만들며 넣은 줄바꿈 수 (비운 채 떠나면 되돌린다)
    block_sel: Option<Range<usize>>, // 다중 블록 또는 전체 문서 선택 바이트 구간
    drag_anchor: Option<(usize, egui::Pos2)>, // 비활성 블록에서 시작한 드래그 기준점
    drag_last_pos: Option<egui::Pos2>,
    last_sel: Option<Sel>,
    pending_sel: Option<Sel>,
    focus_editor: bool,
    scroll_cursor: bool,
    slash_sel: usize,
    toc_open: bool,
    toc_active: Option<usize>,
    scroll_to: Option<usize>,
    allow_close: bool,
    last_title: String,
    block_heights: HashMap<egui::Id, f32>,
    height_cache_key: (u32, u32),
    active_char_cache: (usize, u64, usize, usize),
    canvas_items: Vec<(Range<usize>, bool)>,
    canvas_rects: Vec<(Range<usize>, egui::Rect)>,
    startup_frames: u8,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, arg: Option<PathBuf>, cwd: &Path) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        theme::install_fonts(&cc.egui_ctx);
        let get = |k: &str| cc.storage.and_then(|s| s.get_string(k));
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
            current: None,
            dark,
            full_width: flag("full_width"),
            small_text: flag("small_text"),
            md_cache: CommonMarkCache::default(),
            status: String::new(),
            toast: (String::new(), None),
            find: Find::default(),
            dialog: None,
            active: None,
            blank: 0,
            block_sel: None,
            drag_anchor: None,
            drag_last_pos: None,
            last_sel: None,
            pending_sel: None,
            focus_editor: false,
            scroll_cursor: false,
            slash_sel: 0,
            toc_open: false,
            toc_active: None,
            scroll_to: None,
            allow_close: false,
            last_title: String::new(),
            block_heights: HashMap::new(),
            height_cache_key: (0, 0),
            active_char_cache: (usize::MAX, u64::MAX, 0, 0),
            canvas_items: Vec::new(),
            canvas_rects: Vec::new(),
            startup_frames: 0,
        };
        if has_arg {
            app.switch_root(root, open);
            if !draft.is_empty() {
                app.docs.insert(SCRATCH_KEY.into(), Doc::scratch(draft));
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
        self.current.as_deref() == Some(SCRATCH_KEY)
    }

    fn draft_text(&self) -> &str {
        self.docs.get(SCRATCH_KEY).map_or("", |d| d.text.as_str())
    }

    fn open_scratch(&mut self, initial: String) {
        self.deactivate();
        self.docs
            .entry(SCRATCH_KEY.into())
            .or_insert_with(|| Doc::scratch(initial));
        self.current = Some(SCRATCH_KEY.into());
        if self.docs[SCRATCH_KEY].text.is_empty() {
            self.active = Some(0);
            self.pending_sel = Some((0, 0));
            self.focus_editor = true;
        }
    }

    fn new_scratch(&mut self) {
        self.deactivate();
        if let Some(doc) = self.docs.get_mut(SCRATCH_KEY) {
            if !doc.text.is_empty() {
                doc.text.clear();
                doc.edited_at = Some(Instant::now());
                doc.refresh_meta();
            }
        } else {
            self.docs
                .insert(SCRATCH_KEY.into(), Doc::scratch(String::new()));
        }
        self.current = Some(SCRATCH_KEY.into());
        self.active = Some(0);
        self.pending_sel = Some((0, 0));
        self.focus_editor = true;
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
        self.docs.retain(|k, _| k == SCRATCH_KEY);
        self.current = None;
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
        }
    }

    fn refresh_files(&mut self) {
        let mut names = Vec::new();
        collect_markdown_files(&self.root, &self.root, 0, &mut names);
        names.sort_by_key(|n| n.to_lowercase());
        self.files = names;
    }

    fn open_file(&mut self, name: &str) -> bool {
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
        if self.current.as_deref() != Some(name) {
            self.deactivate();
            self.current = Some(name.to_string());
            // 수정되지 않은 비활성 문서는 메모리에서 내려 메모리 점유를 최소화
            self.docs
                .retain(|k, d| k == SCRATCH_KEY || k == name || d.dirty());
            self.block_heights.clear();
        }
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
        if name == SCRATCH_KEY {
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
        let Some(name) = self.current.clone().filter(|n| n != SCRATCH_KEY) else {
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
        let c = char_len(&doc.text[..caret]);
        self.blank = 0;
        self.block_sel = None;
        self.active = Some(start);
        self.pending_sel = Some((c, c));
        self.focus_editor = true;
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
        let name = validate_name(input)?;
        let path = self.root.join(&name);
        if path.exists() {
            return Err(format!("이미 있는 파일입니다: {name}"));
        }
        let body = self
            .docs
            .get(SCRATCH_KEY)
            .map(|d| d.encoded())
            .unwrap_or_default();
        save_atomic(&path, body.as_bytes()).map_err(|e| format!("저장 실패: {e}"))?;
        let mut doc = self
            .docs
            .remove(SCRATCH_KEY)
            .unwrap_or_else(|| Doc::scratch(String::new()));
        doc.scratch = false;
        doc.saved = doc.text.clone();
        doc.edited_at = None;
        doc.dirty_flag = false;
        doc.mtime = modified(&path);
        self.docs.insert(name.clone(), doc);
        self.docs
            .insert(SCRATCH_KEY.into(), Doc::scratch(String::new()));
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
        if self.current.as_deref() == Some(from) {
            self.current = Some(to.clone());
        }
        self.refresh_files();
        self.status = format!("이름 변경: {from} → {to}");
        Ok(())
    }

    fn delete(&mut self, name: &str) {
        if name == SCRATCH_KEY {
            return;
        }
        match std::fs::remove_file(self.root.join(name)) {
            Ok(()) => {
                if self.current.as_deref() == Some(name) {
                    self.active = None;
                    self.blank = 0;
                    self.current = None;
                }
                self.docs.remove(name);
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
            Some(cur) if cur != SCRATCH_KEY => self
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

    fn editor_focused(&self, ctx: &egui::Context) -> bool {
        match (&self.current, self.active_block()) {
            (Some(name), Some(r)) => ctx.memory(|m| m.has_focus(Self::editor_id(name, r.start))),
            _ => false,
        }
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
        let base = char_len(&doc.text[..r.start]);
        let blen = char_len(&doc.text[r.clone()]);
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
        self.drag_anchor = None;
        self.drag_last_pos = None;
        self.last_sel = None;
    }

    // 블록으로 이동 (위치는 이동 전 기준 바이트)
    fn go(&mut self, start: usize, caret: usize) {
        let removed = self.leave_blank();
        let fix = |p: usize| match removed {
            Some((r, n)) if p >= r + n => p - n,
            _ => p,
        };
        let (start, caret) = (fix(start), fix(caret));
        let Some(doc) = self.doc() else {
            return;
        };
        let c = char_len(&doc.text[..caret.min(doc.text.len())]);
        self.block_sel = None;
        self.active = Some(start);
        self.pending_sel = Some((c, c));
        self.focus_editor = true;
    }

    // 다중 블록 / 전체 문서 선택 구간 삭제
    fn delete_block_sel(&mut self) -> bool {
        let Some(sel) = self.block_sel.take() else {
            return false;
        };
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
        let (del_start, del_end, ins, caret_off) = if start == 0 && end == len {
            (0, len, "", 0)
        } else if start == 0 {
            (0, (end + post_nl).min(len), "", 0)
        } else if end == len {
            let rm = pre_nl.min(2);
            (start - rm, len, "", 0)
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
            self.pending_sel = Some((0, 0));
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
            let c = char_len(&doc.text[..caret.min(doc.text.len())]);
            self.active = Some(block_start);
            self.blank = 0;
            self.pending_sel = Some((c, c));
            self.focus_editor = true;
        }
        true
    }

    // 다중 블록 / 전체 문서 선택 구간을 새 문자열로 대체
    fn replace_block_sel(&mut self, with: &str) {
        let Some(sel) = self.block_sel.take() else {
            return;
        };
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
        let c = char_len(&doc.text[..caret]);
        self.active = Some(block_start);
        self.blank = 0;
        self.pending_sel = Some((c, c));
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
        let c = doc.chars;
        self.active = Some(a);
        self.blank = n;
        self.pending_sel = Some((c, c));
        self.focus_editor = true;
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
        let c = char_len(&doc.text[..a]);
        self.active = Some(a);
        self.blank = if rest_empty { 2 } else { 0 };
        self.pending_sel = Some((c, c));
        self.focus_editor = true;
    }

    fn set_text(&mut self, text: String, sel: Sel) {
        if let Some(doc) = self.doc_mut() {
            doc.text = text;
            doc.edited_at = Some(Instant::now());
            doc.refresh_meta();
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
        let c = char_len(&doc.text[..caret]);
        self.blank = 0;
        self.slash_sel = 0;
        self.active = Some(r.start);
        self.pending_sel = Some((c, c));
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

    // Enter: 목록은 다음 항목, 빈 항목은 목록 끝내기, 그 밖에는 블록 나누기. 코드·표·수식 안은 일반 줄바꿈.
    fn enter(&mut self, ctx: &egui::Context) -> bool {
        let sel = self.cursor(ctx);
        let (Some(r), Some(doc)) = (self.active_block(), self.doc()) else {
            return false;
        };
        if sel.0 != sel.1 {
            return false;
        }
        let text = doc.text.clone();
        let t0 = text[r.clone()].trim_start();
        if ["```", "~~~", "$$", ":::toggle", "|"]
            .iter()
            .any(|p| t0.starts_with(p))
        {
            return false;
        }
        let b = byte_idx(&text, sel.0);
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
            let mut t2 = text.clone();
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

    // 블록 맨 앞 Backspace: 빈 블록은 지우고, 내용이 있으면 앞 블록과 합친다
    fn backspace_at_start(&mut self, ctx: &egui::Context) -> bool {
        let sel = self.cursor(ctx);
        let (Some(r), Some(doc)) = (self.active_block(), self.doc()) else {
            return false;
        };
        if sel.0 != sel.1 || sel.0 != char_len(&doc.text[..r.start]) {
            return false;
        }
        if r.start == 0 && r.is_empty() {
            let post = doc.text.bytes().take_while(|&b| b == b'\n').count();
            if post == 0 {
                return false;
            }
            let mut t2 = doc.text.clone();
            t2.replace_range(0..post, "");
            self.blank = 0;
            self.active = Some(0);
            self.set_text(t2, (0, 0));
            return true;
        }
        let Some(prev) = doc
            .blocks
            .iter()
            .rev()
            .find(|x| x.end <= r.start && x.start < r.start)
            .cloned()
        else {
            return false;
        };
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
        true
    }

    // 블록 맨 끝 Delete: 빈 블록은 지우고, 내용이 있으면 뒤 블록과 합친다
    fn delete_at_end(&mut self, ctx: &egui::Context) -> bool {
        let sel = self.cursor(ctx);
        let (Some(r), Some(doc)) = (self.active_block(), self.doc()) else {
            return false;
        };
        if sel.0 != sel.1 || sel.0 != char_len(&doc.text[..r.end]) {
            return false;
        }
        let Some(next) = doc
            .blocks
            .iter()
            .find(|x| x.start >= r.end && x.start > r.start)
            .cloned()
        else {
            if r.is_empty() && r.start > 0 {
                return self.backspace_at_start(ctx);
            }
            return false;
        };
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
        true
    }

    // 첫 줄에서 ↑ / 마지막 줄에서 ↓ 는 이웃 블록으로
    fn arrow(&mut self, ctx: &egui::Context, up: bool) -> bool {
        let sel = self.cursor(ctx);
        let (Some(r), Some(doc)) = (self.active_block(), self.doc()) else {
            return false;
        };
        if sel.0 != sel.1 {
            return false;
        }
        let text = &doc.text;
        let b = byte_idx(text, sel.0).clamp(r.start, r.end);
        let blocks = &doc.blocks;
        let target = if up {
            if text[r.start..b].contains('\n') {
                return false;
            }
            blocks
                .iter()
                .rev()
                .find(|x| x.end <= r.start && x.start < r.start)
                .map(|x| (x.start, x.end))
        } else {
            if text[b..r.end].contains('\n') {
                return false;
            }
            blocks
                .iter()
                .find(|x| x.start > r.start && x.start >= r.end)
                .map(|x| (x.start, x.start))
        };
        match target {
            Some((s, c)) => {
                self.go(s, c);
                true
            }
            None => false,
        }
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

    // 캔버스 링크 중 로컬 .md 문서는 앱 안에서 연다 (나머지 URL은 시스템 브라우저)
    fn handle_links(&mut self, ctx: &egui::Context) {
        let local: Vec<String> = ctx.output_mut(|o| {
            let mut taken = Vec::new();
            o.commands.retain(|c| match c {
                egui::OutputCommand::OpenUrl(u) if !u.url.contains("://") => {
                    taken.push(u.url.clone());
                    false
                }
                _ => true,
            });
            taken
        });
        for url in local {
            let path = url.split('#').next().unwrap_or("").replace("%20", " ");
            if path.is_empty() {
                continue; // 문서 내 앵커(#...)는 무시
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
            if let Some(doc) = self.docs.get_mut(SCRATCH_KEY) {
                if let Some(st) = storage {
                    st.set_string("draft", doc.text.clone());
                    st.flush();
                }
                doc.saved.clear();
                doc.saved.push_str(&doc.text);
                doc.edited_at = None;
                doc.dirty_flag = false;
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
        let cmd = Modifiers::COMMAND;
        let shift_cmd = Modifiers::COMMAND | Modifiers::SHIFT;
        let k = |m, key| ctx.input_mut(|i| i.consume_key(m, key));
        // Shift 조합을 먼저 본다 (consume_key는 추가 Shift를 무시하므로)
        if k(shift_cmd, Key::O) {
            self.pick_folder();
        } else if k(shift_cmd, Key::I) {
            self.dialog = Some(Dialog::Image {
                url_tab: false,
                url: String::new(),
                alt: String::new(),
            });
        } else if k(cmd, Key::O) {
            self.open_files();
        }
        if k(cmd, Key::S) {
            self.trigger_save();
        }
        if k(cmd, Key::N) {
            self.new_scratch();
        }
        if k(cmd, Key::H) {
            self.open_find(ctx, true);
        } else if k(cmd, Key::F) {
            self.open_find(ctx, false);
        }
        if !self.find.query.is_empty() {
            if k(Modifiers::SHIFT, Key::F3) {
                self.find_step(ctx, false);
            } else if k(Modifiers::NONE, Key::F3) {
                self.find_step(ctx, true);
            }
        }
        if k(cmd, Key::P) {
            self.print(ctx);
        }
        if k(cmd, Key::Q) {
            self.guarded(After::Close);
        }
        if k(Modifiers::NONE, Key::F1) {
            self.dialog = Some(Dialog::Help);
        }
        if k(Modifiers::NONE, Key::F2) {
            if self.is_scratch() {
                self.dialog = Some(Dialog::SaveScratch(String::new()));
            } else if let Some(n) = self.current.clone() {
                self.ask_rename(&n);
            }
        }

        let focused = self.editor_focused(ctx);
        // 실행 취소: 찾기 입력창 등 다른 입력란에 포커스가 있으면 그쪽 기본 동작에 맡긴다
        if focused || ctx.memory(|m| m.focused().is_none()) {
            if k(shift_cmd, Key::Z) || k(cmd, Key::Y) {
                self.history(false);
                return;
            }
            if k(cmd, Key::Z) {
                self.history(true);
                return;
            }
        }
        if focused {
            if let Some(q) = self.slash_query() {
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
            } else {
                self.slash_sel = 0;
            }
        }
        if k(Modifiers::NONE, Key::Escape) {
            if self.find.open {
                self.find.open = false;
                if self.active.is_some() {
                    self.focus_editor = true;
                }
            } else if self.block_sel.is_some() {
                self.block_sel = None;
            } else {
                self.deactivate();
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
                }
                return;
            }
        }

        // 다중 블록 / 문서 전체 선택 상태에서의 삭제·복사·잘라내기·붙여넣기·서식·타이핑
        if self.block_sel.is_some() && !self.find.open {
            if k(Modifiers::NONE, Key::Backspace)
                || k(Modifiers::NONE, Key::Delete)
                || k(Modifiers::NONE, Key::Enter)
            {
                self.delete_block_sel();
                return;
            }
            let mut do_copy = k(cmd, Key::C);
            let mut do_cut = k(cmd, Key::X);
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

        if !focused {
            // 활성 블록에 선택 영역이 있는데 포커스만 잠시 빠진 경우 Backspace/Delete 처리
            if !self.find.open && self.active.is_some() {
                let sel = self.cursor(ctx);
                if sel.0 < sel.1
                    && (k(Modifiers::NONE, Key::Backspace) || k(Modifiers::NONE, Key::Delete))
                {
                    if let Some(t) = self.doc().map(|d| replace_range(&d.text, sel, "")) {
                        self.set_text(t, (sel.0, sel.0));
                    }
                }
            }
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
        if k(Modifiers::SHIFT, Key::Tab) {
            self.edit(ctx, |t, sel| Some(indent_lines(t, sel, true)));
        } else if k(Modifiers::NONE, Key::Tab) {
            self.edit(ctx, |t, sel| {
                let multi = t[byte_idx(t, sel.0)..byte_idx(t, sel.1)].contains('\n');
                let line = &t[byte_idx(t, line_start(t, sel.0))..byte_idx(t, line_end(t, sel.0))];
                if multi || list_marker(line).is_some() {
                    Some(indent_lines(t, sel, false))
                } else {
                    Some((replace_range(t, sel, "  "), (sel.0 + 2, sel.0 + 2)))
                }
            });
        }
        let pressed = |key| ctx.input(|i| i.key_pressed(key) && i.modifiers.is_none());
        if pressed(Key::Enter) && self.enter(ctx) {
            k(Modifiers::NONE, Key::Enter);
        }
        if pressed(Key::Backspace) && self.backspace_at_start(ctx) {
            k(Modifiers::NONE, Key::Backspace);
        }
        if pressed(Key::Delete) && self.delete_at_end(ctx) {
            k(Modifiers::NONE, Key::Delete);
        }
        if pressed(Key::ArrowUp) && self.arrow(ctx, true) {
            k(Modifiers::NONE, Key::ArrowUp);
        }
        if pressed(Key::ArrowDown) && self.arrow(ctx, false) {
            k(Modifiers::NONE, Key::ArrowDown);
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
        self.pending_sel = next; // 찾기 입력창 포커스는 유지
        self.scroll_cursor = true;
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

        // 1. 렌더링된 문단을 클릭하면 그 블록이 편집 상태가 된다
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

        // 2. 입력은 블록 끝(커서)에 들어간다
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
        for _ in 0..3 {
            h.key_press(egui::Key::ArrowLeft);
            h.run_steps(1);
        }
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
                app.docs
                    .insert(SCRATCH_KEY.into(), Doc::scratch(saved_draft));
                app.current = Some(SCRATCH_KEY.into());
                app
            });
        h2.run_steps(2);
        assert!(h2.state().is_scratch());
        assert_eq!(h2.state().doc().unwrap().text, "임시 메모 테스트");

        // 4. Ctrl+N으로 새 빈 메모 시작 (Ctrl+Z로 이전 메모 복구 가능)
        h2.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::N);
        h2.run_steps(2);
        assert!(h2.state().is_scratch());
        assert_eq!(h2.state().doc().unwrap().text, "");
        h2.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
        h2.run_steps(2);
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
}
