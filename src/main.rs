#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// MDEditor - 제로 의존성 Markdown 편집기 & 미리보기 서비스
// 표준 라이브러리(std)만 사용하여 최대한 가볍게 구성했습니다.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

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
static ACTIVE_TABS: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);

fn register_tab_ping(tab_id: &str) {
    if tab_id.is_empty() {
        return;
    }
    let mut guard = ACTIVE_TABS.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    let now = Instant::now();
    map.insert(tab_id.to_string(), now);
    map.retain(|_, last_seen| now.duration_since(*last_seen) < Duration::from_secs(15));
}

fn unregister_tab(tab_id: &str) -> usize {
    let mut guard = ACTIVE_TABS.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    let now = Instant::now();
    map.remove(tab_id);
    map.retain(|_, last_seen| now.duration_since(*last_seen) < Duration::from_secs(15));
    map.len()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // --version / -v 및 --help / -h 커맨드라인 옵션 처리
    for arg in &args {
        match arg.as_str() {
            "-v" | "--version" => {
                attach_console();
                print_version();
                return;
            }
            "-h" | "--help" => {
                attach_console();
                print_help();
                return;
            }
            _ => {}
        }
    }

    // 커맨드라인 파일/폴더 인자 파싱:
    //   - mdeditor.exe "C:\docs\memo.md" -> 작업 폴더 C:\docs, 여는 파일 memo.md
    //   - mdeditor.exe "C:\docs"         -> 작업 폴더 C:\docs, 여는 파일 없음
    //   - mdeditor.exe                   -> 현재 폴더(cwd)
    let file_arg: Option<PathBuf> = args
        .into_iter()
        .find(|a| !a.starts_with('-'))
        .map(PathBuf::from);

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let (root, open_file) = resolve_target(file_arg.as_deref(), &cwd);
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
                println!(
                    "[안내] 이 폴더를 서비스하는 MDEditor 가 이미 실행 중입니다. 앱 창을 엽니다."
                );
                open_app_window(&browser_url(&addr, &open_file));
                return;
            }
            Err(_) => {}
        }
    }
    let Some((listener, port)) = bound else {
        eprintln!(
            "[오류] 사용 가능한 포트가 없습니다 ({PORT}~{}).",
            PORT + PORT_TRIES - 1
        );
        return;
    };
    let addr = format!("{HOST}:{port}");

    println!("================================================");
    println!(" MDEditor 가 실행되었습니다.");
    println!(" 앱 창: http://{addr}");
    println!(" 작업 폴더: {}", root.display());
    if let Some(name) = &open_file {
        println!(" 여는 파일: {name}");
    }
    println!(" 종료: 창을 닫거나 Ctrl + C");
    println!("================================================");

    // 서버 기동 직후 독립 앱 창 자동 열기 (별도 스레드에서 약간의 지연 후)
    {
        let url = browser_url(&addr, &open_file);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
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

// 실행 인자(파일 또는 디렉토리)에 따른 작업 루트와 초기 로드 파일 결정
fn resolve_target(file_arg: Option<&Path>, cwd: &Path) -> (PathBuf, Option<String>) {
    match file_arg {
        Some(p) => {
            let abs = std::fs::canonicalize(p).unwrap_or_else(|_| cwd.join(p));
            if abs.is_dir() {
                let canonical_dir = std::fs::canonicalize(&abs).unwrap_or(abs);
                (canonical_dir, None)
            } else {
                let parent = abs
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| cwd.to_path_buf());
                let canonical_parent = std::fs::canonicalize(&parent).unwrap_or(parent);
                let name = abs.file_name().and_then(|n| n.to_str()).map(str::to_string);
                (canonical_parent, name)
            }
        }
        None => {
            let canonical_cwd = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
            (canonical_cwd, None)
        }
    }
}

fn print_version() {
    print_output(&format!("MDEditor v{}\n", env!("CARGO_PKG_VERSION")));
}

fn print_help() {
    let help = format!(
        "MDEditor v{}\n초경량 무의존성 Markdown 데스크톱 편집기 & 미리보기\n\n사용법:\n  mdeditor [옵션] [파일 또는 폴더 경로]\n\n옵션:\n  -v, --version    버전 정보를 출력합니다\n  -h, --help       도움말을 출력합니다\n\n예시:\n  mdeditor               현재 작업 폴더를 루트로 MDEditor 실행\n  mdeditor memo.md       memo.md 파일을 즉시 열고 편집\n  mdeditor C:\\docs       C:\\docs 폴더를 루트로 열기\n",
        env!("CARGO_PKG_VERSION")
    );
    print_output(&help);
}

