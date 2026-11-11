# 📝 MDEditor

**Rust로 만든 순수 로컬 Markdown 편집기 — 브라우저·웹뷰·로컬 서버 없이 네이티브 창 하나로 동작합니다.**

![Rust 2021](https://img.shields.io/badge/Rust-2021-DEA584?style=flat-square&logo=rust&logoColor=white) ![Native](https://img.shields.io/badge/UI-eframe%2Fegui-blue?style=flat-square) ![Binary Size](https://img.shields.io/badge/Binary_Size-~7MB-blue?style=flat-square) ![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey?style=flat-square) ![Tests](https://img.shields.io/badge/Tests-51%20Passed-brightgreen?style=flat-square) ![License](https://img.shields.io/badge/License-MIT-orange?style=flat-square)

---

## 💡 소개

MDEditor는 `eframe`/`egui`(OpenGL 렌더러) 기반의 네이티브 데스크톱 앱입니다.

- **웹 기술 없음**: HTTP 서버, 포트, Edge/Chrome 앱 창, CDN이 전혀 없습니다. 완전 오프라인으로 동작합니다.
- **단일 실행 파일 (~7MB)**: 설치 없이 실행할 수 있고, Windows 설치 프로그램도 제공합니다.
- **로컬 파일 그대로**: 작업 폴더의 `.md` 파일을 직접 읽고 씁니다. 원자적 저장으로 원본이 손상되지 않고, 줄바꿈(CRLF)과 BOM도 원래대로 보존합니다.

---

## ✨ 기능

### 편집
- **편집 / 분할 / 미리보기** 3가지 보기 (`Alt+1/2/3`). 분할 보기에서는 입력하는 즉시 미리보기가 갱신됩니다.
- **서식 도구 모음 (20종)**: 제목 1~3, 굵게, 기울임, 취소선, 형광펜, 인라인 코드, 글머리·번호·할 일 목록, 인용, 링크, 이미지, 표, 코드 블록, 수식, 콜아웃, 토글, 구분선.
- **스마트 목록**: 목록에서 `Enter`를 누르면 다음 기호·번호를 자동으로 이어 쓰고, 빈 항목에서 `Enter`를 누르면 목록이 끝납니다.
- **들여쓰기**: `Tab` / `Shift+Tab`으로 여러 줄을 한 번에 2칸씩 들여쓰거나 내어씁니다.
- **찾기 및 바꾸기** (`Ctrl+F` / `Ctrl+H`): 대소문자 구분, 일치 개수 표시, 한 개씩 또는 모두 바꾸기.
- **자동 저장**: 입력을 멈추고 2초 뒤 저장합니다 (⚙ 메뉴에서 끌 수 있음). 창을 닫을 때 저장되지 않은 문서가 있으면 확인 창이 뜹니다.

### 이미지
- **스크린샷 붙여넣기** (`Ctrl+V`): 클립보드 이미지를 `문서 폴더/images/paste-<시각>.png`로 저장하고 링크를 삽입합니다.
- **끌어다 놓기 / 🖼 버튼**: 이미지 파일을 `images/`로 복사하고 링크를 삽입합니다 (문서 폴더 안의 파일이면 복사하지 않고 상대 경로로 링크).
- 미리보기에서 로컬 이미지(png, jpg, gif, webp, bmp)를 바로 표시합니다.

### 미리보기
- CommonMark + GFM: 제목, 강조, 표, 할 일 목록, 각주, 코드 구문 강조, GitHub 알림(`> [!NOTE]`, `[!TIP]`, `[!WARNING]` 등).
- **할 일 체크박스를 클릭하면 원문이 바로 바뀝니다.**
- 확장 문법: `==형광펜==`(굵게로 표시), `[[Ctrl]]` 키캡, `:::toggle 제목 … :::` 토글(제목으로 표시), `$수식$`(소스 코드로 표시).
- 문서 안의 `.md` 링크를 클릭하면 앱 안에서 열고, 웹 링크는 기본 브라우저로 엽니다.

### 파일과 폴더
- 사이드바 **파일** 탭: 작업 폴더의 `.md` 파일을 하위 폴더까지 표시합니다 (`Ctrl+O`로 검색, `Enter`로 열기). 우클릭하면 이름 변경·삭제 메뉴가 나옵니다.
- 사이드바 **목차** 탭: 문서의 제목 구조를 보여 주고, 클릭하면 해당 위치로 이동합니다.
- 새 문서(`Ctrl+N`, `notes/할일.md`처럼 하위 폴더 지정 가능), 이름 변경(`F2`), 폴더 열기(`Ctrl+Shift+O`).
- `.md` 파일이나 폴더를 창에 끌어다 놓으면 바로 엽니다.
- **HTML 내보내기**: 스타일이 포함된 단일 `.html` 파일로 저장합니다.
- 마지막 작업 폴더·문서·보기·창 크기·테마(라이트/다크/시스템)를 기억합니다.

### 안전장치
- 새 문서와 이름 변경은 작업 폴더 밖으로 나가는 경로(`..`, 절대 경로, 드라이브 지정)와 Windows 예약 이름(`CON`, `NUL`, `COM1` 등) 및 금지 문자를 거부합니다.
- UTF-8이 아닌 파일은 빈 문서로 위장하지 않고 열기 오류를 표시합니다.

---

## ⌨️ 단축키

| 단축키 | 기능 |
|---|---|
| `Ctrl+S` | 저장 |
| `Ctrl+N` | 새 문서 |
| `Ctrl+O` | 파일 검색 |
| `Ctrl+Shift+O` | 폴더 열기 |
| `Ctrl+F` / `Ctrl+H` | 찾기 / 바꾸기 (`Enter` 다음, `Shift+Enter` 이전) |
| `Ctrl+B` / `Ctrl+I` / `Ctrl+K` | 굵게 / 기울임 / 링크 |
| `Tab` / `Shift+Tab` | 들여쓰기 / 내어쓰기 |
| `Ctrl+V` (이미지) | 스크린샷 저장 후 삽입 |
| `Alt+1` / `Alt+2` / `Alt+3` | 편집 / 분할 / 미리보기 |
| `F2` | 이름 변경 |
| `F1` | 도움말 |
| `Esc` | 찾기 막대 닫기 |

---

## 🚀 빌드 및 실행

요구 사항: [Rust 툴체인](https://rustup.rs/) (stable 1.95 이상)

```bash
git clone https://github.com/Hannoah/mdeditor.git
cd mdeditor
cargo run --release                 # 현재 폴더(또는 마지막 작업 폴더)로 실행
cargo run --release -- C:\docs      # 폴더 열기
cargo run --release -- memo.md      # 파일 열기 (상위 폴더가 작업 폴더)
```

### Windows 설치

- **단일 설치 파일**: `package.bat` 실행 → `dist/MDEditor-Setup.exe` 생성 (테스트 → 릴리즈 빌드 → 설치 파일 → 포터블 zip → SHA256).
- **소스에서 바로 설치**: `install.bat` 실행.
  - `%LOCALAPPDATA%\Programs\mdeditor`에 설치하고, `.md`/`.markdown` 연결, 우클릭 "MDEditor로 이 폴더 열기", 시작 메뉴·바탕화면 바로가기, 사용자 `PATH`, "설치된 앱" 목록 등록까지 처리합니다.
  - 관리자 권한이 필요 없습니다.
- **제거**: `uninstall.bat` 또는 Windows 설정 > 앱 > 설치된 앱.

---

## 🏗️ 구조

```
src/main.rs        네이티브 앱 (UI, 편집 연산, 파일 I/O, 미리보기 연결)
src/markdown.rs    Markdown → HTML 변환기 (HTML 내보내기에 사용)
installer/         std 전용 단일 파일 설치 프로그램 (mdeditor.exe 내장)
```

| 의존성 | 용도 |
|---|---|
| `eframe` (glow) | 네이티브 창, OpenGL 렌더링, 클립보드, 설정 저장 |
| `egui_commonmark` | 미리보기 렌더링 (pulldown-cmark) |
| `egui_extras` + `image` | 로컬 이미지 로딩, 앱 아이콘, PNG 인코딩 |
| `rfd` | 운영체제 기본 폴더/파일 대화상자 |

---

## 🧪 테스트

```bash
cargo test                                    # 51개
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

네이티브 앱 테스트는 편집 연산(감싸기, 줄머리 토글, 들여쓰기, 스마트 Enter), 찾기/바꾸기, 목차, 미리보기 전처리와 체크박스 반영, 이름 검증, 원자적 저장과 CRLF/BOM 보존을 다룹니다.

---

## ⚠️ 알려진 한계

- **노션식 라이브 캔버스(WYSIWYG)는 없습니다.** 네이티브 UI에서는 원문 편집 + 실시간 미리보기(분할 보기)로 대체했습니다.
- **수식(KaTeX)·Mermaid 다이어그램은 렌더링하지 않고** 소스 코드로 표시합니다 (브라우저 엔진이 필요한 기능).
- 원격(http) 이미지는 표시하지 않습니다 (오프라인 우선). 로컬 이미지만 지원합니다.
- PDF 인쇄 대신 HTML로 내보낸 뒤 인쇄하세요.

---

## 📄 라이선스

[MIT License](LICENSE)
