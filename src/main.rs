#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// MDEditor — 순수 로컬 네이티브 Markdown 편집기 (eframe/egui). 브라우저·웹뷰·로컬 서버 없음.
// 실행: mdeditor [폴더|파일]
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui;
use egui::text::{CCursor, CCursorRange};
use egui::{Key, Modifiers};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

const AUTO_SAVE_DELAY: Duration = Duration::from_secs(2);
const MAX_FILES: usize = 5000; // ponytail: 거대한 폴더(예: 사용자 홈) 스캔 상한
const PREVIEW_WIDTH: f32 = 860.0;

fn main() -> eframe::Result {
    let arg = std::env::args()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .map(PathBuf::from);
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MDEditor")
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([640.0, 420.0])
            .with_icon(app_icon())
            .with_drag_and_drop(true),
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

// 편집기(고정폭)의 한글이 시스템 대체 글꼴로 넓게 벌어지지 않도록 맑은 고딕을 뒤쪽 대체 글꼴로 등록.
// 다른 OS나 글꼴이 없으면 eframe의 시스템 글꼴 대체에 맡긴다.
fn install_korean_fallback(ctx: &egui::Context) {
    let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\malgun.ttf") else {
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("malgun".into(), egui::FontData::from_owned(bytes).into());
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("malgun".into());
    }
    ctx.set_fonts(fonts);
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

struct Doc {
    text: String,
    saved: String,
    crlf: bool,
    bom: bool,
    edited_at: Option<Instant>,
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
        Ok(Doc {
            saved: text.clone(),
            text,
            crlf,
            bom,
            edited_at: None,
        })
    }

    fn dirty(&self) -> bool {
        self.text != self.saved // ponytail: 매 프레임 문자열 비교, 수 MB 문서까지는 무시할 비용
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
fn wrap_selection(s: &str, (a, b): Sel, pre: &str, suf: &str, placeholder: &str) -> (String, Sel) {
    let inner = if a == b {
        placeholder.to_string()
    } else {
        s[byte_idx(s, a)..byte_idx(s, b)].to_string()
    };
    let out = replace_range(s, (a, b), &format!("{pre}{inner}{suf}"));
    let start = a + char_len(pre);
    (out, (start, start + char_len(&inner)))
}

// 선택된 각 줄의 머리 기호를 토글한다 (제목·인용·목록 버튼)
fn toggle_line_prefix(s: &str, (a, b): Sel, prefix: &str) -> (String, Sel) {
    let (ls, le) = (line_start(s, a), line_end(s, b));
    let block = &s[byte_idx(s, ls)..byte_idx(s, le)];
    let all_have = block.lines().all(|l| l.starts_with(prefix));
    let new_block: Vec<String> = block
        .split('\n')
        .map(|l| {
            if all_have {
                l[prefix.len()..].to_string()
            } else {
                let body = if prefix.starts_with('#') {
                    l.trim_start_matches('#').trim_start()
                } else {
                    l
                };
                format!("{prefix}{body}")
            }
        })
        .collect();
    let new_block = new_block.join("\n");
    let out = replace_range(s, (ls, le), &new_block);
    (out, (ls, ls + char_len(&new_block)))
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

// 찾기: 모든 일치 구간(문자 인덱스). 대소문자 무시는 문자 단위 비교.
fn find_all(s: &str, query: &str, case: bool) -> Vec<Sel> {
    let norm = |c: char| {
        if case {
            c
        } else {
            c.to_lowercase().next().unwrap_or(c)
        }
    };
    let hay: Vec<char> = s.chars().map(norm).collect();
    let needle: Vec<char> = query.chars().map(norm).collect();
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

// 목차: (레벨, 제목, 줄 번호, 줄 시작 문자 인덱스). 코드 펜스 안은 제외.
fn outline(s: &str) -> Vec<(usize, String, usize, usize)> {
    let mut out = Vec::new();
    let (mut in_code, mut ci) = (false, 0);
    for (n, line) in s.split('\n').enumerate() {
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_code = !in_code;
        } else if !in_code {
            let level = t.chars().take_while(|&c| c == '#').count();
            if (1..=6).contains(&level) && t[level..].starts_with(' ') {
                out.push((
                    level,
                    t[level..].trim().trim_end_matches('#').trim().into(),
                    n,
                    ci,
                ));
            }
        }
        ci += char_len(line) + 1;
    }
    out
}

// 미리보기 전용 변환. 바이트 길이를 보존해서 체크박스 위치가 원문과 1:1로 대응한다.
//   :::toggle 제목 → ##### ▸ 제목,  ::: → 공백,  ==형광펜== → **형광펜**,  [[Ctrl]] → ``Ctrl``
fn preprocess(s: &str) -> String {
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
            out.push_str(&" ".repeat(line.len()));
        } else {
            out.push_str(&preprocess_inline(line));
        }
    }
    debug_assert_eq!(out.len(), s.len());
    out
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
fn export_html(title: &str, md: &str) -> String {
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

// ---------- 앱 ----------

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Edit,
    Split,
    Preview,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Files,
    Outline,
}

enum Dialog {
    NewFile(String),
    Rename { from: String, to: String },
    Delete(String),
    Unsaved(After),
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
}

enum Tool {
    Wrap(&'static str, &'static str, &'static str),
    Line(&'static str),
    Block(&'static str),
    Image,
}

const TOOLS: &[(&str, &str, Tool)] = &[
    ("H1", "제목 1", Tool::Line("# ")),
    ("H2", "제목 2", Tool::Line("## ")),
    ("H3", "제목 3", Tool::Line("### ")),
    ("B", "굵게 (Ctrl+B)", Tool::Wrap("**", "**", "굵게")),
    ("I", "기울임 (Ctrl+I)", Tool::Wrap("*", "*", "기울임")),
    ("S", "취소선", Tool::Wrap("~~", "~~", "취소선")),
    ("==", "형광펜", Tool::Wrap("==", "==", "형광펜")),
    ("</>", "인라인 코드", Tool::Wrap("`", "`", "code")),
    ("•", "글머리 목록", Tool::Line("- ")),
    ("1.", "번호 목록", Tool::Line("1. ")),
    ("☑", "할 일", Tool::Line("- [ ] ")),
    ("“", "인용", Tool::Line("> ")),
    (
        "🔗",
        "링크 (Ctrl+K)",
        Tool::Wrap("[", "](https://)", "링크"),
    ),
    ("🖼", "이미지 삽입", Tool::Image),
    (
        "▦",
        "표",
        Tool::Block("| 제목 | 내용 |\n| --- | --- |\n|  |  |\n"),
    ),
    ("{ }", "코드 블록", Tool::Block("```\n\n```\n")),
    ("∑", "수식 블록", Tool::Block("$$\n\n$$\n")),
    ("💡", "콜아웃", Tool::Block("> [!NOTE]\n> \n")),
    ("▸", "토글", Tool::Block(":::toggle 제목\n내용\n:::\n")),
    ("―", "구분선", Tool::Block("---\n")),
];

struct App {
    root: PathBuf,
    files: Vec<String>,
    filter: String,
    docs: HashMap<String, Doc>,
    current: Option<String>,
    view: View,
    side: Side,
    font_size: f32,
    auto_save: bool,
    md_cache: CommonMarkCache,
    status: String,
    find: Find,
    dialog: Option<Dialog>,
    pending_sel: Option<Sel>,
    preview_jump: Option<f32>,
    preview_height: f32,
    focus_filter: bool,
    focus_editor: bool,
    allow_close: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, arg: Option<PathBuf>, cwd: &Path) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        install_korean_fallback(&cc.egui_ctx);
        let get = |k: &str| cc.storage.and_then(|s| s.get_string(k));
        let (root, open) = match arg {
            Some(p) => resolve_target(Some(&p), cwd),
            None => match get("root").map(PathBuf::from).filter(|p| p.is_dir()) {
                Some(r) => (r, get("file")),
                None => resolve_target(None, cwd),
            },
        };
        let view = match get("view").as_deref() {
            Some("edit") => View::Edit,
            Some("preview") => View::Preview,
            _ => View::Split,
        };
        let mut app = Self {
            root: PathBuf::new(),
            files: Vec::new(),
            filter: String::new(),
            docs: HashMap::new(),
            current: None,
            view,
            side: Side::Files,
            font_size: get("font_size")
                .and_then(|s| s.parse().ok())
                .unwrap_or(15.0),
            auto_save: get("auto_save").as_deref() != Some("false"),
            md_cache: CommonMarkCache::default(),
            status: String::new(),
            find: Find::default(),
            dialog: None,
            pending_sel: None,
            preview_jump: None,
            preview_height: 0.0,
            focus_filter: false,
            focus_editor: false,
            allow_close: false,
        };
        app.switch_root(root, open);
        app
    }

    fn switch_root(&mut self, root: PathBuf, open: Option<String>) {
        self.root = root;
        self.docs.clear();
        self.current = None;
        self.filter.clear();
        self.refresh_files();
        let first = open
            .filter(|n| self.root.join(n).is_file())
            .or_else(|| self.files.first().cloned());
        if let Some(name) = first {
            self.open_file(&name);
        }
        self.status = format!("작업 폴더: {}", self.root.display());
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
        self.current = Some(name.to_string());
        true
    }

    fn save(&mut self, name: &str) -> bool {
        let Some(doc) = self.docs.get_mut(name) else {
            return false;
        };
        match save_atomic(&self.root.join(name), doc.encoded().as_bytes()) {
            Ok(()) => {
                doc.saved = doc.text.clone();
                doc.edited_at = None;
                self.status = format!("저장됨: {name}");
                true
            }
            Err(e) => {
                self.status = format!("저장 실패: {name} — {e}");
                false
            }
        }
    }

    fn save_all(&mut self) -> bool {
        let dirty: Vec<String> = self
            .docs
            .iter()
            .filter(|(_, d)| d.dirty())
            .map(|(n, _)| n.clone())
            .collect();
        dirty.iter().filter(|n| !self.save(n)).count() == 0 // 실패해도 나머지는 계속 저장
    }

    fn any_dirty(&self) -> bool {
        self.docs.values().any(Doc::dirty)
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
        self.focus_editor = true;
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
        match std::fs::remove_file(self.root.join(name)) {
            Ok(()) => {
                self.docs.remove(name);
                if self.current.as_deref() == Some(name) {
                    self.current = None;
                }
                self.refresh_files();
                self.status = format!("삭제됨: {name}");
            }
            Err(e) => self.status = format!("삭제 실패: {e}"),
        }
    }

    // 저장 안 된 문서가 있으면 (자동 저장 시도 후) 확인 대화상자를 거친다
    fn guarded(&mut self, after: After) {
        if self.auto_save {
            self.save_all();
        }
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

    fn doc_dir(&self) -> PathBuf {
        let cur = self.current.as_deref().unwrap_or("");
        self.root
            .join(cur)
            .parent()
            .map_or_else(|| self.root.clone(), Path::to_path_buf)
    }

    fn editor_id(name: &str) -> egui::Id {
        egui::Id::unique(("editor", name))
    }

    // 저장 안 된 문서는 이름 앞에 ● 표시
    fn label(&self, name: &str) -> String {
        if self.docs.get(name).is_some_and(Doc::dirty) {
            format!("● {name}")
        } else {
            name.to_string()
        }
    }

    fn ask_rename(&mut self, name: &str) {
        self.dialog = Some(Dialog::Rename {
            from: name.to_string(),
            to: name.to_string(),
        });
    }

    fn cursor(&self, ctx: &egui::Context) -> Sel {
        let Some(name) = &self.current else {
            return (0, 0);
        };
        let len = self.docs.get(name).map_or(0, |d| char_len(&d.text));
        egui::TextEdit::load_state(ctx, Self::editor_id(name))
            .and_then(|s| s.cursor.char_range())
            .map(|r| {
                let r = r.as_sorted_char_range();
                (r.start.0.min(len), r.end.0.min(len))
            })
            .unwrap_or((len, len))
    }

    // 현재 문서에 편집 연산을 적용하고 커서/선택을 갱신한다
    fn edit(&mut self, ctx: &egui::Context, f: impl FnOnce(&str, Sel) -> Option<(String, Sel)>) {
        let sel = self.cursor(ctx);
        let Some(doc) = self.current.as_ref().and_then(|n| self.docs.get_mut(n)) else {
            return;
        };
        if let Some((text, new_sel)) = f(&doc.text, sel) {
            doc.text = text;
            doc.edited_at = Some(Instant::now());
            self.pending_sel = Some(new_sel);
            self.focus_editor = true;
        }
    }

    fn insert(&mut self, ctx: &egui::Context, s: &str) {
        let s = s.to_string();
        self.edit(ctx, move |t, sel| {
            let at = sel.0 + char_len(&s);
            Some((replace_range(t, sel, &s), (at, at)))
        });
    }

    // 블록은 항상 새 줄에서 시작하도록 앞에 줄바꿈을 보충한다
    fn insert_block(&mut self, ctx: &egui::Context, block: &str) {
        let block = block.to_string();
        self.edit(ctx, move |t, sel| {
            let need_nl = sel.0 > 0 && line_start(t, sel.0) != sel.0;
            let s = format!("{}{block}", if need_nl { "\n" } else { "" });
            let at = sel.0 + char_len(&s);
            Some((replace_range(t, sel, &s), (at, at)))
        });
    }

    fn run_tool(&mut self, ctx: &egui::Context, tool: &Tool) {
        match *tool {
            Tool::Wrap(pre, suf, ph) => {
                self.edit(ctx, |t, sel| Some(wrap_selection(t, sel, pre, suf, ph)))
            }
            Tool::Line(prefix) => self.edit(ctx, |t, sel| Some(toggle_line_prefix(t, sel, prefix))),
            Tool::Block(b) => self.insert_block(ctx, b),
            Tool::Image => {
                if let Some(p) = rfd::FileDialog::new()
                    .add_filter("이미지", &["png", "jpg", "jpeg", "gif", "webp", "bmp"])
                    .pick_file()
                {
                    self.insert_image_file(ctx, &p);
                }
            }
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

    // 미리보기 링크 중 로컬 .md 문서는 앱 안에서 연다 (나머지 URL은 시스템 브라우저)
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

    fn auto_save_tick(&mut self, ctx: &egui::Context) {
        if !self.auto_save {
            return;
        }
        let now = Instant::now();
        let mut due = Vec::new();
        for (name, doc) in &self.docs {
            if let (true, Some(t)) = (doc.dirty(), doc.edited_at) {
                let elapsed = now.duration_since(t);
                if elapsed >= AUTO_SAVE_DELAY {
                    due.push(name.clone());
                } else {
                    ctx.request_repaint_after(AUTO_SAVE_DELAY - elapsed);
                }
            }
        }
        for name in due {
            self.save(&name);
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.dialog.is_some() {
            return;
        }
        let cmd = Modifiers::COMMAND;
        let shift_cmd = Modifiers::COMMAND | Modifiers::SHIFT;
        let k = |m, key| ctx.input_mut(|i| i.consume_key(m, key));
        if k(shift_cmd, Key::O) {
            self.pick_folder();
        } else if k(cmd, Key::O) {
            self.side = Side::Files;
            self.focus_filter = true;
        }
        if k(cmd, Key::S) {
            if let Some(n) = self.current.clone() {
                self.save(&n);
            }
        }
        if k(cmd, Key::N) {
            self.dialog = Some(Dialog::NewFile(String::new()));
        }
        if k(cmd, Key::H) {
            self.open_find(true);
        } else if k(cmd, Key::F) {
            self.open_find(false);
        }
        if k(Modifiers::NONE, Key::F1) {
            self.dialog = Some(Dialog::Help);
        }
        if k(Modifiers::NONE, Key::F2) {
            if let Some(n) = self.current.clone() {
                self.ask_rename(&n);
            }
        }
        for (key, view) in [
            (Key::Num1, View::Edit),
            (Key::Num2, View::Split),
            (Key::Num3, View::Preview),
        ] {
            if k(Modifiers::ALT, key) {
                self.view = view;
            }
        }
        if k(Modifiers::NONE, Key::Escape) {
            self.find.open = false;
        }

        // 편집기에 포커스가 있을 때만: 서식 단축키, 스마트 Enter, Tab 들여쓰기
        let Some(name) = self.current.clone() else {
            return;
        };
        if !ctx.memory(|m| m.has_focus(Self::editor_id(&name))) {
            return;
        }
        if k(cmd, Key::B) {
            self.run_tool(ctx, &Tool::Wrap("**", "**", "굵게"));
        }
        if k(cmd, Key::I) {
            self.run_tool(ctx, &Tool::Wrap("*", "*", "기울임"));
        }
        if k(cmd, Key::K) {
            self.run_tool(ctx, &Tool::Wrap("[", "](https://)", "링크"));
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
        let plain_enter = ctx.input(|i| i.key_pressed(Key::Enter) && i.modifiers.is_none());
        if plain_enter {
            let sel = self.cursor(ctx);
            let text = &self.docs[&name].text;
            if sel.0 == sel.1 {
                if let Some((t, c)) = smart_enter(text, sel.0) {
                    ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
                    self.edit(ctx, |_, _| Some((t, (c, c))));
                }
            }
        }
    }

    fn open_find(&mut self, replace: bool) {
        self.find.open = true;
        self.find.replace = replace;
        self.find.focus = true;
        if self.view == View::Preview {
            self.view = View::Split;
        }
    }

    fn find_step(&mut self, ctx: &egui::Context, forward: bool) {
        let Some(doc) = self.current.as_ref().and_then(|n| self.docs.get(n)) else {
            return;
        };
        let matches = find_all(&doc.text, &self.find.query, self.find.case);
        if matches.is_empty() {
            self.status = "일치 항목 없음".into();
            return;
        }
        let (a, b) = self.cursor(ctx);
        let next = if forward {
            matches.iter().find(|m| m.0 >= b).or(matches.first())
        } else {
            matches.iter().rev().find(|m| m.1 <= a).or(matches.last())
        };
        self.pending_sel = next.copied();
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
        self.status = format!("{count}개 바꿈");
    }
}

// ---------- UI ----------

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            if self.auto_save {
                self.save_all();
            }
            if self.any_dirty() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.dialog = Some(Dialog::Unsaved(After::Close));
            }
        }

        self.handle_drops(&ctx);
        let pasted: Vec<_> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::PasteImage(img) => Some(img.clone()),
                    _ => None,
                })
                .collect()
        });
        for img in pasted {
            self.insert_pasted_image(&ctx, &img);
        }
        self.handle_shortcuts(&ctx);
        self.auto_save_tick(&ctx);
        self.title_bar(&ctx);

        egui::Panel::top("toolbar").show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::Panel::left("sidebar")
            .resizable(true)
            .default_size(240.0)
            .show(ui, |ui| self.sidebar(ui));
        egui::CentralPanel::default().show(ui, |ui| self.central(ui));

        self.dialogs(&ctx);
        self.handle_links(&ctx);

        if self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string("root", self.root.to_string_lossy().into_owned());
        storage.set_string("file", self.current.clone().unwrap_or_default());
        let view = match self.view {
            View::Edit => "edit",
            View::Split => "split",
            View::Preview => "preview",
        };
        storage.set_string("view", view.into());
        storage.set_string("font_size", self.font_size.to_string());
        storage.set_string("auto_save", self.auto_save.to_string());
    }
}