fn print_output(text: &str) {
    #[cfg(windows)]
    {
        use std::fs::OpenOptions;
        if let Ok(mut f) = OpenOptions::new().write(true).open("CONOUT$") {
            let _ = write!(f, "{text}");
            return;
        }
    }
    print!("{text}");
}

#[cfg(windows)]
fn attach_console() {
    const ATTACH_PARENT_PROCESS: u32 = 0xFFFFFFFF;
    unsafe {
        #[link(name = "kernel32")]
        extern "system" {
            fn AttachConsole(dwProcessId: u32) -> i32;
        }
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(not(windows))]
fn attach_console() {}

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
    write!(
        s,
        "GET /api/info HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
    )
    .ok()?;
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
    let _ = stream.set_nodelay(true);
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
        return send_json(
            &mut stream,
            413,
            "Payload Too Large",
            r#"{"error":"본문이 너무 큽니다 (최대 10MB)"}"#,
        );
    }

    // exit 요청이 아닌 유효한 요청이 들어오면 대기 중인 자동 종료를 취소 (F5 새로고침 방어)
    if !(req.method == "POST" && req.path == "/api/exit") {
        EXIT_SEQ.fetch_add(1, Ordering::SeqCst);
    }

    match (req.method.as_str(), req.path.as_str()) {
        // 메인 편집기 페이지
        ("GET", "/") => send_html(&mut stream, 200, "OK", INDEX_HTML),

        // 파비콘 / 앱 창 아이콘 (브라우저 캐시 허용)
        ("GET", "/favicon.ico") => send_response_cache(
            &mut stream,
            200,
            "OK",
            "image/x-icon",
            APP_ICO,
            "public, max-age=86400",
        ),

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
                    Ok(c) => send_json(
                        &mut stream,
                        200,
                        "OK",
                        &json_wrap("content", c.trim_start_matches('\u{feff}')),
                    ),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => send_json(
                        &mut stream,
                        404,
                        "Not Found",
                        r#"{"error":"파일을 찾을 수 없습니다"}"#,
                    ),
                    // 읽기 실패를 빈 문서로 위장하면 저장 시 원본이 덮어써진다 → 반드시 오류로 알린다
                    Err(e) => send_json(
                        &mut stream,
                        500,
                        "Error",
                        &json_wrap(
                            "error",
                            &format!("파일을 읽을 수 없습니다 (UTF-8 인코딩이 아닐 수 있음): {e}"),
                        ),
                    ),
                },
                None => send_json(
                    &mut stream,
                    400,
                    "Bad Request",
                    r#"{"error":"잘못된 파일명"}"#,
                ),
            }
        }

        // 마크다운 -> HTML 미리보기 렌더링 (불필요한 String 복사 제거)
        ("POST", "/api/render") => {
            let md = String::from_utf8_lossy(&req.body);
            let html = markdown::to_html(&md);
            send_json(&mut stream, 200, "OK", &json_wrap("html", &html))
        }

        // 파일 저장: /api/save?file=foo.md (body = 내용)
        ("POST", "/api/save") => {
            let file = query_param(&req.query, "file").unwrap_or_default();
            match safe_path(root, &file) {
                Some(p) => match save_atomic(&p, &req.body) {
                    Ok(_) => send_json(&mut stream, 200, "OK", r#"{"ok":true}"#),
                    Err(e) => send_json(
                        &mut stream,
                        500,
                        "Error",
                        &json_wrap("error", &e.to_string()),
                    ),
                },
                None => send_json(
                    &mut stream,
                    400,
                    "Bad Request",
                    r#"{"error":"잘못된 파일명"}"#,
                ),
            }
        }

        // 파일 삭제: /api/delete?file=foo.md
        ("POST", "/api/delete") => {
            let file = query_param(&req.query, "file").unwrap_or_default();
            match safe_path(root, &file) {
                Some(p) => {
                    if p.exists() {
                        match std::fs::remove_file(&p) {
                            Ok(_) => send_json(&mut stream, 200, "OK", r#"{"ok":true}"#),
                            Err(e) => send_json(
                                &mut stream,
                                500,
                                "Error",
                                &json_wrap("error", &e.to_string()),
                            ),
                        }
                    } else {
                        send_json(
                            &mut stream,
                            404,
                            "Not Found",
                            r#"{"error":"삭제할 파일이 존재하지 않습니다"}"#,
                        )
                    }
                }
                None => send_json(
                    &mut stream,
                    400,
                    "Bad Request",
                    r#"{"error":"잘못된 파일명"}"#,
                ),
            }
        }

        // 파일 이름 변경: /api/rename?from=old.md&to=new.md
        ("POST", "/api/rename") => {
            let from = query_param(&req.query, "from").unwrap_or_default();
            let to = query_param(&req.query, "to").unwrap_or_default();
            match (safe_path(root, &from), safe_path(root, &to)) {
                (Some(from_p), Some(to_p)) => {
                    if !from_p.exists() {
                        send_json(
                            &mut stream,
                            404,
                            "Not Found",
                            r#"{"error":"원본 파일이 존재하지 않습니다"}"#,
                        )
                    } else if from_p == to_p {
                        send_json(&mut stream, 200, "OK", r#"{"ok":true}"#)
                    } else if to_p.exists() && !is_case_only_rename(&from_p, &to_p) {
                        send_json(
                            &mut stream,
                            409,
                            "Conflict",
                            r#"{"error":"동일한 이름의 파일이 이미 존재합니다"}"#,
                        )
                    } else {
                        match do_rename(&from_p, &to_p) {
                            Ok(_) => send_json(&mut stream, 200, "OK", r#"{"ok":true}"#),
                            Err(e) => send_json(
                                &mut stream,
                                500,
                                "Error",
                                &json_wrap("error", &e.to_string()),
                            ),
                        }
                    }
                }
                _ => send_json(
                    &mut stream,
                    400,
                    "Bad Request",
                    r#"{"error":"잘못된 파일명"}"#,
                ),
            }
        }

        // 로컬 애셋/미디어 안전 서빙: /api/raw?file=images/foo.png
        ("GET", "/api/raw") => {
            let file = query_param(&req.query, "file").unwrap_or_default();
            match safe_asset_path(root, &file) {
                Some(p) => match std::fs::read(&p) {
                    Ok(bytes) => {
                        let mime = content_type_for_path(&p);
                        send_response(&mut stream, 200, "OK", mime, &bytes)
                    }
                    Err(e) => send_json(
                        &mut stream,
                        500,
                        "Error",
                        &json_wrap("error", &e.to_string()),
                    ),
                },
                None => send_json(
                    &mut stream,
                    404,
                    "Not Found",
                    r#"{"error":"파일을 찾을 수 없거나 허용되지 않은 경로입니다"}"#,
                ),
            }
        }

        // 탭 생존 신호 (주기적 핑으로 다중 창/탭 관리)
        ("POST", "/api/ping") | ("GET", "/api/ping") => {
            let tab = query_param(&req.query, "tab").unwrap_or_default();
            register_tab_ping(&tab);
            send_json(&mut stream, 200, "OK", r#"{"pong":true}"#)
        }

        // 창 닫을 때 백엔드 자동 종료 (다중 탭 고려 및 F5 새로고침 방어)
        ("POST", "/api/exit") => {
            let tab = query_param(&req.query, "tab").unwrap_or_default();
            let remaining = if !tab.is_empty() {
                unregister_tab(&tab)
            } else {
                0
            };

            let _ = send_json(&mut stream, 200, "OK", r#"{"bye":true}"#);

            // 다른 활성 탭이 남아있다면 서버를 종료하지 않음
            if remaining > 0 {
                return Ok(());
            }

            let my_seq = EXIT_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(2000));
                // 2초 동안 새 요청(새로고침 등)이 발생하지 않았을 때만 실제로 프로세스 종료
                if EXIT_SEQ.load(Ordering::SeqCst) == my_seq {
                    std::process::exit(0);
                }
            });
            Ok(())
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

// 임시 파일에 쓴 뒤 동기화 후 rename → 저장 도중 크래시해도 원본이 잘리지 않고 충돌 방지
fn save_atomic(path: &Path, body: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let pid = std::process::id();
    let seq = EXIT_SEQ.fetch_add(1, Ordering::Relaxed);
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".tmp-{pid}-{seq}"));
    let tmp = PathBuf::from(tmp);

    {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(body)?;
        file.sync_all()?;
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

// 대소문자만 다른 이름 변경인지 판별 (대소문자 미구분 FS에서 같은 파일을 가리킴)
fn is_case_only_rename(from_p: &Path, to_p: &Path) -> bool {
    from_p.parent() == to_p.parent()
        && match (
            from_p.file_name().and_then(|s| s.to_str()),
            to_p.file_name().and_then(|s| s.to_str()),
        ) {
            (Some(a), Some(b)) => a.eq_ignore_ascii_case(b) && a != b,
            _ => false,
        }
}

// 이름 변경 실행. 대소문자만 바꾸는 경우 일부 FS에서 무시될 수 있어 임시 파일 경유.
fn do_rename(from_p: &Path, to_p: &Path) -> std::io::Result<()> {
    if let Some(parent) = to_p.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    if !is_case_only_rename(from_p, to_p) {
        return std::fs::rename(from_p, to_p);
    }
    let mut tmp = from_p.with_extension("rename-tmp");
    let mut n = 0;
    while tmp.exists() {
        n += 1;
        tmp = from_p.with_extension(format!("rename-tmp-{n}"));
    }
    std::fs::rename(from_p, &tmp)?;
    std::fs::rename(&tmp, to_p)
}

// ---------- HTTP 요청 파싱 ----------
// 한 줄을 MAX_LINE 까지만 읽는다 (무한 길이 헤더 방지)
fn read_line_limited<R: Read>(reader: &mut BufReader<R>) -> std::io::Result<String> {
    let mut line = String::new();
    reader.by_ref().take(MAX_LINE).read_line(&mut line)?;
    if line.len() as u64 >= MAX_LINE && !line.ends_with('\n') {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "헤더가 너무 깁니다",
        ));
    }
    Ok(line)
}

fn parse_request(stream: &mut TcpStream) -> std::io::Result<Option<Request>> {
    let mut reader = BufReader::new(stream);
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
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "헤더가 너무 많습니다",
            ));
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

// ---------- 경로 보안: 작업 폴더 내부(하위 폴더 포함) 안전 접근, 마크다운 확장자만 허용 ----------
fn is_markdown_name(name: &str) -> bool {
    let l = name.to_lowercase();
    l.ends_with(".md") || l.ends_with(".markdown")
}

// file 은 query_param 에서 이미 디코딩된 값이다 (여기서 다시 디코딩하면 '+', '%' 파일명이 깨진다)
fn safe_path(root: &Path, file: &str) -> Option<PathBuf> {
    if file.is_empty()
        || file.ends_with(' ')
        || file.ends_with('.')
        || file.starts_with(' ')
        || !is_markdown_name(file)
    {
        return None;
    }
    // Windows: 대체 데이터 스트림(a.md:x), 드라이브 문자(C:), UNC(\\), 루트(/), 금지 문자, 제어 문자
    if file.contains(':')
        || file.starts_with('\\')
        || file.starts_with('/')
        || file.contains(|c: char| c.is_control() || "<>\"|?*".contains(c))
    {
        return None;
    }

    let candidate = Path::new(file);
    let mut depth = 0;
    for comp in candidate.components() {
        match comp {
            Component::Normal(c) => {
                depth += 1;
                let s = c.to_string_lossy();
                if s.ends_with(' ') || s.ends_with('.') || s.starts_with(' ') {
                    return None;
                }
                let stem = s
                    .split('.')
                    .next()
                    .unwrap_or("")
                    .trim_end()
                    .to_ascii_uppercase();
                let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                    || (stem.len() == 4
                        && (stem.starts_with("COM") || stem.starts_with("LPT"))
                        && stem.as_bytes()[3].is_ascii_digit());
                if reserved {
                    return None;
                }
            }
            _ => return None, // 절대경로, 루트(/), 상위이동(..) 등 비정상 컴포넌트 원천 차단
        }
    }

    if depth == 0 {
        return None;
    }

    let full = root.join(candidate);
    let canonical_root = root
        .canonicalize()
        .ok()
        .unwrap_or_else(|| root.to_path_buf());

    if full.exists() {
        let canonical_full = full.canonicalize().ok()?;
        if !canonical_full.starts_with(&canonical_root) || !canonical_full.is_file() {
            return None;
        }
        Some(canonical_full)
    } else {
        // 새 파일인 경우: 가장 가까운 상위 디렉토리를 찾아 canonical_root 내부인지 검증
        let mut cur = full.parent();
        while let Some(ancestor) = cur {
            if ancestor.exists() {
                let canonical_ancestor = ancestor.canonicalize().ok()?;
                if !canonical_ancestor.starts_with(&canonical_root) {
                    return None;
                }
                break;
            }
            cur = ancestor.parent();
        }
        Some(full)
    }
}

// 로컬 이미지 및 문서 애셋 허용 확장자
fn is_allowed_asset_ext(ext: &str) -> bool {
    matches!(
        ext,
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "svg"
            | "webp"
            | "bmp"
            | "ico"
            | "avif"
            | "pdf"
            | "mp4"
            | "webm"
            | "mp3"
            | "wav"
            | "ogg"
            | "json"
            | "txt"
            | "csv"
    )
}

// 로컬 애셋/미디어 안전 경로 확인 (root 내부의 상대 하위 경로 허용, 경로 탈출 및 상위 이동 차단)
fn safe_asset_path(root: &Path, file: &str) -> Option<PathBuf> {
    if file.is_empty()
        || file.ends_with(' ')
        || file.ends_with('.')
        || file.contains(|c: char| c.is_control() || "<>:\"|?*".contains(c))
    {
        return None;
    }
    // Windows 드라이브 문자(C:), UNC(\\), ADS, 루트(/) 차단
    if file.contains(':') || file.starts_with('\\') || file.starts_with('/') {
        return None;
    }
    let p = Path::new(file);
    for comp in p.components() {
        match comp {
            Component::Normal(_) => {}
            _ => return None,
        }
    }
    let ext = p
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !is_allowed_asset_ext(&ext) {
        return None;
    }
    let full = root.join(p);
    let canonical_full = full.canonicalize().ok()?;
    let canonical_root = root
        .canonicalize()
        .ok()
        .unwrap_or_else(|| root.to_path_buf());
    if !canonical_full.starts_with(&canonical_root) {
        return None;
    }
    if !canonical_full.is_file() {
        return None;
    }
    Some(canonical_full)
}

fn content_type_for_path(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "avif" => "image/avif",
        "pdf" => "application/pdf",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "json" => "application/json",
        "txt" | "log" | "csv" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn list_markdown_files(root: &Path) -> String {
    let mut names: Vec<String> = Vec::new();
    collect_markdown_files(root, root, 0, &mut names);
    names.sort();
    let mut out = String::with_capacity(32 + names.len() * 32);
    out.push_str("{\"files\":[");
    for (i, name) in names.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('"');
        write_json_escaped(name, &mut out);
        out.push('"');
    }
    out.push_str("]}");
    out
}

fn collect_markdown_files(dir: &Path, base: &Path, depth: usize, out: &mut Vec<String>) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.')
            || matches!(
                name,
                "target" | "node_modules" | "dist" | "build" | "vendor" | "venv"
            )
        {
            continue;
        }
        if file_type.is_file() {
            if is_markdown_name(name) {
                if let Ok(rel) = path.strip_prefix(base) {
                    let rel_str = rel.to_string_lossy().replace('\\', "/");
                    out.push(rel_str);
                }
            }
        } else if file_type.is_dir() {
            collect_markdown_files(&path, base, depth + 1, out);
        }
    }
}

