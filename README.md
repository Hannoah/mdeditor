# 📝 mdeditor

<p align="center">
  <strong>외부 의존성 제로(0개) — 표준 라이브러리(std)만으로 구현된 초경량 Markdown 편집기 & 데스크톱 앱</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-2021-DEA584?style=flat-square&logo=rust&logoColor=white" alt="Rust 2021" />
  <img src="https://img.shields.io/badge/Dependencies-0-brightgreen?style=flat-square" alt="Zero Dependencies" />
  <img src="https://img.shields.io/badge/Binary_Size-~414KB-blue?style=flat-square" alt="Binary Size" />
  <img src="https://img.shields.io/badge/Platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey?style=flat-square" alt="Platform" />
  <img src="https://img.shields.io/badge/License-MIT-orange?style=flat-square" alt="License" />
</p>

---

## 💡 소개

**mdeditor**는 수백 메가바이트의 메모리를 소모하는 일렉트론(Electron)이나 무거운 외부 웹뷰 런타임 바인딩 없이, **오직 Rust 표준 라이브러리(`std`)만으로 제작된 초경량 Markdown 데스크톱 앱**입니다.

- **외부 크레이트 0개**: 외부 패키지 다운로드나 복잡한 C 링커(MSVC/MinGW) 충돌 없이 `cargo build` 한 번으로 즉시 빌드됩니다.
- **초경량 단일 바이너리**: 릴리즈 빌드 최적화(`opt-level = "z"`, `LTO`, `strip`)를 통해 실행 파일 크기가 **약 414KB**에 불과하며, 시작 속도가 거의 즉각적입니다.
- **네이티브 데스크톱 앱 창 모드**: 주소창이나 탭 없는 깔끔한 독립 윈도우 창으로 실행되며, 창을 닫으면 백엔드 프로세스도 안전하게 함께 종료됩니다.

---

## ✨ 핵심 기능

### 🖥️ 데스크톱 & 편집 사용자 경험 (UX)
- **데스크톱 앱 모드**: Edge/Chrome의 단독 앱 창(`--app`) 모드로 실행되어 일반 네이티브 데스크톱 애플리케이션처럼 동작합니다.
- **실시간 반응형 미리보기**: 왼쪽에서 마크다운을 입력하면 오른쪽 미리보기에 즉시 HTML로 렌더링됩니다. (디바운스 & 비동기 응답 순서 보장)
- **에디터-미리보기 스크롤 동기화**: 양방향 비율 기반 스크롤 연동으로 긴 문서도 매끄럽게 작성할 수 있습니다.
- **다중 라인 들여쓰기 & 내어쓰기**: `Tab` / `Shift + Tab`을 누르면 단일 커서뿐만 아니라 드래그한 여러 줄 블록 전체를 간편하게 들여쓰기/내어쓰기할 수 있습니다.
- **실시간 문서 통계**: 상단 바에서 현재 문서의 글자 수와 줄 수를 실시간으로 제공합니다.
- **외부 파일 드래그 앤 드롭**: 외부 `.md` 파일을 창으로 끌어다 놓아 바로 편집할 수 있습니다. (작업 폴더 덮어쓰기 방어 적용)

### 📂 안전한 파일 관리 & 인스턴스 중복 방지
- **원자적 저장 (Atomic Save)**: 임시 파일(`.tmp`)에 완전히 작성한 후 원자적 이름 교체를 수행하므로, 저장 도중 전원이 꺼지거나 크래시가 발생해도 원본 파일이 절대 손상되지 않습니다.
- **미저장 변경 경고**: 수정 중인 내용이 있을 때 창을 닫거나 다른 파일로 전환하려 하면 확인 알림을 띄웁니다.
- **스마트 포트 바인딩 & 단일 인스턴스 활성화**: 이미 같은 폴더를 서비스 중인 mdeditor가 실행 중이면 중복 서버를 띄우지 않고 기존 앱 창을 포커스합니다. 다른 폴더의 문서를 열면 자동으로 다음 가용 포트(8080~8089)를 찾아 새 독립 인스턴스를 시작합니다.

