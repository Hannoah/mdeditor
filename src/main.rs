#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// mdeditor - 제로 의존성 Markdown 편집기 & 미리보기 서비스
// 표준 라이브러리(std)만 사용하여 최대한 가볍게 구성했습니다.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

mod markdown;

const HOST: &str = "127.0.0.1";
const PORT: u16 = 8080; // 사용 중이면 PORT..PORT+PORT_TRIES 범위에서 다음 포트를 시도
const PORT_TRIES: u16 = 10;

const MAX_BODY: usize = 10 * 1024 * 1024; // 요청 본문 상한 (10MB)
const MAX_LINE: u64 = 8 * 1024; // 요청/헤더 한 줄 상한
const MAX_HEADERS: usize = 64;
const MAX_CONN: usize = 64; // 동시 처리 연결 상한
const IO_TIMEOUT: Duration = Duration::from_secs(10);

static ACTIVE: AtomicUsize = AtomicUsize::new(0);
static EXIT_SEQ: AtomicU64 = AtomicU64::new(0);

fn main() {
    // 커맨드라인 인자 파싱: 연결 프로그램으로 실행 시 .md 파일 경로가 넘어옴
    //   예) mdeditor.exe "C:\docs\memo.md"
    let file_arg: Option<PathBuf> = std::env::args()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .map(PathBuf::from);

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // 작업 루트 결정:
    //   - 파일 인자가 있으면 그 파일의 부모 폴더
    //   - 없으면 현재 실행 디렉토리
    let (root, open_file): (PathBuf, Option<String>) = match &file_arg {
        Some(p) => {
            let abs = std::fs::canonicalize(p).unwrap_or_else(|_| cwd.join(p));
            let parent = abs.parent().map(Path::to_path_buf).unwrap_or(cwd);
            let name = abs.file_name().and_then(|n| n.to_str()).map(str::to_string);
            (parent, name)
        }
        None => (cwd, None),
    };
    // 서버 간 root 비교를 위해 정규화
    let root = std::fs::canonicalize(&root).unwrap_or(root);
    let info = info_json(&root);

    // 포트 바인딩: 이미 같은 폴더를 서비스 중인 mdeditor 가 있으면 브라우저만 열고,
    // 다른 폴더/다른 프로그램이 쓰고 있으면 다음 포트로 넘어간다.
    let mut bound: Option<(TcpListener, u16)> = None;
    for port in PORT..PORT + PORT_TRIES {
        let addr = format!("{HOST}:{port}");
        match TcpListener::bind(&addr) {
            Ok(l) => {
                bound = Some((l, port));
                break;
            }
            Err(_) if fetch_info(&addr).as_deref() == Some(info.as_str()) => {
                println!("[안내] 이 폴더를 서비스하는 mdeditor 가 이미 실행 중입니다. 앱 창을 엽니다.");
                open_app_window(&browser_url(&addr, &open_file));
                return;
            }
            Err(_) => {}
        }
    }
    let Some((listener, port)) = bound else {
        eprintln!("[오류] 사용 가능한 포트가 없습니다 ({PORT}~{}).", PORT + PORT_TRIES - 1);
        return;
    };
    let addr = format!("{HOST}:{port}");

    println!("================================================");
    println!(" mdeditor 가 실행되었습니다.");
    println!(" 앱 창: http://{addr}");
    println!(" 작업 폴더: {}", root.display());
    if let Some(name) = &open_file {
        println!(" 여는 파일: {name}");
    }
    println!(" 종료: 창을 닫거나 Ctrl + C");
    println!("================================================");

    // 서버 기동 직후 앱 창 자동 열기 (별도 스레드에서 약간의 지연 후)
    {
        let url = browser_url(&addr, &open_file);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(400));
            open_app_window(&url);
        });
    }

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if ACTIVE.fetch_add(1, Ordering::SeqCst) >= MAX_CONN {
                    ACTIVE.fetch_sub(1, Ordering::SeqCst);
                    continue; // 과부하: 연결을 버린다
                }
                let root = root.clone();
                let info = info.clone();
                thread::spawn(move || {
                    if let Err(e) = handle_client(stream, &root, port, &info) {
                        eprintln!("[연결 처리 오류] {e}");
                    }
                    ACTIVE.fetch_sub(1, Ordering::SeqCst);
                });
            }
            Err(e) => eprintln!("[연결 수락 오류] {e}"),
        }
    }
}