// ---------- 응답 헬퍼 ----------
fn send_html(stream: &mut TcpStream, code: u16, status: &str, body: &str) -> std::io::Result<()> {
    send_response(
        stream,
        code,
        status,
        "text/html; charset=utf-8",
        body.as_bytes(),
    )
}

fn send_json(stream: &mut TcpStream, code: u16, status: &str, body: &str) -> std::io::Result<()> {
    send_response(
        stream,
        code,
        status,
        "application/json; charset=utf-8",
        body.as_bytes(),
    )
}

fn send_response(
    stream: &mut TcpStream,
    code: u16,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    send_response_cache(stream, code, status, content_type, body, "no-cache")
}

fn send_response_cache(
    stream: &mut TcpStream,
    code: u16,
    status: &str,
    content_type: &str,
    body: &[u8],
    cache_control: &str,
) -> std::io::Result<()> {
    let mut writer = BufWriter::with_capacity(16 * 1024, stream);
    write!(
        writer,
        "HTTP/1.1 {code} {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: {cache_control}\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    writer.write_all(body)?;
    writer.flush()
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
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).to_string())
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn write_json_escaped(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + s.len() / 8 + 16);
    write_json_escaped(s, &mut out);
    out
}

fn json_wrap(key: &str, value: &str) -> String {
    let mut out = String::with_capacity(key.len() + value.len() + 16);
    out.push_str("{\"");
    write_json_escaped(key, &mut out);
    out.push_str("\":\"");
    write_json_escaped(value, &mut out);
    out.push_str("\"}");
    out
}