### 🛡️ 상용 수준의 보안 설계 (Security & Hardening)
- **디렉토리 탈출(Path Traversal) 원천 차단**: 상위 디렉토리 이동(`..`), 서브디렉토리, 절대 경로 접근을 방어하며 단일 파일 컴포넌트만 허용합니다.
- **Windows 특수 장치명 & ADS 방어**: `CON`, `PRN`, `AUX`, `NUL`, `COM1-9`, `LPT1-9` 및 대체 데이터 스트림(`file.md:stream`), 파일명 금지 문자(`< > : " | ? *`) 차단.
- **DNS Rebinding & CSRF 방어**: `Host` 및 `Origin` 헤더를 검증하여 로컬 주소(`127.0.0.1`, `localhost`)에서 발생한 정상 요청만 처리합니다.
- **XSS 방어 & 스킴 화이트리스트**: 마크다운 렌더링 시 모든 HTML 엔티티를 이스케이프하며, 링크와 이미지 스킴은 `http://`, `https://`, `mailto:`, `#`만 허용합니다 (`javascript:`, `data:` 차단).
- **스택 오버플로 및 DoS 방어**: 블록인용 재귀 깊이 상한(8단계), 요청 본문 10MB 상한, 헤더 라인 8KB 상한, 타임아웃(10초), 최대 동시 연결 제한(64개) 적용.

---

## 🏗️ 시스템 아키텍처

```
┌─────────────────────────────────────────────────────────────────┐
│                       mdeditor.exe                              │
│                                                                 │
│   ┌─────────────────────────────────────────────────────────┐   │
│   │ src/main.rs (경량 HTTP 서버 & 보안 라우팅)                │   │
│   │  - std::net::TcpListener (의존성 없는 멀티스레드 서빙)    │   │
│   │  - safe_path (Path Traversal / ADS / 장치명 방어)        │   │
│   │  - save_atomic (임시 파일 기반 원자적 저장)              │   │
│   │  - DNS Rebinding / CSRF 방어 및 프로세스 생명주기 관리   │   │
│   └──────────────────────────┬──────────────────────────────┘   │
│                              │                                  │
│            ┌─────────────────┴─────────────────┐                │
│            ▼                                   ▼                │
│   ┌───────────────────────────┐    ┌────────────────────────┐   │
│   │ src/markdown.rs           │    │ src/index.html (UI)    │   │
│   │  - 제로 의존성 파서       │    │  - 에디터 / 미리보기   │   │
│   │  - XSS 방어 & 스키마 검증  │    │  - 스크롤 동기화       │   │
│   │  - ReDoS / 재귀 오버플로  │    │  - 다중 라인 Tab 지원  │   │
│   │    방어                   │    │  - 드래그 앤 드롭 지원 │   │
│   └───────────────────────────┘    └────────────────────────┘   │
└──────────────────────────────┬──────────────────────────────────┘
                               │ 시스템 WebView / Edge / Chrome --app 창 구동
                               ▼
┌─────────────────────────────────────────────────────────────────┐
│              Standalone Desktop App Window (1200x760)           │
└─────────────────────────────────────────────────────────────────┘
```

---

## ⌨️ 단축키 안내

| 단축키 | 기능 | 설명 |
|---|---|---|
| <kbd>Ctrl</kbd> + <kbd>S</kbd> | **파일 저장** | 현재 열려 있는 파일 원자적 저장 (새 파일이면 이름 확인) |
| <kbd>Ctrl</kbd> + <kbd>N</kbd> | **새 파일 생성** | 새로운 `.md` 파일을 생성하고 에디터 초기화 |
| <kbd>Tab</kbd> | **들여쓰기** | 커서 위치 또는 선택한 다중 라인 블록 전체 2칸 들여쓰기 |
| <kbd>Shift</kbd> + <kbd>Tab</kbd> | **내어쓰기** | 커서 앞 또는 선택한 다중 라인 블록 전체 2칸 내어쓰기 |

---

## 📦 지원하는 Markdown 문법

| 문법 | 예시 |
|---|---|
| **제목 (Headings)** | `# H1` 부터 `###### H6` 까지 |
| **강조 (Emphasis)** | `**굵게**`, `*기울임*`, `***굵은 기울임***` |
| **취소선 (Strikethrough)** | `~~취소선~~` |
| **인라인 코드 & 코드 블록** | `` `code` `` 및 ` ```rust ... ``` ` (HTML 특수문자 이스케이프) |
| **링크 & 이미지** | `[텍스트](https://...)`, `![대체텍스트](https://...)` (안전한 프로토콜만 허용) |
| **목록 (Lists)** | 순서 없는 목록(`-`, `*`, `+`), 순서 있는 목록(`1.`), 들여쓰기 중첩 지원 |
| **태스크 체크박스** | `- [ ] 할 일`, `- [x] 완료된 일` |
| **인용 (Blockquotes)** | `> 인용문` (다중 라인 병합, 중첩 인용, 목록 포함 지원) |
| **표 (Tables)** | `\| 제목 \| 내용 \|` + 정렬(`:---`, `:---:`, `---:`) 지원, 코드 내 파이프 보호 |
| **구분선 (Horizontal Rule)** | `---`, `***`, `___` |
| **이스케이프** | `\*`, `\[`, `\|` 등 특수문자 리터럴 출력 지원 |