// 열어야 할 브라우저 URL (파일 인자가 있으면 ?file=... 로 자동 로드)
fn browser_url(addr: &str, open_file: &Option<String>) -> String {
    match open_file {
        Some(name) => format!("http://{addr}/?file={}", url_encode(name)),
        None => format!("http://{addr}/"),
    }
}

// 서버 식별용 JSON (/api/info). 두 번째 실행이 "같은 폴더의 mdeditor" 인지 판별하는 데 쓴다.
fn info_json(root: &Path) -> String {
    format!(
        "{{\"app\":\"mdeditor\",\"root\":\"{}\"}}",
        json_escape(&root.display().to_string())
    )
}

// 이미 떠 있는 서버의 /api/info 응답 본문을 가져온다 (std 만으로 최소 HTTP 요청)
fn fetch_info(addr: &str) -> Option<String> {
    let sa: SocketAddr = addr.parse().ok()?;
    let mut s = TcpStream::connect_timeout(&sa, Duration::from_millis(500)).ok()?;
    s.set_read_timeout(Some(Duration::from_millis(500))).ok()?;
    write!(s, "GET /api/info HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n").ok()?;
    let mut buf = String::new();
    (&mut s).take(64 * 1024).read_to_string(&mut buf).ok()?;
    buf.split_once("\r\n\r\n").map(|(_, body)| body.to_string())
}

struct Request {
    method: String,
    path: String,
    query: String,
    host: String,
    origin: Option<String>,
    oversize: bool,
    body: Vec<u8>,
}

