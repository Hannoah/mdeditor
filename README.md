# 📝 mdeditor

<p align="center">
  <strong>외부 의존성 제로(0개) — 표준 라이브러리(std)만으로 구현된 초경량 Markdown 편집기 & 데스크톱 앱</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-2021-DEA584?style=flat-square&logo=rust&logoColor=white" alt="Rust 2021" />
  <img src="https://img.shields.io/badge/Dependencies-0-brightgreen?style=flat-square" alt="Zero Dependencies" />
  <img src="https://img.shields.io/badge/Binary_Size-~561KB-blue?style=flat-square" alt="Binary Size" />
  <img src="https://img.shields.io/badge/Platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey?style=flat-square" alt="Platform" />
  <img src="https://img.shields.io/badge/Tests-36%20Passed-brightgreen?style=flat-square" alt="Tests" />
  <img src="https://img.shields.io/badge/License-MIT-orange?style=flat-square" alt="License" />
</p>

---

## 💡 소개

**mdeditor**는 수백 메가바이트의 메모리를 소모하는 일렉트론(Electron)이나 무거운 외부 웹뷰 런타임 바인딩 없이, **오직 Rust 표준 라이브러리(`std`)만으로 제작된 초경량 Notion 스타일 Markdown 데스크톱 앱**입니다.

- **외부 크레이트 0개 (Zero Dependencies)**: 외부 패키지 다운로드나 복잡한 C 링커(MSVC/MinGW) 충돌 없이 `cargo build` 한 번으로 즉시 빌드됩니다.
- **초경량 단일 바이너리 (~561KB)**: 릴리즈 빌드 최적화(`opt-level = "z"`, `LTO`, `strip`) 및 고해상도 다중 해상도 아이콘 임베딩을 통해 실행 파일 크기가 **약 561KB**에 불과하며, 시작 속도가 거의 즉각적입니다.
- **Notion 수준의 현대적 표현력**: GitHub Alerts/Notion 콜아웃 블록, LaTeX 수식(KaTeX), Mermaid 다이어그램, 코드 구문 강조, 접기(Toggle), 표, 형광펜, 키캡, 각주 등 풍부한 문법을 지원합니다.
- **로컬 이미지 & 애셋 서빙**: 원격 URL뿐만 아니라 문서와 같은 폴더나 하위 폴더(`images/`, `assets/`)에 위치한 로컬 그림 파일도 실시간으로 안전하게 서빙하여 보여줍니다.
- **편리한 데스크톱 통합**: Windows 설치 마법사(`install.bat`)를 통해 `.md` 파일 기본 연결, 우클릭 메뉴, 시작 메뉴 및 바탕화면 바로가기 등록을 지원합니다.

---

## ✨ 핵심 기능