> **단락 규칙**: 연속된 줄은 하나의 단락으로 합쳐지며, 줄 끝에 공백 2개(`  `)를 두면 강제 줄바꿈(`<br/>`)이 적용됩니다.

---

## 🚀 빠른 시작

### 1. 요구 사항
- [Rust 툴체인](https://rustup.rs/) (최신 Stable 버전 권장)

### 2. 빌드 및 실행

```bash
# 1. 저장소 클론
git clone https://github.com/Hannoah/mdeditor.git
cd mdeditor

# 2. 릴리즈 빌드 및 실행
cargo run --release
```

실행 즉시 단독 앱 윈도우가 화면에 표시됩니다. (기본 주소: `http://127.0.0.1:8080`)

### 3. Windows 편의 실행 (`mdeditor.bat`)
Windows 환경에서는 포함된 [mdeditor.bat](file:///C:/Users/kornah/Work/09_Personal_Project/mdeditor/mdeditor.bat)를 활용할 수 있습니다:
- **더블 클릭**: 현재 폴더를 작업 디렉토리로 하여 즉시 데스크톱 앱 창을 엽니다. (최초 1회 자동 빌드 수행)
- **파일 끌어다 놓기(Drag & Drop)**: 탐색기에서 원하는 `.md` 파일을 `mdeditor.bat` 아이콘 위로 끌어다 놓으면 해당 파일이 바로 열립니다.

### 4. CLI 명령 인자

```bash
# 현재 디렉토리에서 실행
mdeditor

# 특정 파일을 지정하여 열기 (해당 파일의 상위 폴더가 작업 디렉토리가 됨)
mdeditor C:\path\to\document.md

# 브라우저/앱 창을 자동으로 띄우지 않고 서버만 실행 (헤드리스/서버 모드)
set MDEDITOR_NO_BROWSER=1
mdeditor
```

---

## 🔌 REST API 명세

표준 HTTP API를 제공하므로 필요 시 외부 스크립트나 커스텀 도구와 쉽게 연동할 수 있습니다.

| 메서드 | 엔드포인트 | 파라미터 / 본문 | 설명 |
|---|---|---|---|
| `GET` | `/` | - | 메인 에디터 및 미리보기 웹 UI |
| `GET` | `/api/info` | - | 서버 식별 정보 (`{"app":"mdeditor","root":"..."}`) |
| `GET` | `/api/list` | - | 작업 디렉토리 내 `.md` 파일 목록 JSON 반환 |
| `GET` | `/api/load` | `?file=파일명.md` | 대상 파일 내용 반환 (`{"content":"..."}`) |
| `POST` | `/api/render` | 요청 본문(Markdown) | 변환된 HTML 반환 (`{"html":"..."}`) |
| `POST` | `/api/save` | `?file=파일명.md` (본문: 내용) | 원자적 파일 저장 (`{"ok":true}`) |
| `POST` | `/api/exit` | - | 백엔드 프로세스 안전 종료 (F5 새로고침 방어 2초 유예) |

---

## 🧪 테스트 실행

마크다운 파싱, 보안 검증, 원자적 파일 I/O 등 25개의 단위 테스트가 작성되어 있습니다.

```bash
cargo test
```

```text
running 25 tests
test markdown::tests::backslash_escapes ... ok
test markdown::tests::blockquote_merges_and_nests ... ok
test markdown::tests::code_block_is_escaped_and_unclosed_fence_ok ... ok
...
test tests::host_and_origin_checks ... ok
test tests::safe_path_accepts_plain_names ... ok
test tests::safe_path_rejects_escapes ... ok
test tests::atomic_save_roundtrip ... ok

test result: ok. 25 passed; 0 failed; 0 ignored; finished in 0.05s
```

---

## 📄 라이선스

이 프로젝트는 [MIT License](LICENSE)에 따라 자유롭게 사용, 수정, 배포할 수 있습니다.