// ---------- 단독 데스크톱 앱 창 열기 (주소창/탭 없는 데스크톱 윈도우 모드) ----------
fn open_app_window(url: &str) {
    if std::env::var_os("MDEDITOR_NO_BROWSER").is_some() {
        return; // 테스트/서버 전용 실행용
    }

    let force_browser = std::env::var_os("MDEDITOR_BROWSER").is_some();

    #[cfg(windows)]
    if !force_browser {
        // 1순위: Microsoft Edge 단독 앱 창 모드 (Windows 10/11 기본 내장)
        let edge_candidates = [
            "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
            "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
        ];
        for edge in edge_candidates {
            if std::path::Path::new(edge).exists()
                && std::process::Command::new(edge)
                    .args([&format!("--app={url}"), "--window-size=1280,820"])
                    .spawn()
                    .is_ok()
            {
                return;
            }
        }

        // 2순위: Google Chrome / Brave 단독 앱 창 모드
        let chrome_candidates = [
            "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
            "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
            "C:\\Program Files\\BraveSoftware\\Brave-Browser\\Application\\brave.exe",
        ];
        for chrome in chrome_candidates {
            if std::path::Path::new(chrome).exists()
                && std::process::Command::new(chrome)
                    .args([&format!("--app={url}"), "--window-size=1280,820"])
                    .spawn()
                    .is_ok()
            {
                return;
            }
        }
    }

    #[cfg(target_os = "macos")]
    if !force_browser {
        if std::process::Command::new("open")
            .args(["-a", "Google Chrome", "--args", &format!("--app={url}")])
            .spawn()
            .is_ok()
        {
            return;
        }
    }

    // fallback: 시스템 기본 브라우저
    #[cfg(windows)]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
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
const APP_ICO: &[u8] = include_bytes!("../app.ico");

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
        assert!(safe_path(root, "sub/guide.md").is_some());
        assert!(safe_path(root, "docs/sub/guide.markdown").is_some());
    }

    #[test]
    fn safe_path_rejects_escapes() {
        let root = Path::new("root");
        for bad in [
            "",
            "memo.txt",
            "../x.md",
            "..\\x.md",
            "sub/../x.md",
            "sub/../../x.md",
            "/abs.md",
            "\\abs.md",
            "C:\\x.md",
            "C:x.md",
            "a.md:ads.md",
            "con.md",
            "sub/con.md",
            "NUL.md",
            "com1.md",
            "lpt9.md",
            "a|b.md",
            "a\0.md",
            "trailing.md ",
            "trailing.md.",
            "sub /x.md",
        ] {
            assert!(safe_path(root, bad).is_none(), "통과하면 안 됨: {bad:?}");
        }
    }

    #[test]
    fn list_markdown_files_recursive() {
        let dir = std::env::temp_dir().join("mdeditor_test_list_rec");
        let sub = dir.join("docs");
        let ignored = dir.join("target");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::create_dir_all(&ignored).unwrap();
        std::fs::write(dir.join("root.md"), b"root").unwrap();
        std::fs::write(sub.join("sub.md"), b"sub").unwrap();
        std::fs::write(ignored.join("ignored.md"), b"ignored").unwrap();

        let json = list_markdown_files(&dir);
        assert!(json.contains("\"root.md\""));
        assert!(json.contains("\"docs/sub.md\""));
        assert!(!json.contains("ignored.md"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn tab_session_lifecycle() {
        register_tab_ping("tab-1");
        register_tab_ping("tab-2");
        assert_eq!(unregister_tab("tab-1"), 1);
        assert_eq!(unregister_tab("tab-2"), 0);
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
        assert!(origin_allowed(
            &req("localhost:8080", Some("http://localhost:8080")),
            8080
        ));
        assert!(!origin_allowed(&req("evil.com:8080", None), 8080)); // DNS rebinding
        assert!(!origin_allowed(
            &req("127.0.0.1:8080", Some("http://evil.com")),
            8080
        )); // CSRF
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

    #[test]
    fn safe_asset_path_security() {
        let dir = std::env::temp_dir().join("mdeditor_test_assets");
        let sub = dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        let valid_img = sub.join("logo.png");
        std::fs::write(&valid_img, b"fake png").unwrap();

        let canonical_dir = dir.canonicalize().unwrap();
        assert!(safe_asset_path(&canonical_dir, "sub/logo.png").is_some());

        // 확장자 필터링 (허용되지 않은 파일 차단)
        let exe_file = dir.join("evil.exe");
        std::fs::write(&exe_file, b"executable").unwrap();
        assert!(safe_asset_path(&canonical_dir, "evil.exe").is_none());

        // 상위 이동(..) 탈출 차단
        assert!(safe_asset_path(&canonical_dir, "../secret.png").is_none());
        assert!(safe_asset_path(&canonical_dir, "sub/../../secret.png").is_none());

        // Windows 드라이브/절대경로/ADS 차단
        assert!(safe_asset_path(&canonical_dir, "C:\\logo.png").is_none());
        assert!(safe_asset_path(&canonical_dir, "/logo.png").is_none());
        assert!(safe_asset_path(&canonical_dir, "logo.png:stream").is_none());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn mime_type_detection() {
        assert_eq!(content_type_for_path(Path::new("a.png")), "image/png");
        assert_eq!(content_type_for_path(Path::new("a.jpg")), "image/jpeg");
        assert_eq!(content_type_for_path(Path::new("a.svg")), "image/svg+xml");
        assert_eq!(content_type_for_path(Path::new("a.pdf")), "application/pdf");
        assert_eq!(
            content_type_for_path(Path::new("a.unknown")),
            "application/octet-stream"
        );
    }

    #[test]
    fn case_only_rename_detection() {
        let root = Path::new("root");
        assert!(is_case_only_rename(&root.join("A.md"), &root.join("a.md")));
        assert!(!is_case_only_rename(&root.join("a.md"), &root.join("a.md")));
        assert!(!is_case_only_rename(&root.join("a.md"), &root.join("b.md")));
        assert!(!is_case_only_rename(
            &root.join("a.md"),
            &root.join("sub").join("A.md")
        ));
    }

    #[test]
    fn case_only_rename_roundtrip() {
        let dir = std::env::temp_dir().join("mdeditor_test_rename");
        std::fs::create_dir_all(&dir).unwrap();
        let from = dir.join("Case.md");
        std::fs::write(&from, b"hi").unwrap();
        let to = dir.join("case.md");
        do_rename(&from, &to).unwrap();
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "hi");
        let entries: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
        assert_eq!(entries.len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn file_deletion_and_safe_path() {
        let dir = std::env::temp_dir().join("mdeditor_test_delete");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("to_delete.md");
        std::fs::write(&file, b"content").unwrap();
        assert!(file.exists());

        let p = safe_path(&dir, "to_delete.md").unwrap();
        std::fs::remove_file(&p).unwrap();
        assert!(!file.exists());

        assert!(safe_path(&dir, "to_delete.md").is_some());
        assert!(!p.exists());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolve_target_file_and_directory() {
        let temp = std::env::temp_dir().join("mdeditor_test_resolve");
        let sub = temp.join("folder");
        std::fs::create_dir_all(&sub).unwrap();
        let file = sub.join("doc.md");
        std::fs::write(&file, b"# Hello").unwrap();

        // 1. 디렉토리를 넘겼을 때
        let (root, open) = resolve_target(Some(&sub), &temp);
        assert_eq!(root, sub.canonicalize().unwrap());
        assert_eq!(open, None);

        // 2. 파일을 넘겼을 때
        let (root, open) = resolve_target(Some(&file), &temp);
        assert_eq!(root, sub.canonicalize().unwrap());
        assert_eq!(open.as_deref(), Some("doc.md"));

        // 3. 인자가 없을 때 (None)
        let (root, open) = resolve_target(None, &temp);
        assert_eq!(root, temp.canonicalize().unwrap());
        assert_eq!(open, None);

        std::fs::remove_dir_all(&temp).unwrap();
    }

    #[test]
    fn json_escape_and_wrap() {
        let wrapped = json_wrap("title", "Hello \"world\"\nLine 2");
        assert_eq!(wrapped, "{\"title\":\"Hello \\\"world\\\"\\nLine 2\"}");
    }
}