### 🖥️ 데스크톱 & 편집 사용자 경험 (UX)
- **마크다운 서식 툴바**: 굵게, 기울임, 취소선, 형광펜, 코드, 헤딩, 콜아웃, 수식, 코드블록, Mermaid 다이어그램, 표, 링크, 이미지, 토글, 태스크, 구분선, 각주 등 19종의 도구를 원클릭으로 삽입/감싸기 가능합니다.
- **스마트 Enter 목록 이어쓰기**: 목록(`-`, `*`, `1.`)이나 체크박스(`- [ ]`) 항목 작성 중 Enter를 누르면 다음 번호나 불릿이 자동으로 생성되며, 내용 없이 Enter를 누르면 목록이 깔끔하게 종료됩니다.
- **자동 괄호/따옴표 닫기**: 텍스트를 드래그한 상태에서 `(`, `[`, `{`, `"`, `` ` ``, `*`를 누르면 선택 영역이 해당 기호로 감싸집니다.
- **드래그 조절 가능한 분할 구분선**: 에디터와 미리보기 사이의 경계선을 마우스로 드래그하여 자유롭게 너비를 조절할 수 있으며, 더블 클릭 시 즉시 50:50으로 초기화됩니다. (조절 비율 자동 저장)
- **3단계 뷰 모드 전환**: ✏️ 에디터 전용(`Alt+1`), 🪟 분할 보기(`Alt+2`), 👁️ 미리보기 전용(`Alt+3`) 모드를 손쉽게 전환할 수 있습니다.
- **문서 목차 (TOC) 사이드 드로어**: 문서 내 헤딩(H1, H2, H3) 구조를 실시간 스캔하여 사이드바에 표시하며, 클릭 시 해당 섹션으로 부드럽게 스크롤 이동합니다.
- **에디터-미리보기 스크롤 동기화**: 양방향 비율 기반 스크롤 연동으로 긴 문서도 매끄럽게 작성할 수 있습니다.
- **다중 라인 들여쓰기 & 내어쓰기**: `Tab` / `Shift + Tab`으로 여러 줄 블록 전체를 간편하게 2칸 단위로 들여쓰기/내어쓰기합니다.
- **실시간 문서 통계**: 글자 수, 단어 수, 줄 수 및 예상 읽기 시간(분)을 실시간으로 계산하여 표시합니다.
- **외부 파일 드래그 앤 드롭**: 외부 `.md` 파일을 창으로 끌어다 놓아 바로 편집할 수 있습니다.

### 🖨️ 공유 & 내보내기 (Export)
- **🖨️ PDF / 인쇄 모드**: `@media print` 최적화 스타일링이 적용되어 브라우저 인쇄 기능을 통해 헤더와 에디터를 숨기고 깔끔한 인쇄물이나 PDF로 즉시 저장할 수 있습니다.
- **💾 독립 실행형 HTML 파일 다운로드**: 스타일 시트와 KaTeX/Highlight.js가 포함된 완결형 `.html` 파일로 내보낼 수 있습니다.
- **📋 서식 있는 HTML 클립보드 복사**: 원클릭으로 렌더링된 서식 있는 HTML을 복사하여 Google Docs, Word, Gmail, Slack, Notion에 그대로 붙여넣을 수 있습니다.

### 📂 안전한 파일 관리 & 인스턴스 중복 방지
- **원자적 저장 (Atomic Save)**: 임시 파일(`.tmp`)에 완전히 작성한 후 원자적 이름 교체를 수행하므로, 저장 도중 전원이 꺼지거나 크래시가 발생해도 원본 파일이 절대 손상되지 않습니다.
- **파일 관리 기능**: UI 상에서 간편하게 새 파일 만들기(`Ctrl+N`), 저장(`Ctrl+S`), 이름 변경(Rename), 안전 삭제(Delete)를 수행할 수 있습니다.
- **스마트 포트 바인딩 & 단일 인스턴스 활성화**: 이미 같은 폴더를 서비스 중인 mdeditor가 실행 중이면 중복 서버를 띄우지 않고 기존 브라우저 탭을 활성화합니다.
- **깔끔한 종료 (Exit)**: 상단 종료(`⏻`) 버튼을 클릭하여 백엔드 서버를 즉시 안전하게 종료할 수 있습니다.

### 🛡️ 보안 설계 (Security & Hardening)
- **디렉토리 탈출(Path Traversal) 원천 차단**: 상위 디렉토리 이동(`..`), 루트, 절대 경로 접근을 방어하며 작업 디렉토리 내부의 검증된 파일만 안전하게 접근합니다.
- **로컬 애셋 안전 격리**: `/api/raw` 엔드포인트를 통해 허용된 미디어 확장자(`png, jpg, svg, webp, gif, pdf` 등)만 프로젝트 루트 하위에서 서빙합니다.
- **Windows 특수 장치명 & ADS 방어**: `CON`, `PRN`, `AUX`, `NUL`, `COM1-9`, `LPT1-9` 및 대체 데이터 스트림(`file.md:stream`), 파일명 금지 문자(`< > : " | ? *`) 차단.
- **DNS Rebinding & CSRF 방어**: `Host` 및 `Origin` 헤더를 엄격히 검증하여 로컬 주소(`127.0.0.1`, `localhost`)에서 발생한 정상 요청만 처리합니다.
- **XSS 방어 & 스킴 화이트리스트**: 마크다운 렌더링 시 모든 HTML 엔티티를 이스케이프하며, 링크와 이미지 스킴은 `http://`, `https://`, `mailto:`, `#`만 허용합니다 (`javascript:`, `data:` 차단).

