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

mod theme;
mod ui;

const AUTO_SAVE_DELAY: Duration = Duration::from_secs(2);
const MAX_FILES: usize = 5000; // ponytail: 거대한 폴더(예: 사용자 홈) 스캔 상한

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
fn split_blocks(s: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
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
    out
}

// ---------- 앱 ----------

enum Dialog {
    Files {
        filter: String,
        sel: usize,
    },
    NewFile(String),
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
    active: Option<usize>, // 캔버스에서 편집 중인 블록의 시작 바이트
    blank: usize,          // 활성 블록을 만들며 넣은 줄바꿈 수 (비운 채 떠나면 되돌린다)
    pending_sel: Option<Sel>,
    focus_editor: bool,
    scroll_cursor: bool,
    slash_sel: usize,
    toc_open: bool,
    toc_active: Option<usize>,
    scroll_to: Option<usize>,
    allow_close: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, arg: Option<PathBuf>, cwd: &Path) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        theme::install_fonts(&cc.egui_ctx);
        let get = |k: &str| cc.storage.and_then(|s| s.get_string(k));
        let (root, open) = match arg {
            Some(p) => resolve_target(Some(&p), cwd),
            None => match get("root").map(PathBuf::from).filter(|p| p.is_dir()) {
                Some(r) => (r, get("file")),
                None => resolve_target(None, cwd),
            },
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
            pending_sel: None,
            focus_editor: false,
            scroll_cursor: false,
            slash_sel: 0,
            toc_open: false,
            toc_active: None,
            scroll_to: None,
            allow_close: false,
        };
        app.switch_root(root, open);
        app
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
        self.docs.clear();
        self.current = None;
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
        if self.current.as_deref() != Some(name) {
            self.deactivate();
            self.current = Some(name.to_string());
        }
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
        let sel = self
            .current
            .as_ref()
            .and_then(|c| self.files.iter().position(|f| f == c))
            .unwrap_or(0);
        self.refresh_files();
        self.dialog = Some(Dialog::Files {
            filter: String::new(),
            sel,
        });
    }

    fn doc_dir(&self) -> PathBuf {
        let cur = self.current.as_deref().unwrap_or("");
        self.root
            .join(cur)
            .parent()
            .map_or_else(|| self.root.clone(), Path::to_path_buf)
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

    // 활성 블록의 바이트 구간 (새로 만든 빈 블록이면 a..a)
    fn active_block(&self) -> Option<Range<usize>> {
        let a = self.active?;
        let text = &self.doc()?.text;
        if a > text.len() || !text.is_char_boundary(a) {
            return None;
        }
        Some(
            split_blocks(text)
                .into_iter()
                .find(|b| b.start <= a && a <= b.end)
                .unwrap_or(a..a),
        )
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
        let len = char_len(&doc.text);
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
            .unwrap_or((base + blen, base + blen))
    }

    // 새 빈 블록을 비운 채 떠나면 만들 때 넣은 줄바꿈을 되돌린다. 지운 위치를 돌려준다.
    fn leave_blank(&mut self) -> Option<(usize, usize)> {
        let n = std::mem::take(&mut self.blank);
        let a = self.active?;
        let doc = self.doc_mut()?;
        let t = &doc.text;
        let empty = n > 0
            && a >= n
            && a <= t.len()
            && t.is_char_boundary(a)
            && t[..a].ends_with(&"\n".repeat(n))
            && t[a..].chars().next().is_none_or(|c| c == '\n');
        if !empty {
            return None;
        }
        doc.text.replace_range(a - n..a, "");
        Some((a - n, n))
    }

    fn deactivate(&mut self) {
        self.leave_blank();
        self.active = None;
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
        self.active = Some(start);
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
        let a = doc.text.len();
        let c = char_len(&doc.text);
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
        let blocks = split_blocks(&doc.text);
        let Some(prev) = blocks
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
        let blocks = split_blocks(text);
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

    fn auto_save_tick(&mut self, ctx: &egui::Context) {
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
            if let Some(n) = self.current.clone() {
                self.ask_rename(&n);
            }
        }

        let focused = self.editor_focused(ctx);
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
            } else {
                self.deactivate();
            }
        }
        if !focused {
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
        if pressed(Key::ArrowUp) && self.arrow(ctx, true) {
            k(Modifiers::NONE, Key::ArrowUp);
        }
        if pressed(Key::ArrowDown) && self.arrow(ctx, false) {
            k(Modifiers::NONE, Key::ArrowDown);
        }
    }

    fn open_find(&mut self, replace: bool) {
        self.find.open = true;
        self.find.replace = replace;
        self.find.focus = true;
    }

    fn find_step(&mut self, ctx: &egui::Context, forward: bool) {
        let Some(doc) = self.doc() else {
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
        self.leave_blank();
        self.pending_sel = next.copied(); // 찾기 입력창 포커스는 유지
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
        let _ = std::fs::remove_dir_all(&dir);
    }
}