impl App {
    fn title_bar(&self, ctx: &egui::Context) {
        let title = match &self.current {
            Some(n) => format!("{} — MDEditor", self.label(n)),
            None => "MDEditor".into(),
        };
        // ponytail: 매 프레임 전송하지만 winit이 같은 제목은 무시한다
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui
                .button("📂 폴더")
                .on_hover_text("폴더 열기 (Ctrl+Shift+O)")
                .clicked()
            {
                self.pick_folder();
            }
            if ui.button("＋ 새 문서").on_hover_text("Ctrl+N").clicked() {
                self.dialog = Some(Dialog::NewFile(String::new()));
            }
            let can_save = self.current.is_some();
            if ui
                .add_enabled(can_save, egui::Button::new("💾 저장"))
                .on_hover_text("Ctrl+S")
                .clicked()
            {
                if let Some(n) = self.current.clone() {
                    self.save(&n);
                }
            }
            ui.separator();
            if let Some(name) = self.current.clone() {
                if ui
                    .add(
                        egui::Label::new(egui::RichText::new(self.label(&name)).strong())
                            .sense(egui::Sense::click()),
                    )
                    .on_hover_text("클릭해서 이름 변경 (F2)")
                    .clicked()
                {
                    self.ask_rename(&name);
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("?").on_hover_text("도움말 (F1)").clicked() {
                    self.dialog = Some(Dialog::Help);
                }
                ui.menu_button("⚙", |ui| {
                    ui.checkbox(&mut self.auto_save, "자동 저장 (2초)");
                    ui.add(
                        egui::Slider::new(&mut self.font_size, 11.0..=24.0)
                            .text("편집기 글자 크기"),
                    );
                    ui.separator();
                    egui::widgets::global_theme_preference_buttons(ui);
                });
                if ui
                    .add_enabled(self.current.is_some(), egui::Button::new("⤓ HTML"))
                    .on_hover_text("HTML로 내보내기")
                    .clicked()
                {
                    self.export();
                }
                if ui
                    .button("🔍")
                    .on_hover_text("찾기 (Ctrl+F) · 바꾸기 (Ctrl+H)")
                    .clicked()
                {
                    self.open_find(false);
                }
                ui.separator();
                ui.selectable_value(&mut self.view, View::Preview, "미리보기")
                    .on_hover_text("Alt+3");
                ui.selectable_value(&mut self.view, View::Split, "분할")
                    .on_hover_text("Alt+2");
                ui.selectable_value(&mut self.view, View::Edit, "편집")
                    .on_hover_text("Alt+1");
            });
        });
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

    fn status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if let Some(doc) = self.current.as_ref().and_then(|n| self.docs.get(n)) {
                let (a, b) = self.cursor(ui.ctx());
                let before = &doc.text[..byte_idx(&doc.text, a)];
                let line = before.matches('\n').count() + 1;
                let col = char_len(before.rsplit('\n').next().unwrap_or("")) + 1;
                let chars = char_len(&doc.text);
                ui.label(format!("{line}행 {col}열"));
                if b > a {
                    ui.label(format!("({}자 선택)", b - a));
                }
                ui.separator();
                ui.label(format!(
                    "{chars}자 · {}단어 · 약 {}분",
                    doc.text.split_whitespace().count(),
                    chars.div_ceil(500).max(1)
                ));
                ui.separator();
                ui.label(if doc.dirty() {
                    "편집 중"
                } else {
                    "저장됨"
                });
                ui.separator();
            }
            ui.label(&self.status);
        });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.side, Side::Files, "파일");
            ui.selectable_value(&mut self.side, Side::Outline, "목차");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("⟳").on_hover_text("새로고침").clicked() {
                    self.refresh_files();
                }
            });
        });
        ui.separator();
        match self.side {
            Side::Files => self.file_list(ui),
            Side::Outline => self.outline_list(ui),
        }
    }

    fn file_list(&mut self, ui: &mut egui::Ui) {
        let filter_id = egui::Id::unique("file-filter");
        let resp = ui.add(
            egui::TextEdit::singleline(&mut self.filter)
                .id(filter_id)
                .hint_text("파일 검색 (Ctrl+O)")
                .desired_width(f32::INFINITY),
        );
        if std::mem::take(&mut self.focus_filter) {
            resp.request_focus();
        }
        let needle = self.filter.to_lowercase();
        let shown: Vec<String> = self
            .files
            .iter()
            .filter(|f| f.to_lowercase().contains(&needle))
            .cloned()
            .collect();
        if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            if let Some(first) = shown.first() {
                self.open_file(first);
                self.focus_editor = true;
            }
        }
        ui.label(
            egui::RichText::new(self.root.display().to_string())
                .small()
                .weak(),
        )
        .on_hover_text("작업 폴더");
        egui::ScrollArea::vertical().show(ui, |ui| {
            if shown.is_empty() {
                ui.weak("마크다운 파일이 없습니다");
            }
            for name in shown {
                let selected = self.current.as_deref() == Some(name.as_str());
                let r = ui.selectable_label(selected, self.label(&name));
                if r.clicked() {
                    self.open_file(&name);
                }
                r.context_menu(|ui| {
                    if ui.button("이름 변경").clicked() {
                        self.ask_rename(&name);
                        ui.close();
                    }
                    if ui.button("삭제").clicked() {
                        self.dialog = Some(Dialog::Delete(name.clone()));
                        ui.close();
                    }
                });
            }
        });
    }

    fn outline_list(&mut self, ui: &mut egui::Ui) {
        let Some(doc) = self.current.as_ref().and_then(|n| self.docs.get(n)) else {
            ui.weak("열린 문서가 없습니다");
            return;
        };
        let items = outline(&doc.text);
        let total = doc.text.matches('\n').count().max(1) as f32;
        egui::ScrollArea::vertical().show(ui, |ui| {
            if items.is_empty() {
                ui.weak("제목(#)이 없습니다");
            }
            for (level, title, line, ci) in items {
                ui.horizontal(|ui| {
                    ui.add_space((level - 1) as f32 * 12.0);
                    let text = egui::RichText::new(title);
                    let text = if level == 1 { text.strong() } else { text };
                    if ui.link(text).clicked() {
                        self.pending_sel = Some((ci, ci));
                        self.preview_jump = Some(line as f32 / total);
                        self.focus_editor = self.view != View::Preview;
                    }
                });
            }
        });
    }

    fn central(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.current.is_none() {
            ui.centered_and_justified(|ui| {
                ui.label("왼쪽 목록에서 문서를 선택하거나 [＋ 새 문서]로 시작하세요.\n.md 파일이나 폴더를 창에 끌어다 놓아도 됩니다.");
            });
            return;
        }
        if self.find.open {
            self.find_bar(ui, &ctx);
            ui.separator();
        }
        if self.view != View::Preview {
            ui.horizontal_wrapped(|ui| {
                for (label, tip, tool) in TOOLS {
                    if ui.small_button(*label).on_hover_text(*tip).clicked() {
                        self.run_tool(&ctx, tool);
                    }
                }
            });
            ui.separator();
        }
        match self.view {
            View::Edit => self.editor(ui),
            View::Preview => self.preview(ui),
            View::Split => {
                ui.columns(2, |cols| {
                    self.editor(&mut cols[0]);
                    self.preview(&mut cols[1]);
                });
            }
        }
    }

    fn find_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let matches = self
            .current
            .as_ref()
            .and_then(|n| self.docs.get(n))
            .map(|d| find_all(&d.text, &self.find.query, self.find.case))
            .unwrap_or_default();
        let sel = self.cursor(ctx);
        ui.horizontal(|ui| {
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.find.query)
                    .hint_text("찾기")
                    .desired_width(200.0),
            );
            if std::mem::take(&mut self.find.focus) {
                r.request_focus();
            }
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                let back = ui.input(|i| i.modifiers.shift);
                self.find_step(ctx, !back);
                r.request_focus();
            }
            ui.toggle_value(&mut self.find.case, "Aa")
                .on_hover_text("대소문자 구분");
            let idx = matches.iter().position(|&m| m == sel).map_or(0, |i| i + 1);
            ui.label(format!("{idx}/{}", matches.len()));
            if ui.small_button("◀").clicked() {
                self.find_step(ctx, false);
            }
            if ui.small_button("▶").clicked() {
                self.find_step(ctx, true);
            }
            ui.toggle_value(&mut self.find.replace, "바꾸기");
            if self.find.replace {
                ui.add(
                    egui::TextEdit::singleline(&mut self.find.with)
                        .hint_text("바꿀 내용")
                        .desired_width(160.0),
                );
                if ui.button("바꾸기").clicked() {
                    self.replace_one(ctx);
                }
                if ui.button("모두 바꾸기").clicked() {
                    self.replace_everything(ctx);
                }
            }
            if ui.small_button("✕").on_hover_text("닫기 (Esc)").clicked() {
                self.find.open = false;
            }
        });
    }

    fn editor(&mut self, ui: &mut egui::Ui) {
        let Some(name) = self.current.clone() else {
            return;
        };
        let ctx = ui.ctx().clone();
        let id = Self::editor_id(&name);
        let jump = self.pending_sel.take();
        if let Some((a, b)) = jump {
            let mut st = egui::TextEdit::load_state(&ctx, id).unwrap_or_default();
            st.cursor
                .set_char_range(Some(CCursorRange::two(CCursor::new(a), CCursor::new(b))));
            egui::TextEdit::store_state(&ctx, id, st);
        }
        let font = egui::FontId::monospace(self.font_size);
        let Some(doc) = self.docs.get_mut(&name) else {
            return;
        };
        egui::ScrollArea::vertical()
            .id_salt(("editor-scroll", &name))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let out = egui::TextEdit::multiline(&mut doc.text)
                    .id(id)
                    .font(font)
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::symmetric(8, 6))
                    .desired_width(f32::INFINITY)
                    .min_size(ui.available_size())
                    .lock_focus(true)
                    .show(ui);
                if out.response.changed() {
                    doc.edited_at = Some(Instant::now());
                }
                if std::mem::take(&mut self.focus_editor) {
                    out.response.request_focus();
                }
                if let Some((_, b)) = jump {
                    let rect = out
                        .galley
                        .pos_from_cursor(CCursor::new(b))
                        .translate(out.galley_pos.to_vec2());
                    ui.scroll_to_rect(rect, Some(egui::Align::Center));
                }
            });
    }

    fn preview(&mut self, ui: &mut egui::Ui) {
        let Some(name) = self.current.clone() else {
            return;
        };
        // Windows는 file:///C:/... 형식이어야 로더가 경로로 인식한다
        let scheme = format!(
            "file:///{}/",
            to_slash(&self.doc_dir()).trim_start_matches('/')
        );
        let Some(doc) = self.docs.get_mut(&name) else {
            return;
        };
        // ponytail: 매 프레임 전처리+파싱. 수천 줄 문서까지는 충분히 빠름, 느려지면 show_scrollable로
        let before = preprocess(&doc.text);
        let mut shown = before.clone();
        let math = |ui: &mut egui::Ui, src: &str, _inline: bool| {
            ui.code(src.trim());
        };
        let mut area = egui::ScrollArea::vertical()
            .id_salt(("preview-scroll", &name))
            .auto_shrink([false, false]);
        if let Some(frac) = self.preview_jump.take() {
            area = area.vertical_scroll_offset(frac * self.preview_height);
        }
        let cache = &mut self.md_cache;
        let out = area.show(ui, |ui| {
            let w = ui.available_width().min(PREVIEW_WIDTH);
            let pad = ((ui.available_width() - w) / 2.0).max(8.0);
            ui.horizontal_top(|ui| {
                ui.add_space(pad);
                ui.vertical(|ui| {
                    ui.set_width(w - 16.0);
                    CommonMarkViewer::new()
                        .default_implicit_uri_scheme(scheme)
                        .max_image_width(Some(w as usize))
                        .render_math_fn(Some(&math))
                        .show_mut(ui, cache, &mut shown);
                });
            });
        });
        self.preview_height = out.content_size.y;
        if shown != before && apply_preview_change(&mut doc.text, &before, &shown) {
            doc.edited_at = Some(Instant::now());
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.dialog.take() else {
            return;
        };
        let (mut close, mut cancel) = (false, false);
        let mut error: Option<String> = None;
        let modal = egui::Modal::new(egui::Id::unique("dialog")).show(ctx, |ui| {
            ui.set_min_width(360.0);
            match &mut dialog {
                Dialog::NewFile(input) => {
                    ui.heading("새 문서");
                    ui.label("파일 이름 (하위 폴더 가능, 예: notes/할일.md)");
                    let r = ui.text_edit_singleline(input);
                    r.request_focus();
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    ui.horizontal(|ui| {
                        if ui.button("만들기").clicked() || enter {
                            match self.create_file(input) {
                                Ok(()) => close = true,
                                Err(e) => error = Some(e),
                            }
                        }
                        if ui.button("취소").clicked() {
                            close = true;
                        }
                    });
                }
                Dialog::Rename { from, to } => {
                    ui.heading("이름 변경");
                    ui.label(format!("현재: {from}"));
                    let r = ui.text_edit_singleline(to);
                    r.request_focus();
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    ui.horizontal(|ui| {
                        if ui.button("변경").clicked() || enter {
                            let (f, t) = (from.clone(), to.clone());
                            match self.rename(&f, &t) {
                                Ok(()) => close = true,
                                Err(e) => error = Some(e),
                            }
                        }
                        if ui.button("취소").clicked() {
                            close = true;
                        }
                    });
                }
                Dialog::Delete(name) => {
                    ui.heading("파일 삭제");
                    ui.label(format!(
                        "'{name}' 파일을 영구 삭제할까요? 되돌릴 수 없습니다."
                    ));
                    ui.horizontal(|ui| {
                        if ui.button("삭제").clicked() {
                            let n = name.clone();
                            self.delete(&n);
                            close = true;
                        }
                        if ui.button("취소").clicked() {
                            close = true;
                        }
                    });
                }
                Dialog::Unsaved(_) => {
                    ui.heading("저장되지 않은 변경 사항");
                    let names: Vec<&String> = self
                        .docs
                        .iter()
                        .filter(|(_, d)| d.dirty())
                        .map(|(n, _)| n)
                        .collect();
                    for n in names {
                        ui.label(format!("• {n}"));
                    }
                    ui.horizontal(|ui| {
                        if ui.button("저장").clicked() {
                            if self.save_all() {
                                close = true;
                            } else {
                                error = Some(self.status.clone());
                            }
                        }
                        if ui.button("저장 안 함").clicked() {
                            for d in self.docs.values_mut() {
                                d.text = d.saved.clone();
                            }
                            close = true;
                        }
                        if ui.button("취소").clicked() {
                            cancel = true;
                        }
                    });
                }
                Dialog::Help => {
                    ui.heading("단축키");
                    egui::Grid::new("help").striped(true).show(ui, |ui| {
                        for (k, v) in [
                            ("Ctrl+S", "저장 (자동 저장은 ⚙에서 켜고 끔)"),
                            ("Ctrl+N", "새 문서"),
                            ("Ctrl+O", "파일 검색"),
                            ("Ctrl+Shift+O", "폴더 열기"),
                            ("Ctrl+F / Ctrl+H", "찾기 / 바꾸기"),
                            ("Ctrl+B / I / K", "굵게 / 기울임 / 링크"),
                            ("Tab / Shift+Tab", "들여쓰기 / 내어쓰기"),
                            (
                                "Enter (목록)",
                                "다음 항목 자동 이어쓰기, 빈 항목은 목록 종료",
                            ),
                            ("Ctrl+V (이미지)", "스크린샷을 images/ 에 저장하고 삽입"),
                            ("Alt+1 / 2 / 3", "편집 / 분할 / 미리보기"),
                            ("F2", "이름 변경"),
                            ("F1", "도움말"),
                            ("끌어다 놓기", ".md 열기 · 폴더 열기 · 이미지 삽입"),
                        ] {
                            ui.strong(k);
                            ui.label(v);
                            ui.end_row();
                        }
                    });
                    ui.add_space(6.0);
                    ui.weak("미리보기의 체크박스를 클릭하면 원문이 바로 바뀝니다.");
                    if ui.button("닫기").clicked() {
                        close = true;
                    }
                }
            }
            if let Some(e) = &error {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
        });
        cancel |= modal.should_close();
        if let Some(e) = error {
            self.status = e;
            self.dialog = Some(dialog);
        } else if !close && !cancel {
            self.dialog = Some(dialog);
        } else if let (false, Dialog::Unsaved(after)) = (cancel, dialog) {
            self.finish(after); // 취소면 닫기/폴더 전환을 하지 않고 대화상자만 닫는다
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
        assert_eq!(toggle_line_prefix("a\nb", (0, 3), "- ").0, "- a\n- b");
        assert_eq!(toggle_line_prefix("- a\n- b", (0, 7), "- ").0, "a\nb");
        assert_eq!(toggle_line_prefix("# t", (0, 0), "## ").0, "## t");
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
}