---

## 🏗️ 시스템 아키텍처

```
┌─────────────────────────────────────────────────────────────────┐
│                       mdeditor.exe                              │
│                                                                 │
│   ┌─────────────────────────────────────────────────────────┐   │
│   │ src/main.rs (초경량 HTTP 서버 & 보안 라우팅)             │   │
│   │  - std::net::TcpListener (표준 라이브러리 멀티스레드)   │   │
│   │  - safe_path & safe_asset_path (경로 탈출 / ADS 방어)   │   │
│   │  - save_atomic (임시 파일 기반 원자적 저장)              │   │
│   │  - 로컬 이미지 / 애셋 서빙 (/api/raw)                   │   │
│   │  - 파일 이름 변경 (/api/rename) & 삭제 (/api/delete)    │   │
│   │  - DNS Rebinding / CSRF 방어 & 프로세스 생명주기 관리   │   │
│   └──────────────────────────┬──────────────────────────────┘   │
│                              │                                  │
│            ┌─────────────────┴─────────────────┐                │
│            ▼                                   ▼                │
│   ┌───────────────────────────┐    ┌────────────────────────┐   │
│   │ src/markdown.rs           │    │ src/index.html (UI)    │   │
│   │  - 제로 의존성 순수 파서   │    │  - 에디터 서식 툴바    │   │
│   │  - 콜아웃 / 수식 / Mermaid│    │  - 드래그 리사이저      │   │
│   │  - 토글 / 표 / 각주 / 키캡 │    │  - 목차 (TOC) 드로어   │   │
│   │  - 로컬 이미지 자동 매핑  │    │  - 뷰 모드 전환 (3종)  │   │
│   │  - 슬러그 ID 앵커 생성    │    │  - PDF/HTML 내보내기   │   │
│   └───────────────────────────┘    └────────────────────────┘   │
└──────────────────────────────┬──────────────────────────────────┘
                               │ 시스템 기본 브라우저 자동 호출
                               ▼
┌─────────────────────────────────────────────────────────────────┐
│              Browser Desktop Interface (127.0.0.1:8080)         │
└─────────────────────────────────────────────────────────────────┘
```

---

## ⌨️ 단축키 안내

| 단축키 | 기능 | 설명 |
|---|---|---|
| <kbd>Ctrl</kbd> + <kbd>S</kbd> | **파일 저장** | 현재 열려 있는 파일 원자적 저장 (새 파일이면 이름 확인) |
| <kbd>Ctrl</kbd> + <kbd>N</kbd> | **새 파일 생성** | 새로운 `.md` 파일을 생성하고 에디터 초기화 |
| <kbd>Ctrl</kbd> + <kbd>B</kbd> | **굵게 서식** | 선택 영역에 `**굵게**` 서식 적용 |
| <kbd>Ctrl</kbd> + <kbd>I</kbd> | **기울임 서식** | 선택 영역에 `*기울임*` 서식 적용 |
| <kbd>Ctrl</kbd> + <kbd>K</kbd> | **하이퍼링크** | 선택 영역에 `[텍스트](url)` 링크 서식 적용 |
| <kbd>Alt</kbd> + <kbd>1</kbd> | **에디터 모드** | 에디터 전용 전체 화면 보기 |
| <kbd>Alt</kbd> + <kbd>2</kbd> | **분할 보기** | 에디터와 미리보기 나란히 보기 |
| <kbd>Alt</kbd> + <kbd>3</kbd> | **미리보기 모드** | 미리보기 전용 전체 화면 보기 (독서/발표 모드) |
| <kbd>Tab</kbd> | **들여쓰기** | 커서 위치 또는 선택한 다중 라인 블록 전체 2칸 들여쓰기 |
| <kbd>Shift</kbd> + <kbd>Tab</kbd> | **내어쓰기** | 커서 앞 또는 선택한 다중 라인 블록 전체 2칸 내어쓰기 |
| <kbd>Enter</kbd> | **스마트 목록** | 목록 항목에서 Enter 시 자동 번호/불릿 부여 및 빈 줄 시 종료 |
| **구분선 더블 클릭** | **분할 초기화** | 좌우 창 너비 비율을 50:50으로 초기화 |