fn handle_client(mut stream: TcpStream, root: &Path, port: u16, info: &str) -> std::io::Result<()> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;

    let req = match parse_request(&mut stream)? {
        Some(r) => r,
        None => return Ok(()),
    };

    // DNS rebinding / CSRF 방어: Host 와 Origin 이 우리 서버 주소일 때만 처리
    if !origin_allowed(&req, port) {
        return send_html(&mut stream, 403, "Forbidden", "<h1>403 Forbidden</h1>");
    }
    if req.oversize {
        return send_json(&mut stream, 413, "Payload Too Large", r#"{"error":"본문이 너무 큽니다 (최대 10MB)"}"#);
    }

    // exit 요청이 아닌 유효한 요청이 들어오면 대기 중인 자동 종료를 취소 (F5 새로고침 방어)
    if !(req.method == "POST" && req.path == "/api/exit") {
        EXIT_SEQ.fetch_add(1, Ordering::SeqCst);
    }

    match (req.method.as_str(), req.path.as_str()) {
        // 메인 편집기 페이지
        ("GET", "/") => send_html(&mut stream, 200, "OK", INDEX_HTML),

        // 서버 식별 (두 번째 실행 시 중복 서버 판별용)
        ("GET", "/api/info") => send_json(&mut stream, 200, "OK", info),

        // 작업 폴더의 .md 파일 목록 (JSON)
        ("GET", "/api/list") => {
            let body = list_markdown_files(root);
            send_json(&mut stream, 200, "OK", &body)
        }

        // 특정 파일 읽기: /api/load?file=foo.md
        ("GET", "/api/load") => {
            let file = query_param(&req.query, "file").unwrap_or_default();
            match safe_path(root, &file) {
                Some(p) => match std::fs::read_to_string(&p) {
                    // UTF-8 BOM(메모장 저장본)은 제거해야 첫 줄 제목이 인식된다
                    Ok(c) => send_json(&mut stream, 200, "OK", &json_wrap("content", c.trim_start_matches('\u{feff}'))),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        send_json(&mut stream, 404, "Not Found", r#"{"error":"파일을 찾을 수 없습니다"}"#)
                    }
                    // 읽기 실패를 빈 문서로 위장하면 저장 시 원본이 덮어써진다 → 반드시 오류로 알린다
                    Err(e) => send_json(
                        &mut stream,
                        500,
                        "Error",
                        &json_wrap("error", &format!("파일을 읽을 수 없습니다 (UTF-8 인코딩이 아닐 수 있음): {e}")),
                    ),
                },
                None => send_json(&mut stream, 400, "Bad Request", r#"{"error":"잘못된 파일명"}"#),
            }
        }

        // 마크다운 -> HTML 미리보기 렌더링
        ("POST", "/api/render") => {
            let md = String::from_utf8_lossy(&req.body).to_string();
            let html = markdown::to_html(&md);
            send_json(&mut stream, 200, "OK", &json_wrap("html", &html))
        }

        // 파일 저장: /api/save?file=foo.md (body = 내용)
        ("POST", "/api/save") => {
            let file = query_param(&req.query, "file").unwrap_or_default();
            match safe_path(root, &file) {
                Some(p) => match save_atomic(&p, &req.body) {
                    Ok(_) => send_json(&mut stream, 200, "OK", r#"{"ok":true}"#),
                    Err(e) => send_json(&mut stream, 500, "Error", &json_wrap("error", &e.to_string())),
                },
                None => send_json(&mut stream, 400, "Bad Request", r#"{"error":"잘못된 파일명"}"#),
            }
        }

        // 창 닫을 때 백엔드 자동 종료 (F5 새로고침 시 즉시 종료되지 않도록 2초 유예 시간 부여)
        ("POST", "/api/exit") => {
            let _ = send_json(&mut stream, 200, "OK", r#"{"bye":true}"#);
            let my_seq = EXIT_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(2000));
                // 2초 동안 새 요청(새로고침 등)이 발생하지 않았을 때만 실제로 프로세스 종료
                if EXIT_SEQ.load(Ordering::SeqCst) == my_seq {
                    std::process::exit(0);
                }
            });
            return Ok(());
        }

        _ => send_html(&mut stream, 404, "Not Found", "<h1>404 Not Found</h1>"),
    }
}

fn origin_allowed(req: &Request, port: u16) -> bool {
    let hosts = [format!("{HOST}:{port}"), format!("localhost:{port}")];
    hosts.contains(&req.host)
        && req
            .origin
            .as_ref()
            .is_none_or(|o| hosts.iter().any(|h| *o == format!("http://{h}")))
}

// 임시 파일에 쓴 뒤 rename → 저장 도중 크래시해도 원본이 잘리지 않는다
fn save_atomic(path: &Path, body: &[u8]) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp"); // .md 로 끝나지 않으므로 목록에 노출되지 않는다
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, body)?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

// ---------- HTTP 요청 파싱 ----------
// 한 줄을 MAX_LINE 까지만 읽는다 (무한 길이 헤더 방지)
fn read_line_limited(reader: &mut BufReader<TcpStream>) -> std::io::Result<String> {
    let mut line = String::new();
    reader.by_ref().take(MAX_LINE).read_line(&mut line)?;
    if line.len() as u64 >= MAX_LINE && !line.ends_with('\n') {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "헤더가 너무 깁니다"));
    }
    Ok(line)
}

