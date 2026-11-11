# 📝 MDEditor

**Rust로 만든 순수 로컬 Markdown 편집기 — 브라우저·웹뷰·로컬 서버 없이 네이티브 창 하나로 동작합니다.**

![Rust 2021](https://img.shields.io/badge/Rust-2021-DEA584?style=flat-square&logo=rust&logoColor=white) ![Native](https://img.shields.io/badge/UI-eframe%2Fegui-blue?style=flat-square) ![Binary Size](https://img.shields.io/badge/Binary_Size-~9MB-blue?style=flat-square) ![Platform](https://img.shields.io/badge/Platform-Windows-lightgrey?style=flat-square) ![Tests](https://img.shields.io/badge/Tests-19%20Passed-brightgreen?style=flat-square) ![License](https://img.shields.io/badge/License-MIT-orange?style=flat-square)

---

## 💡 소개

MDEditor는 `eframe`/`egui`(OpenGL 렌더러) 기반의 네이티브 데스크톱 앱입니다. 화면 구성과 디자인은 이전 웹 버전(노션 스타일 라이브 캔버스)을 그대로 옮겼습니다.

- **웹 기술 없음**: HTTP 서버, 포트, Edge/Chrome 앱 창, CDN이 전혀 없습니다. 완전 오프라인으로 동작합니다.
- **단일 실행 파일**: 설치 없이 실행할 수 있고, 같은 파일이 Windows 설치 마법사도 겸합니다.
- **로컬 파일 그대로**: 작업 폴더의 `.md` 파일을 직접 읽고 씁니다. 원자적 저장으로 원본이 손상되지 않고, 줄바꿈(CRLF)과 BOM도 원래대로 보존합니다.

---

## ✨ 기능

### 노션 스타일 라이브 캔버스
- 문서가 렌더링된 상태로 보이고, **블록을 클릭하면 그 자리에서 편집**됩니다 (편집 중인 블록만 원문 표시, 제목은 크기·굵기 유지).
- `Enter`로 블록 나누기, 블록 맨 앞 `Backspace`로 앞 블록과 합치기, 첫 줄/마지막 줄에서 `↑`/`↓`로 블록 이동.
- **슬래시(`/`) 메뉴**: 빈 블록에서 `/`를 입력하면 텍스트·제목·할 일·목록·인용·구분선·콜아웃·코드·표·토글·수식·Mermaid·이미지·각주 블록을 삽입합니다.
- **떠 있는 서식 막대**: 글자를 선택하면 굵게·기울임·취소선·형광펜·코드·키캡·링크·서식 지우기 막대가 나타납니다.
- **스마트 목록**: 목록에서 `Enter`를 누르면 다음 기호·번호를 이어 쓰고, 빈 항목에서 `Enter`를 누르면 목록이 끝납니다. `Tab`/`Shift+Tab` 들여쓰기.
- 렌더링된 **할 일 체크박스를 클릭하면 원문이 바로 바뀝니다.**
- 마지막 블록 아래 빈 곳을 클릭하면 새 블록이 생깁니다.
- **문서 단위 실행 취소** (`Ctrl+Z` / `Ctrl+Y`): 글자 입력뿐 아니라 블록 나누기·합치기, 서식, 슬래시 메뉴, 모두 바꾸기, 체크박스까지 되돌립니다. 이어 친 입력은 한 단계로 묶입니다.
- 서식 단축키는 **토글**입니다: 굵은 글자에서 `Ctrl+B`를 다시 누르면 해제됩니다. `Ctrl+K`는 링크를 만든 뒤 주소 자리를 선택해 둡니다.

### 화면 구성 (웹 버전과 동일)
- **상단 바**: 로고, 파일 이름 배지(클릭하면 이름 변경, 저장 안 됨 표시 점), 파일 탐색기·열기·새 파일·저장, 목차·찾기·이미지, 인쇄·HTML 내보내기, 설정.
- **작업 폴더 파일 탐색기** (`Ctrl+O`): 필터, `↑↓`/`Enter` 이동·열기, 파일 삭제, 새 파일, 폴더 변경.
- **목차 서랍**: 오른쪽에서 열리는 제목 목록. 현재 위치 강조, 클릭하면 해당 제목으로 이동.
- **찾기·바꾸기 막대** (`Ctrl+F`/`Ctrl+H`): 오른쪽 위에 떠 있는 막대, 대소문자 구분, 일치 개수. 선택한 글자가 검색어로 들어가고 `F3`/`Shift+F3`으로 이동합니다.
- **시작 화면**: 열린 문서가 없으면 새 문서·폴더 열기·파일 열기 버튼을 보여 줍니다. 시작 메뉴에서 처음 실행하면 `문서\MDEditor`가 기본 작업 폴더입니다.
- **상태 알약**: 저장됨/저장되지 않음 표시와 글자·줄·읽기 시간.
- **설정 메뉴**: 라이트/다크 테마, 전체 너비, 글꼴 크기, 마크다운 원본/HTML 복사, 도움말, 종료.
- 웹과 같은 색상 토큰(Notion 팔레트)과 글꼴(KoPubWorld 돋움 → Segoe UI → 맑은 고딕, 코드는 Consolas), Lucide 아이콘.

### 이미지
- **스크린샷 붙여넣기** (`Ctrl+V`): 클립보드 이미지를 `문서 폴더/images/paste-<시각>.png`로 저장하고 링크를 삽입합니다.
- **이미지 삽입 창** (`Ctrl+Shift+I`): 파일 업로드(복사 후 상대 경로 링크) 또는 웹 링크(URL).
- 창에 이미지 파일을 끌어다 놓아도 삽입됩니다. 캔버스에서 로컬 이미지(png, jpg, gif, webp, bmp)를 바로 표시합니다.

### Markdown
- CommonMark + GFM: 제목, 강조, 표, 할 일 목록, 각주, 코드 구문 강조, GitHub 알림(`> [!NOTE]`, `[!TIP]`, `[!WARNING]` 등).
- 확장 문법: `==형광펜==`(굵게로 표시), `[[Ctrl]]` 키캡, `:::toggle 제목 … :::` 접고 펴는 토글 카드, `$$ … $$` 수식 카드·Mermaid 카드(원문 표시).
- 문서 안의 `.md` 링크를 클릭하면 앱 안에서 열고, 웹 링크는 기본 브라우저로 엽니다.

### 저장과 안전장치
- 입력을 멈추고 2초 뒤 조용히 자동 저장(상태 알약으로 표시, 실패할 때만 알림), 창을 닫을 때 저장되지 않은 문서가 있으면 확인 창.
- 다른 프로그램이 열려 있는 파일을 바꾸면 창으로 돌아올 때 다시 불러옵니다. 편집 중이던 문서는 덮어쓰지 않고 경고만 합니다.
- 새 문서와 이름 변경은 작업 폴더 밖으로 나가는 경로(`..`, 절대 경로, 드라이브 지정)와 Windows 예약 이름(`CON`, `NUL`, `COM1` 등) 및 금지 문자를 거부합니다.
- UTF-8이 아닌 파일은 빈 문서로 위장하지 않고 열기 오류를 표시합니다.
- 마지막 작업 폴더·문서·테마·전체 너비·글꼴 크기·창 크기를 기억합니다.

---

## ⌨️ 단축키

| 단축키 | 기능 |
|---|---|
| `Ctrl+S` | 저장 |
| `Ctrl+N` | 새 파일 |
| `Ctrl+O` | 작업 폴더 파일 탐색기 |
| `Ctrl+Shift+O` | 작업 폴더 바꾸기 |
| `Ctrl+Z` / `Ctrl+Y` | 실행 취소 / 다시 실행 |
| `Ctrl+F` / `Ctrl+H` | 찾기 / 바꾸기 (`Enter`·`F3` 다음, `Shift+Enter`·`Shift+F3` 이전) |
| `Ctrl+B` / `Ctrl+I` / `Ctrl+K` | 굵게 / 기울임 / 링크 (다시 누르면 해제) |
| `Ctrl+Shift+I` | 이미지 삽입 |
| `Ctrl+V` (이미지) | 스크린샷 저장 후 삽입 |
| `Ctrl+P` | 인쇄 (PDF로 저장 가능) |
| `Ctrl+Q` | 종료 |
| `/` | 블록 삽입 메뉴 |
| `Enter` / `Backspace` | 블록 나누기 / 앞 블록과 합치기 |
| `Tab` / `Shift+Tab` | 들여쓰기 / 내어쓰기 |
| `Esc` | 편집 끝내기, 찾기 막대·창 닫기 |
| `F1` / `F2` | 도움말 / 이름 변경 |

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

`package.bat`을 실행하면 `dist/`에 배포 파일이 만들어집니다 (테스트 → 릴리즈 빌드 → 설치 파일 → 포터블 zip → SHA256).

- **`MDEditor-Setup-v<버전>.exe`** — 더블클릭하면 앱과 같은 디자인의 설치 마법사가 열립니다.
  - 선택 항목: `.md` 파일 연결, 탐색기 우클릭 메뉴, 바탕화면 바로가기, 터미널 명령어(PATH).
  - `%LOCALAPPDATA%\Programs\mdeditor`에 설치하며 **관리자 권한이 필요 없습니다.** 이미 설치돼 있으면 업데이트로 동작하고, 실행 중인 MDEditor는 문서를 저장하고 닫은 뒤 진행합니다.
  - 무인 설치: `MDEditor-Setup-v<버전>.exe /S`
- **제거**: Windows 설정 > 앱 > 설치된 앱 > MDEditor (또는 `mdeditor --uninstall`). 문서는 지우지 않고, 앱 설정 삭제는 선택입니다.
- **포터블 zip** — 압축을 풀어 `mdeditor.exe`를 바로 실행합니다. 나중에 `mdeditor --install`로 설치할 수도 있습니다.

설치 파일은 `mdeditor.exe`와 같은 바이너리입니다. 파일 이름에 `Setup`이 들어 있으면 설치 마법사로, 아니면 편집기로 실행됩니다.

---

## 🏗️ 구조

```
src/main.rs        파일 I/O, 편집 연산, 블록 분할, 앱 상태·단축키, HTML 내보내기
src/ui.rs          화면: 상단 바, 노션 캔버스, 목차 서랍, 찾기 막대, 상태바, 모달
src/theme.rs       웹 버전 디자인 토큰(색상), 글꼴, Lucide 아이콘
src/setup.rs       설치·제거 마법사 화면 (같은 실행 파일이 설치 프로그램을 겸함)
src/install.ps1    설치 엔진: 파일 복사, 파일 연결, 바로가기, 설치된 앱 등록 (실행 파일에 내장)
src/uninstall.ps1  제거 엔진
build.rs           아이콘·버전 정보 리소스 (버전은 Cargo.toml 한 곳에서 관리)
package.ps1        배포 빌드: 테스트 → 릴리즈 → 설치 파일 → zip → SHA256
```

| 의존성 | 용도 |
|---|---|
| `eframe` (glow) | 네이티브 창, OpenGL 렌더링, 클립보드, 설정 저장 |
| `egui_commonmark` | 블록 렌더링 (pulldown-cmark) |
| `pulldown-cmark` | HTML 내보내기 (캔버스와 같은 파서) |
| `egui_extras` + `image` | 로컬 이미지·SVG 아이콘 로딩, 앱 아이콘, PNG 인코딩 |
| `rfd` | 운영체제 기본 폴더/파일 대화상자 |

---

## 🧪 테스트

```bash
cargo test                                    # 19개
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

편집 연산(감싸기·토글, 들여쓰기, 스마트 Enter), 실행 취소, 찾기/바꾸기, 목차, 블록 분할, 전처리와 체크박스 반영, 이름 검증, 원자적 저장과 CRLF/BOM 보존을 다루고, `egui_kittest` 헤드리스 UI 테스트로 실제 앱에 클릭·키 입력을 보내 블록 편집 흐름(클릭 → 입력 → Enter → `/` 메뉴 → 체크박스 → Backspace 합치기 → Ctrl+Z)을 검증합니다. 설치 엔진은 숨김 PowerShell 실행과 진행률·오류 전달을 실제로 돌려 확인합니다.

---

## ⚠️ 알려진 한계

- 편집 중인 블록은 Markdown 원문으로 보입니다 (노션처럼 서식이 적용된 채 글자 단위로 편집하지는 않음).
- **수식(KaTeX)·Mermaid 다이어그램은 렌더링하지 않고** 원문 카드로 표시합니다 (브라우저 엔진이 필요한 기능).
- 원격(http) 이미지는 표시하지 않습니다 (오프라인 우선). 로컬 이미지만 지원합니다.
- 인쇄(`Ctrl+P`)는 임시 HTML을 기본 브라우저로 열어 인쇄 대화상자를 띄웁니다.
- 설치 파일에 **코드 서명이 없어** 처음 실행할 때 Windows SmartScreen 경고가 뜹니다. 판매·배포하려면 코드 서명 인증서로 서명해야 합니다.

---

## 📄 라이선스

[MIT License](LICENSE)