---

## 📦 지원하는 Markdown 문법 (Notion 수준의 고도화)

| 문법 | 예시 | 설명 |
|---|---|---|
| **콜아웃 블록 (Callouts)** | `> [!NOTE]`, `> [!TIP]`, `> [!WARNING]`, `> 💡` | 노션 & GitHub Alert 스타일의 아이콘 박스 렌더링 |
| **Mermaid 다이어그램** | ` ```mermaid ... ``` ` | 순서도, 시퀀스 다이어그램, 클래스 다이어그램 등 렌더링 |
| **토글 / 접기 (Toggle)** | `:::toggle 제목` ... `:::` | 클릭하여 열고 닫을 수 있는 접기 블록 |
| **LaTeX 수식 (Math)** | `$E = mc^2$`, `$$e^{i\pi}+1=0$$` | KaTeX 기반 고품질 인라인 및 블록 수식 |
| **로컬 및 원격 이미지** | `![대체](diagram.png)`, `![웹](https://...)` | 로컬 이미지 자동 서빙 및 웹 이미지 지원 |
| **형광펜 하이라이트** | `==노란색 형광펜==` | 노션 감성의 부드러운 텍스트 하이라이트 |
| **키보드 키캡** | `[[Ctrl]]` 또는 `<kbd>Ctrl</kbd>` | 키보드 단축키 뱃지 디자인 |
| **윗첨자 & 각주** | `x^2^` 및 `문장[^1]` + `[^1]: 내용` | 과학 수식 윗첨자 및 문서 하단 각주 자동 정리 |
| **코드 블록 & 복사** | ` ```rust ... ``` ` | 상단 언어 뱃지 및 원클릭 클립보드 복사 버튼 |
| **인터랙티브 태스크 목록** | `- [ ] 할 일`, `- [x] 완료` | 미리보기에서 체크박스 클릭 시 원본 에디터 실시간 동기화 |
| **제목 (Headings)** | `# H1` 부터 `###### H6` 까지 | 슬러그 ID 앵커가 자동 생성되는 노션 스타일 헤딩 |
| **강조 (Emphasis)** | `**굵게**`, `*기울임*`, `***굵은 기울임***` | 다양한 인라인 서식 |
| **취소선 (Strikethrough)** | `~~취소선~~` | 텍스트 가로줄 |
| **목록 (Lists)** | 순서 없는 목록(`-`, `*`, `+`), 순서 있는 목록(`1.`) | 다단계 들여쓰기 중첩 지원 |
| **인용 (Blockquotes)** | `> 인용문` | 다중 라인 병합, 중첩 인용 지원 |
| **표 (Tables)** | `\| 제목 \| 내용 \|` | 정렬(`:---`, `:---:`, `---:`) 및 열 자동 패딩 지원 |
| **구분선 (Horizontal Rule)** | `---`, `***`, `___` | 세련된 1px 분할선 |
| **이스케이프** | `\*`, `\[`, `\|`, `\$`, `\^` 등 | 특수문자 리터럴 출력 지원 |

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

실행 즉시 단독 앱 윈도우가 브라우저에 표시됩니다. (기본 주소: `http://127.0.0.1:8080`)