fn parse_request(stream: &mut TcpStream) -> std::io::Result<Option<Request>> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let request_line = read_line_limited(&mut reader)?;
    if request_line.is_empty() {
        return Ok(None);
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let raw_path = parts.next().unwrap_or("/").to_string();

    let (path, query) = match raw_path.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (raw_path, String::new()),
    };

    // 헤더 파싱 (Content-Length / Host / Origin)
    let mut content_length = 0usize;
    let mut host = String::new();
    let mut origin = None;
    let mut count = 0;
    loop {
        let line = read_line_limited(&mut reader)?;
        let line = line.trim_end().to_ascii_lowercase();
        if line.is_empty() {
            break;
        }
        count += 1;
        if count > MAX_HEADERS {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "헤더가 너무 많습니다"));
        }
        if let Some((k, v)) = line.split_once(':') {
            let v = v.trim();
            match k {
                "content-length" => content_length = v.parse().unwrap_or(0),
                "host" => host = v.to_string(),
                "origin" => origin = Some(v.to_string()),
                _ => {}
            }
        }
    }

    // 바디 읽기 (상한 초과 시 읽지 않고 표시만)
    let oversize = content_length > MAX_BODY;
    let mut body = Vec::new();
    if !oversize && content_length > 0 {
        body = vec![0u8; content_length];
        reader.read_exact(&mut body)?;
    }

    Ok(Some(Request {
        method,
        path: url_decode(&path),
        query,
        host,
        origin,
        oversize,
        body,
    }))
}

// ---------- 경로 보안: 작업 폴더 밖/하위폴더 접근 차단, 마크다운 확장자만 허용 ----------
fn is_markdown_name(name: &str) -> bool {
    let l = name.to_lowercase();
    l.ends_with(".md") || l.ends_with(".markdown")
}

// file 은 query_param 에서 이미 디코딩된 값이다 (여기서 다시 디코딩하면 '+', '%' 파일명이 깨진다)
fn safe_path(root: &Path, file: &str) -> Option<PathBuf> {
    if !is_markdown_name(file) {
        return None;
    }
    // Windows: 대체 데이터 스트림(a.md:x), 금지 문자, 제어 문자
    if file.contains(|c: char| c.is_control() || "<>:\"|?*".contains(c)) {
        return None;
    }
    // Windows 예약 장치 이름 (CON.md, NUL.md, COM1.md ...)
    let stem = file.split('.').next().unwrap_or("").trim_end().to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit());
    if reserved {
        return None;
    }
    // 절대경로/상위 이동(..)/하위 디렉토리 모두 차단 → 파일명 한 조각만 허용
    let candidate = Path::new(file);
    let mut comps = candidate.components();
    match (comps.next(), comps.next()) {
        (Some(Component::Normal(_)), None) => Some(root.join(candidate)),
        _ => None,
    }
}

fn list_markdown_files(root: &Path) -> String {
    let mut names: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if is_markdown_name(name) {
                        names.push(name.to_string());
                    }
                }
            }
        }
    }
    names.sort();
    let items: Vec<String> = names.iter().map(|n| format!("\"{}\"", json_escape(n))).collect();
    format!("{{\"files\":[{}]}}", items.join(","))
}

// ---------- 응답 헬퍼 ----------
fn send_html(stream: &mut TcpStream, code: u16, status: &str, body: &str) -> std::io::Result<()> {
    send_response(stream, code, status, "text/html; charset=utf-8", body.as_bytes())
}

fn send_json(stream: &mut TcpStream, code: u16, status: &str, body: &str) -> std::io::Result<()> {
    send_response(stream, code, status, "application/json; charset=utf-8", body.as_bytes())
}

fn send_response(
    stream: &mut TcpStream,
    code: u16,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 {code} {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

// ---------- 유틸 ----------
fn query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        if k == key {
            Some(url_decode(v))
        } else {
            None
        }
    })
}

fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                if let (Some(h), Some(l)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                    out.push(h * 16 + l);
                    i += 3;
                    continue;
                }
                out.push(bytes[i]);
                i += 1;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn json_wrap(key: &str, value: &str) -> String {
    format!("{{\"{}\":\"{}\"}}", key, json_escape(value))
}