### 3. Windows 설치 및 `.md` 기본 연결 (`install.bat`)
Windows 환경에서 mdeditor를 정식 설치하고 `.md` 파일을 더블클릭할 때마다 mdeditor로 열리도록 기본 앱으로 등록할 수 있습니다:
- **`install.bat` 더블 클릭**:
  - `%LOCALAPPDATA%\Programs\mdeditor`에 최적화된 실행 파일 자동 설치
  - Windows 레지스트리에 `.md`, `.markdown` 기본 연결 및 우클릭 "mdeditor로 편집" 메뉴 등록
  - 시작 메뉴 및 바탕화면 바로가기 등록, 사용자 `PATH`에 등록되어 어디서나 `mdeditor` 실행 가능
  - 관리자 권한(UAC) 불필요, SmartScreen 보안 경고 없음
- **설치 제거**: `uninstall.bat`을 실행하면 레지스트리 연결, 바로가기, 설치 파일이 깔끔하게 원상 복구됩니다.

### 4. Windows 편의 실행 (`mdeditor.bat`)
Windows 환경에서는 포함된 [mdeditor.bat](file:///C:/Users/kornah/Work/09_Personal_Project\mdeditor\mdeditor.bat)를 활용할 수 있습니다:
- **더블 클릭**: 현재 폴더를 작업 디렉토리로 하여 즉시 앱 창을 엽니다. (최초 1회 자동 빌드 수행)
- **파일 끌어다 놓기(Drag & Drop)**: 탐색기에서 원하는 `.md` 파일을 `mdeditor.bat` 아이콘 위로 끌어다 놓으면 해당 파일이 바로 열립니다.

### 5. CLI 명령 인자

```bash
# 현재 디렉토리에서 실행
mdeditor

# 특정 파일을 지정하여 열기 (해당 파일의 상위 폴더가 작업 디렉토리가 됨)
mdeditor C:\path\to\document.md

# 브라우저 창을 자동으로 띄우지 않고 서버만 실행 (헤드리스/서버 모드)
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
| `GET` | `/api/raw` | `?file=경로/이미지.png` | 작업 폴더 내 로컬 이미지/미디어 안전 스트리밍 |
| `POST` | `/api/render` | 요청 본문(Markdown) | 변환된 HTML 반환 (`{"html":"..."}`) |
| `POST` | `/api/save` | `?file=파일명.md` (본문: 내용) | 원자적 파일 저장 (`{"ok":true}`) |
| `POST` | `/api/rename` | `?from=구이름.md&to=새이름.md` | 파일 이름 안전 변경 (`{"ok":true}`) |
| `POST` | `/api/delete` | `?file=파일명.md` | 대상 파일 안전 영구 삭제 (`{"ok":true}`) |
| `POST` | `/api/exit` | - | 백엔드 프로세스 안전 종료 (F5 새로고침 방어 2초 유예) |

---

## 🧪 테스트 실행

마크다운 파싱, 각주, Mermaid, 테이블 분리, 슬러그 생성, 보안 검증, 애셋 탈출 차단, 원자적 파일 I/O 등 **36개의 단위 테스트**가 작성되어 있습니다.

```bash
cargo test
```

```text
running 36 tests
test markdown::tests::backslash_escapes ... ok
test markdown::tests::blockquote_merges_and_nests ... ok
test markdown::tests::callouts_render ... ok
test markdown::tests::code_block_is_escaped_and_unclosed_fence_ok ... ok
test markdown::tests::footnotes_work ... ok
test markdown::tests::mermaid_diagram_rendering ... ok
test markdown::tests::local_relative_images_mapped_to_api_raw ... ok
test markdown::tests::table_stops_on_heading_or_quote_with_pipe ... ok
test markdown::tests::table_pads_missing_columns ... ok
test tests::safe_asset_path_security ... ok
test tests::atomic_save_roundtrip ... ok
...
test result: ok. 36 passed; 0 failed; 0 ignored; finished in 0.05s
```

---

## 📄 라이선스

이 프로젝트는 [MIT License](LICENSE)에 따라 자유롭게 사용, 수정, 배포할 수 있습니다.