// ---------- 단독 앱 창 열기 (주소창/탭 없는 데스크톱 윈도우 모드) ----------
fn open_app_window(url: &str) {
    if std::env::var_os("MDEDITOR_NO_BROWSER").is_some() {
        return; // 테스트/서버 전용 실행용
    }
    #[cfg(windows)]
    {
        // 1순위: Microsoft Edge 단독 앱 모드 (Windows 10/11 기본 내장, 주소창/탭 없이 일반 데스크톱 창으로 실행)
        let edge_candidates = [
            "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
            "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
        ];
        for edge in edge_candidates {
            if std::path::Path::new(edge).exists() {
                if std::process::Command::new(edge)
                    .args([&format!("--app={url}"), "--window-size=1200,760"])
                    .spawn()
                    .is_ok()
                {
                    return;
                }
            }
        }

        // 2순위: Google Chrome 단독 앱 모드
        let chrome_candidates = [
            "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
            "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
        ];
        for chrome in chrome_candidates {
            if std::path::Path::new(chrome).exists() {
                if std::process::Command::new(chrome)
                    .args([&format!("--app={url}"), "--window-size=1200,760"])
                    .spawn()
                    .is_ok()
                {
                    return;
                }
            }
        }

        // 3순위: 시스템 기본 브라우저 fallback
        let _ = std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn();
    }
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").args(["-a", "Google Chrome", "--args", &format!("--app={url}")]).spawn()
        .or_else(|_| std::process::Command::new("open").arg(url).spawn());
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

// URL 쿼리용 최소 퍼센트 인코딩 (안전 문자 외 인코딩)
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

const INDEX_HTML: &str = include_str!("index.html");

#[cfg(test)]
mod tests {
    use super::*;

    fn req(host: &str, origin: Option<&str>) -> Request {
        Request {
            method: "POST".into(),
            path: "/api/save".into(),
            query: String::new(),
            host: host.into(),
            origin: origin.map(String::from),
            oversize: false,
            body: vec![],
        }
    }

    #[test]
    fn safe_path_accepts_plain_names() {
        let root = Path::new("root");
        assert!(safe_path(root, "memo.md").is_some());
        assert!(safe_path(root, "메모.MD").is_some());
        assert!(safe_path(root, "a+b %20.markdown").is_some()); // 이중 디코딩 없이 그대로
    }

    #[test]
    fn safe_path_rejects_escapes() {
        let root = Path::new("root");
        for bad in [
            "", "memo.txt", "../x.md", "..\\x.md", "sub/x.md", "sub\\x.md", "/abs.md",
            "C:\\x.md", "C:x.md", "a.md:ads.md", "con.md", "NUL.md", "com1.md", "lpt9.md",
            "a|b.md", "a\0.md",
        ] {
            assert!(safe_path(root, bad).is_none(), "통과하면 안 됨: {bad:?}");
        }
    }

    #[test]
    fn url_decode_handles_edges() {
        assert_eq!(url_decode("a%20b+c"), "a b c");
        assert_eq!(url_decode("%E3%84%B1"), "ㄱ");
        assert_eq!(url_decode("100%"), "100%");
        assert_eq!(url_decode("%zz"), "%zz");
        assert_eq!(url_decode("%"), "%");
    }

    #[test]
    fn host_and_origin_checks() {
        assert!(origin_allowed(&req("127.0.0.1:8080", None), 8080));
        assert!(origin_allowed(&req("localhost:8080", Some("http://localhost:8080")), 8080));
        assert!(!origin_allowed(&req("evil.com:8080", None), 8080)); // DNS rebinding
        assert!(!origin_allowed(&req("127.0.0.1:8080", Some("http://evil.com")), 8080)); // CSRF
        assert!(!origin_allowed(&req("127.0.0.1:8080", Some("null")), 8080));
        assert!(!origin_allowed(&req("127.0.0.1:8081", None), 8080));
    }

    #[test]
    fn atomic_save_roundtrip() {
        let dir = std::env::temp_dir().join("mdeditor_test_save");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.md");
        save_atomic(&p, b"one").unwrap();
        save_atomic(&p, b"two").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "two");
        assert!(!dir.join("t.md.tmp").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
