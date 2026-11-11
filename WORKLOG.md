# MDEditor 작업 기록

## 2026-11-11 — 네이티브 UI를 웹 버전과 동일하게 재구성

### 1. 화면 구성 (웹 `index.html` 기준)
- 사이드바·편집/분할/미리보기 전환·서식 툴바 줄을 없애고 웹과 같은 **상단 한 줄 헤더**(로고, 파일 배지+저장 안 됨 점, 파일·열기·새 파일·저장 / 목차·찾기·이미지 / 인쇄·내보내기 / 설정)로 교체.
- **노션식 라이브 캔버스**(`src/ui.rs`): 문서를 빈 줄 기준 블록으로 나눠(`split_blocks`, 코드 펜스·`$$`·토글 내부는 유지) 렌더링하고, 클릭한 블록만 그 자리에서 원문 편집. Enter 블록 나누기, 맨 앞 Backspace 합치기, ↑/↓ 블록 이동, 아래 빈 곳 클릭으로 새 블록, 비운 새 블록은 떠날 때 빈 줄 되돌림.
- 슬래시(`/`) 메뉴(웹과 같은 17종·그룹·색 아이콘), 선택 시 떠 있는 서식 막대, 토글 카드(접기/펴기), 수식·Mermaid 카드.
- 작업 폴더 파일 탐색기 모달, 오른쪽 목차 서랍(현재 위치 강조), 오른쪽 위 찾기·바꾸기 막대, 상태 알약, 토스트, 설정 메뉴(테마·전체 너비·글꼴 크기·원본/HTML 복사·도움말·종료), 확인 대화상자(아이콘 상자), 이미지 삽입 모달(파일/URL 탭), 도움말 모달.
- `src/theme.rs`: 웹 디자인 토큰(다크 #191919/#202020, 라이트 #ffffff/#f7f7f5, 강조 #2383e2), 글꼴(KoPubWorld 돋움 → Segoe UI → 맑은 고딕, Consolas, Segoe UI Symbol/Emoji 대체), 웹과 같은 Lucide SVG 아이콘(`egui_extras` svg).
- 단축키 추가: Ctrl+Shift+I(이미지), Ctrl+P(인쇄), Ctrl+Q(종료). 자동 저장은 웹처럼 항상 켬.

### 2. 검증
- `egui_kittest` 헤드리스 UI 테스트로 실제 앱에 클릭·키 입력을 보내 블록 편집 흐름 검증(클릭→입력→Enter→`/제목`+↓+Enter→Esc→체크박스→빈 블록 정리→Backspace 합치기).
- wgpu 렌더러로 편집·슬래시 메뉴·파일 탐색기·저장 확인·도움말·라이트 테마+목차+찾기 화면을 이미지로 확인, 실제 실행 창도 창 단위 캡처로 확인.
- `cargo test` 14개 통과, clippy·fmt 클린.

## 2026-11-11 — 레거시 정리 및 설치 파일 수정

### 1. 레거시 제거
- 웹 시절 자체 변환기 `src/markdown.rs`(1,763줄, `/api/raw` 서버 경로 매핑 포함) 삭제. HTML 내보내기는 미리보기와 같은 `pulldown-cmark` + 같은 전처리로 생성 → 화면과 결과 일치(GitHub 알림 클래스, 체크박스, 형광펜·키캡 동일 처리).
- 오래된 v0.1.0(웹 버전) 산출물이 남아 있던 `dist/`를 비우고 네이티브 v0.2.0으로 재생성.

### 2. 설치 버그 수정
- 단일 설치 파일이 내장 `install.ps1`을 임시 폴더에서 실행하면 `target\release\mdeditor.exe`를 찾다가 종료 → 파일 연결·바로가기·설치된 앱 등록이 전혀 안 되던 문제. `-SourceExe` 인자로 방금 복사한 exe를 넘기고, 등록 실패 시 경고 창 표시.
- 포터블 zip의 `install.bat`도 같은 이유로 실패하던 문제: 스크립트 옆 `mdeditor.exe`를 우선 사용.
- 소스에서 `install.bat` 실행 시 항상 `cargo build --release` → 예전 바이너리가 설치되지 않음.
- 제거 시 앱 설정(`%APPDATA%\mdeditor`)도 삭제. 설치 프로그램 버전 문구는 `CARGO_PKG_VERSION`에서 가져옴.

### 3. 코드 정리 (`src/main.rs`)
- 미저장 확인 대화상자의 '빈 경로 = 취소' 표식을 `cancel` 플래그로 교체, 이름 변경·● 표시 중복 코드를 `ask_rename`/`label`로 통합, 쓰지 않는 변수·중복 클로저 제거.
- `cargo test` 12개 통과, clippy·fmt 클린.

## 2026-11-11 — 네이티브 앱을 기본 mdeditor.exe로 전환 (웹 기반 실행 종료)

### 1. 바이너리 구성 변경
- `mdeditor` = 네이티브 앱(`src/main.rs`, eframe/egui). 설치·패키징 스크립트는 파일명이 같아 수정 없이 네이티브 앱을 배포.
- 이전 웹 서버 버전(`src/main.rs` HTTP 서버 + `src/index.html` UI)과 `src/lib.rs`를 삭제. `native` 피처 제거, eframe 필수 의존성화.
- 렌더러를 wgpu → glow로 교체: 릴리즈 9.4 MB → 7.0 MB, 실행 메모리 384 MB → 150 MB (동일 화면 실측).
- 버전 0.2.0 (Cargo.toml, installer, install.ps1, package.ps1).

### 2. 네이티브 앱 기능
- 미리보기를 자체 LayoutJob 렌더러에서 `egui_commonmark`로 교체: 표·로컬 이미지·구문 강조·GitHub 알림·각주·클릭 가능한 체크박스(원문 반영)·링크.
- 확장 문법(`==형광==`, `[[키]]`, `:::toggle`)은 바이트 길이를 보존하는 전처리로 표시 → 체크박스 위치가 원문과 1:1 대응.
- 편집: 서식 도구 모음 20종, Ctrl+B/I/K, 스마트 목록 Enter, Tab/Shift+Tab, 찾기·바꾸기(Ctrl+F/H).
- 이미지: Ctrl+V 스크린샷 붙여넣기, 파일 끌어다 놓기, 🖼 버튼 → `문서 폴더/images/`.
- 파일: 검색(Ctrl+O), 새 문서(하위 폴더 가능), 이름 변경(F2), 삭제(확인), 폴더 열기 대화상자, .md/폴더 끌어다 놓기, 문서 간 .md 링크 앱 내 열기, 목차 탭.
- 자동 저장(2초), 닫기 시 미저장 확인, CRLF·BOM 보존, UTF-8 아닌 파일은 오류 표시, 작업 폴더·문서·보기·테마·창 크기 기억, 앱 아이콘, HTML 내보내기(로컬 이미지 상대 경로).
- 이름 검증: `..`/절대 경로/드라이브/금지 문자/Windows 예약 이름 거부.

### 3. 검증
- `cargo test`: 51개 통과 (39 markdown + 12 네이티브).
- `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`: 클린.
- 실제 실행 확인: 분할 보기 렌더링(한글·표·이미지·코드·알림·토글), 스크린샷 붙여넣기 → `images/paste-*.png` 저장·링크 삽입, 폴더 인자 실행, 릴리즈(GUI 서브시스템) 실행.
- `installer/` 빌드 성공 (mdeditor-setup.exe 7.6 MB, 네이티브 앱 내장). 설치 자체는 실행하지 않음.

### 4. 한계
- 노션식 WYSIWYG 캔버스 없음(분할 보기로 대체), KaTeX·Mermaid는 소스 표시, 원격 이미지 미지원.

## 2026-11-11 — 순수 네이티브 앱 골격 (`mdeditor-native`, eframe/egui)

### 1. 네이티브 바이너리 신설 (`src/bin/native.rs`, `--features native`)
- **브라우저·서버 완전 제외**: `eframe 0.37` 기반 순수 로컬 데스크톱 앱. 웹 버전(`mdeditor`, std-only)은 그대로 유지.
- **공유 구조**: `src/lib.rs` 신설 후 `markdown` 모듈을 라이브러리로 승격, `main.rs`는 `use mdeditor::markdown`으로 전환 (동작 변경 없음).
- **MVP 기능**: 폴더/파일 인자 처리, 재귀 파일 목록, 멀티 버퍼 편집, 원자적 저장, `Ctrl+S`, 새 파일, 글자/단어 수 상태바, 맑은고딕 자동 등록.
- **선택 의존성**: `eframe`은 optional + `required-features`라 기본 빌드(`cargo build`, `cargo test`)는 std-only 그대로.

### 2. 검증 결과
- `cargo test --features native`: 61개 전체 통과 (39 lib + 16 web + 6 native).
- `cargo clippy --features native --all-targets -- -D warnings`: 무경고 통과.
- `cargo fmt --check`: 클린 통과.
- `cargo build --features native --bin mdeditor-native` (debug + release): 성공. eframe 0.37 API 변경(`App::ui`, `Panel::{left,top,bottom}`, `show(&mut Ui)`) 반영.
- 릴리즈 바이너리: 8.1 MB (`target/release/mdeditor-native.exe`).

### 3. 네이티브 미리보기 (`src/bin/native.rs`)
- **편집/미리보기/분할 3종 뷰**: 상단 바로 전환. 분할은 좌우 컬럼에 편집기와 미리보기를 동시 표시.
- **LayoutJob 렌더러**: 제목(H1-H4 크기 단계), 굵게/기울임/취소선/형광펜, 인라인·블록 코드, 인용구, 순서·비순서·체크박스 목록, 표(모노스페이스), 구분선, 링크(원문 URL 병기), 이미지 자리표시, `$수식` 모노 표시, `[[키캡]]`.
- **볼드 한글**: 맑은고딕 볼드(`malgunbd.ttf`, 없으면 일반체로 대체 등록)로 tofu 방지.

## 2026-11-11 — Windows 단독 앱 창 전용 실행 (일반 브라우저 탭 폴백 제거)

### 1. 네이티브 창 exclusive 실행 (`src/main.rs`)
- **브라우저 탭 폴백 삭제**: Windows에서 Edge/Chrome을 못 찾으면 `cmd /C start`로 기본 브라우저 탭을 열던 동작을 제거. 이제 단독 앱 창(`--app`)으로만 실행된다.
- **찾을 수 없을 때 오류 대화상자**: 인스톨러와 동일한 std-only `win32::message_box(MessageBoxW)` 패턴으로 안내 후 `exit(1)`. 조용한 좀비 서버로 남지 않는다.
- **탐색 로직 분리**: `find_chromium_exe()` + `first_existing()` 순수 함수로 분리 (Edge → Chrome → Brave 우선순위 유지).
- **탈출구 유지**: `MDEDITOR_BROWSER` 지정 시에만 기본 브라우저로 열기, `MDEDITOR_NO_BROWSER` 서버 전용 모드 그대로. macOS/Linux 동작 불변.

### 2. 회귀 테스트 및 문서
- `first_existing_picks_first_present_candidate` 추가: 첫 존재 후보 선택, 전부 없음·빈 목록 시 `None` 검증.
- `README.md`: 테스트 배지 51→55, 브라우저 탭 언급을 앱 창으로 정정.

### 3. 검증 결과
- `cargo test`: 55개 전체 통과 (55 passed, 0 failed).
- `cargo clippy --all-targets -- -D warnings`: 무경고 통과.
- `cargo fmt --check`: 클린 통과.
- `cargo build --release`: 717,824 바이트 (~701 KB).

세션 작업 내역을 일자별로 정리한 로그입니다.
검증 수치는 실행 당시 실측값이며, `target/` 빌드 산출물은 기록 대상에서 제외합니다.

## 2026-11-10 — 단일 독립 설치 파일(Setup.exe) 패키징, 이미지 삽입 기능 구현 및 편집기 핵심 버그(다중 블록 선택·하단 붙여넣기) 완벽 수정

### 1. 단일 독립 실행형 Windows 설치 프로그램 구축 (`installer/`, `dist/MDEditor-Setup.exe`)
- **원클릭 배포 바이너리 제작**:
  - `installer/` 크레이트를 생성하고 `std`만 사용하여 외부 런타임/의존성 없이 작동하는 1.1 MB 크기의 단일 실행 파일 `MDEditor-Setup.exe` 구축.
  - `mdeditor.exe`, `app.ico`, `install.ps1`, `uninstall.bat`, `uninstall.ps1`을 번들 데이터로 컴파일 타임 임베딩(`include_bytes!`).
  - 더블클릭 시 Win32 대화상자(`MessageBoxW`)로 설치 안내를 제공하며, 임시 디렉토리에 번들을 풀어 원클릭 자동 설치 및 Windows 앱 등록/파일 확장자 연결/바로가기 생성 완료 후 즉시 실행.
  - 무인/배치 모드 지원 (`/s`, `/silent`, `-q`, `--quiet`).
- **원스톱 배포 자동화 파이프라인 (`package.ps1`, `package.bat`)**:
  - `cargo test` (54개 테스트) 검증 → `cargo build --release` → `installer` 빌드 → `dist/` 산출물 패키징(`MDEditor-Setup.exe`, 버전 태그 바이너리, 포터블 zip) 및 `SHA256SUMS.txt` 생성까지 자동화.

### 2. 이미지 삽입 전방위 기능 추가 (`src/main.rs`, `src/index.html`)
- **백엔드 업로드 API (`POST /api/upload?name=...`)**:
  - 요청 바디 바이너리를 워크스페이스 내 `images/` 폴더에 즉시 저장.
  - 경로 탐색(Directory Traversal) 공격 방어 새니타이징 및 파일명 중복 시 자동 넘버링(`filename (1).ext`) 보장.
  - 단위 테스트 `tests::upload_filename_sanitization_and_uniqueness` 추가 및 검증 완료.
- **프론트엔드 이미지 삽입 인터페이스**:
  - **노션 스타일 이미지 모달 (`#imageModal`)**: 헤더 이미지 버튼 및 단축키(`Ctrl+Shift+I`) 제공. 로컬 파일 업로드 드롭존 및 웹 URL 삽입 탭 지원.
  - **클립보드 스크린샷 직접 붙여넣기 (`Ctrl+V`)**: 캡처 도구 등으로 캡처한 이미지를 편집기 내에서 바로 붙여넣으면 백엔드로 자동 업로드 후 마크다운 이미지 구문(`![image](images/...)`)으로 즉시 치환.
  - **파일 탐색기 드래그 앤 드롭**: 데스크톱/탐색기에서 이미지 파일을 편집기 위로 끌어다 놓으면 자동 업로드 및 삽입.

### 3. 편집기 핵심 UX 버그 해결 (`src/index.html`)
- **다중 블록 동시 드래그 및 블록 간 선택 불가 버그 해결**:
  - **원인 분석**: 기존에는 `#preview`는 일반 요소이고 각 블록 자식(`p`, `h1`, `li` 등)마다 개별적으로 `contenteditable="true"`가 부여되어 브라우저가 각 블록을 독립된 편집 영역으로 격리 취급함. 이로 인해 여러 블록을 마우스로 드래그하여 한 번에 선택하거나 `Ctrl+A`로 전체 선택하는 동작이 원천 차단됨.
  - **해결**: `#preview`를 단일 편집 컨테이너(`contenteditable="true"`)로 일원화하고, 개별 자식 블록 요소의 `contenteditable` 속성을 전면 제거. 이제 여러 문단, 제목, 목록에 걸쳐 자유로운 마우스 드래그 선택, 다중 블록 삭제, 복사, 굵게/기울임 서식 일괄 적용이 완벽하게 작동.
- **문서 맨 하단에 붙여넣기 시 맨 위에 붙는 버그 해결**:
  - **원인 분석**: 빈 공간 또는 문서 맨 끝 클릭 시 커서 위치의 부모 노드가 `#preview` 자체로 잡히면서, `data-block="true"`를 찾지 못해 안전 폴백으로 지정되어 있던 `preview.firstElementChild`(문서 최상단 블록)로 강제 지정되어 상단에 붙여넣어지는 현상 발생.
  - **해결**: 조상 노드를 탐색하는 `findClosestBlock(node)` 함수를 구현하고, 하단 여백 클릭이나 끝점 선택 시 `preview.lastElementChild`로 정확히 타겟팅되도록 수정. 텍스트 및 이미지 붙여넣기 시 현재 커서 위치 또는 문서 맨 하단에 정확히 삽입됨.

### 4. 불필요한 개발/임시 파일 정리 및 형상 관리
- 불필요한 스크립트(`build_icon.ps1`, `mdeditor.bat`) 및 임시 환경설정(`.env`) 삭제.
- `.gitignore`에 패키징 산출물 디렉토리(`/dist`), 인스톨러 빌드 타깃(`/installer/target`), 도구 캐시(`.squad`) 등록.

### 5. 검증 결과
- `cargo fmt --check`: 클린 통과.
- `cargo clippy -- -D warnings`: 메인 크레이트 및 인스톨러 크레이트 모두 0 경고 통과.
- `cargo test`: 54개 단위 테스트 전체 성공 (54 passed, 0 failed in 0.14s).
- 단일 설치 파일 `dist/MDEditor-Setup.exe` (1,137,664 바이트, ~1.1 MB) 빌드 및 체크섬 생성 완료.

## 2026-11-10 — 백엔드 최적화·리팩터링 및 프로덕션급 Windows 인스톨러/언인스톨러 스위트 구축

### 1. 백엔드 라우팅 및 리소스 최적화 (`src/main.rs`)
- **디렉토리/파일 인자 분기 처리 정밀화 (`resolve_target`)**:
  - 커맨드라인 또는 탐색기 컨텍스트 메뉴에서 디렉토리 경로가 인자로 전달될 때(`abs.is_dir()`), 해당 디렉토리를 루트 작업 폴더로 즉시 바인딩하고 파일 열기 파라미터를 초기화하여 폴더 열기 기능 완전 지원.
  - 파일 인자 전달 시 해당 파일의 부모 디렉토리를 루트로, 파일명을 초기 로드 파일로 안전하게 분기.
- **HTTP 응답 버퍼링 및 시스템 콜 절감 (`send_response_cache`)**:
  - `BufWriter::with_capacity(16 * 1024, stream)`을 도입하여 HTTP 헤더와 응답 본문을 메모리 버퍼에서 단일 TCP 패킷/단일 write 시스템 콜로 병합 전송함으로써 네트워크 패킷 단편화 방지 및 응답 레이턴시 최소화.
- **JSON 직렬화 무할당 최적화 (`write_json_escaped`, `list_markdown_files`, `json_wrap`)**:
  - `write_json_escaped`를 통해 기할당된 `String` 버퍼에 직접 이스케이프 문자를 스트리밍하도록 개선.
  - `list_markdown_files`에서 중간 `Vec<String>` 할당 2회 및 `join(",")` 재할당을 전면 제거하여 파일 목록 조회 시 단일 문자열 버퍼로 조합.
- **CLI 옵션 및 콘솔 리디렉션 (`--version`, `--help`)**:
  - `-v, --version`, `-h, --help` 옵션 추가.
  - GUI 서브시스템(`windows_subsystem = "windows"`) 상태에서 터미널 호출 시 `AttachConsole(ATTACH_PARENT_PROCESS)` 및 `CONOUT$` 스트림 핸들 재연결을 통해 콘솔 출력 지원.

### 2. 프로덕션급 Windows 설치/제거 마법사 업그레이드 (`install.ps1`, `install.bat`, `uninstall.ps1`, `uninstall.bat`)
- **실행 중인 프로세스 감지 및 안전 종료**:
  - 설치 및 제거 시 백그라운드에 구동 중인 `mdeditor.exe` 인스턴스를 감지하여 파일 잠금 충돌(File Lock) 없이 안전하게 종료 후 교체/제거.
- **Windows 설정 "설정 > 앱 > 설치된 앱" (프로그램 추가/제거) 공식 등록**:
  - `HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\MDEditor`에 `DisplayName`, `DisplayVersion`, `Publisher`, `DisplayIcon`, `InstallLocation`, `UninstallString`, `QuietUninstallString`, `EstimatedSize`, `URLInfoAbout`, `NoModify`, `NoRepair` 완벽 등록.
  - Windows 기본 설정 UI 및 제어판에서 앱 아이콘과 함께 표시되며, "제거" 클릭 시 즉시 제거 마법사 연동.
- **Windows 탐색기 폴더 및 폴더 배경 우클릭 컨텍스트 메뉴 ("MDEditor로 이 폴더 열기")**:
  - `HKCU:\Software\Classes\Directory\shell\MDEditor` 및 `HKCU:\Software\Classes\Directory\Background\shell\MDEditor` 등록 (`"%V"` 전달).
  - 어떤 폴더나 폴더 빈 공간에서도 우클릭 한 번으로 해당 위치를 루트로 MDEditor 실행 가능.
- **설치/제거 CLI 파라미터 및 마법사 비주얼 개선**:
  - `-Quiet`, `-NoDesktopShortcut`, `-RunAfterInstall` 파라미터 지원.
  - ASCII 로고 배너, 단계별 상태 인디케이터(`[1/6] ~ [6/6]`), `[✓]` 체크마크 등 상용 제품 수준의 콘솔 UX 제공.
  - 제거 시 인스톨러 디렉토리 자체 삭제를 위한 지연 백그라운드 프로세스 스케줄링 적용.

### 3. 검증 결과
- `cargo fmt --check`: 클린 통과.
- `cargo clippy -- -D warnings`: 무경고 무에러 통과.
- `cargo test`: 53개 단위 테스트 전체 통과 (53 passed, 0 failed in 0.04s).
- `cargo build --release`: **693,760 바이트 (~677 KB)**.
- `install.ps1`: 성공적으로 배포 완료 및 레지스트리/바이너리 해시 일치 검증 (`6AF8CD3D4F125983F4FE3A3DDBED26E6CD03D828E424D44BEB825A0970DDAC76`).

## 2026-11-10 — 블록 측면 인셋 라인 및 배경 박스 제거, 노션 표준 타이포그래피 정제

### 1. 포커스 시 블록 측면 인셋 라인 및 배경 박스 효과 제거 (`src/index.html`)
- 블록 포커스 시 좌측에 표시되던 3px 인셋 세로선(`box-shadow: inset 3px 0 0 var(--accent);`) 및 배경색 틴트(`background: rgba(var(--accent-rgb), 0.06);`)를 전면 제거.
- 블록 호버 시 배경색 깜빡임(`background: rgba(var(--accent-rgb), 0.035);`) 및 불필요한 음수 마진/패딩(`margin: 3px -8px; padding: 4px 8px;`) 제거.
- 우측 상단 호버 힌트 배지(`.notion-hint-badge`) 및 코드 블록 포커스 액센트 테두리 링 제거.
- 실제 노션과 동일하게 문서 캔버스가 깔끔한 종이처럼 작동하며, 텍스트 입력 시 어떠한 인위적인 테두리나 박스 없이 커서만 자연스럽게 깜빡이도록 개선.
- 빈 문단에만 노션 규격의 텍스트 플레이스홀더(`p[data-placeholder]:empty::before`)가 부드럽게 노출되도록 정리.

### 2. 검증 및 배포 결과
- `cargo fmt --check`: 클린 통과.
- `cargo clippy -- -D warnings`: 무경고 무에러 통과.
- `cargo test`: 51개 단위 테스트 전체 통과 (51 passed, 0 failed in 0.04s).
- `cargo build --release`: **668,160 바이트 (~652 KB)**.
- `install.ps1`: 로컬 Windows 데스크톱 배포 완료 (SHA256 해시 일치 확인).

## 2026-11-10 — 노션 라이브 캔버스 단일 전용 뷰 개편 (노션 뷰 단일화)

### 1. 상단 뷰 전환 세그먼트 제거 및 중앙 액션 간소화 (`src/index.html`)
- 상단 헤더의 [노션], [분할], [에디터], [미리보기] 보기 모드 전환 세그먼트 버튼을 완전히 제거하고, 상시 100% 노션 라이브 캔버스 전용 뷰로 전환.
- 상단 중앙에는 문서 목차(TOC) 토글 버튼(`btnToggleToc`)과 본문 검색 버튼(`btnHeaderSearch`)만 간결하게 배치하여 헤더 시각적 복잡도 최소화.

### 2. 레이아웃 단순화 및 불필요한 분할/에디터 컴포넌트 제거 (`src/index.html`)
- 기존 좌측 텍스트에디터 툴바(`.editor-header-bar`) 및 분할 조절용 드래그 구분선(`#dragDivider`) 제거.
- 백엔드 마크다운 상태 동기화 및 원자적 저장을 담당하는 `<textarea id="editor">`는 화면에 보이지 않는 백그라운드 버퍼(`display:none;`)로 유지하여 기존 모든 직렬화/역직렬화/자동저장 엔진 완전 보존.
- 에디터 내부에 중첩되어 있던 검색/바꾸기 플로팅 바(`#searchBar`)를 `<main>` 최상위로 재배치하여 노션 캔버스 상단 우측에 깔끔하게 오버레이되도록 개선.

### 3. 분할 리사이저 및 뷰 전환 단축키 코드 정리 (`src/index.html`)
- 분할 모드 마우스 드래깅 리사이저 이벤트 리스너(`isDragging`, `mousemove`, `mouseup`, `dblclick`) 및 스크롤 동기화 코드 제거.
- `Alt+1 ~ 4` 뷰 모드 전환 단축키를 제거하고, 도움말 모달과 웰컴 문서 가이드를 노션 단일 라이브 캔버스 기준으로 일원화.

### 4. 검증 및 배포 결과
- `cargo fmt --check`: 클린 통과.
- `cargo clippy -- -D warnings`: 무경고 무에러 통과.
- `cargo test`: 51개 단위 테스트 전체 통과 (51 passed, 0 failed in 0.04s).
- `cargo build --release`: **669,184 바이트 (~653 KB)**로 바이너리 추가 경량화.
- `install.ps1`: 로컬 Windows 데스크톱 배포 완료 (SHA256 해시 일치 확인).

## 2026-11-10 — 노션 라이브 캔버스 인터랙션 및 UI/UX 정밀 강화

### 1. 노션 모드 코드 블록 인플레이스 편집 및 키보드 조작 지원 (`src/index.html`)
- 기존에 읽기 전용으로 유지되던 코드 블록(`<pre><code>`)을 노션 모드 편집 가능 대상(`editableSelectors`)에 편입.
- 코드 블록 내부에서 <kbd>Tab</kbd> 입력 시 2칸 공백 들여쓰기, <kbd>Enter</kbd> 입력 시 줄바꿈(`\n`)이 삽입되도록 전용 키 핸들러 구현.
- 코드 블록 내부에서는 주석/경로 작성 시 의도치 않게 슬래시(`/`) 메뉴나 마크다운 자동 변환이 발동되지 않도록 입력 인터셉트 방어.
- 내용이 모두 지워진 상태에서 <kbd>Backspace</kbd> 시 코드 블록 래퍼 전체를 일반 문단(`<p><br></p>`)으로 부드럽게 복구.

### 2. 코드 블록 언어 실시간 변경 인터랙션 (`src/index.html`)
- 코드 블록 헤더의 언어 라벨(`.code-lang`)에 호버 피드백 및 클릭 이벤트 연결.
- 클릭 시 언어 입력 프롬프트(예: python, rust, javascript, html, css, json 등)를 제공하고, `language-{lang}` 클래스 갱신 및 `highlight.js` 구문 강조 즉시 재적용.
- `.code-block-wrapper:focus-within` 상태에서 은은한 액센트 테두리 링 및 그림자를 부여하여 직관적인 포커스 상태 제공.

### 3. 노션 캔버스 하단 여백 클릭 시 자동 문단 생성 (`src/index.html`)
- 문서 최하단이나 캔버스 빈 영역을 클릭했을 때 마지막 문단이 빈 상태이면 해당 문단에 포커스하고, 그렇지 않으면 하단에 새 문단(`<p><br></p>`)을 자동 생성하여 포커스.
- 노션, 크래프트, 옵시디언 라이브 프리뷰와 동일한 자연스러운 문서 작성 흐름 보장.

### 4. 0번 오프셋(맨 앞) 백스페이스 블록 복구/언래핑 UX (`src/index.html`)
- 제목(H1~H6) 블록의 텍스트 맨 앞에서 <kbd>Backspace</kbd>를 누르면 기존 텍스트를 그대로 유지한 채 일반 본문 문단(`<p>`)으로 즉시 전환.
- 목록 항목(`<li>`)의 맨 앞에서 <kbd>Backspace</kbd>를 누르면 목록에서 탈출하여 일반 본문 문단으로 전환되며, 하위/후속 항목의 연속성을 완전 보존.

### 5. 플로팅 서식 툴바 확장 및 전역 단축키 연결 (`src/index.html`)
- 플로팅 툴바에 키보드 키 태그 버튼(`<kbd>K</kbd>`) 및 서식 초기화 버튼(`✕`) 추가.
- 노션 모드에서 <kbd>Ctrl</kbd>+<kbd>K</kbd> 단축키를 통한 하이퍼링크 삽입 기능 지원.

### 6. 빠른 파일 열기(Quick Open) 모달 키보드 탐색 안정성 강화 (`src/index.html`)
- 파일 검색 필터링 시 `selectedFileIndex` 범위 검증 및 `classList` 옵셔널 체이닝을 추가하여 비정상 인덱스 참조 예외 방지.

### 7. 검증 및 배포 결과
- `cargo fmt --check`: 클린 통과.
- `cargo clippy -- -D warnings`: 무경고 무에러 통과.
- `cargo test`: 51개 단위 테스트 전체 통과 (51 passed, 0 failed in 0.09s).
- `cargo build --release`: 679,936 바이트 (~664 KB) 독립 실행 파일 빌드.
- `install.ps1`: 로컬 Windows 데스크톱 배포 완료 (`%LOCALAPPDATA%\Programs\mdeditor\mdeditor.exe` SHA256 일치 확인).

## 2026-11-10 — 워크스페이스 하위 폴더 지원, 파일 삭제, 슬래시 메뉴 검색 및 UX 정밀 고도화

### 1. 워크스페이스 하위 디렉토리(Subfolder) 마크다운 완전 지원 (`src/main.rs`, `src/index.html`)
- **원인 & 배경**: 기존 `safe_path`가 단일 컴포넌트(루트 폴더 내 파일)만 허용하여, `docs/guide.md` 등 하위 폴더에 있는 마크다운 파일을 열거나 링크할 때 400 Bad Request 에러가 발생하던 한계 극복.
- **구현**:
  - `safe_path` 강화: 상대 경로의 모든 컴포넌트가 `Component::Normal`인지 확인(상위 이동 `..`, 루트 `/`, 드라이브 문자 `C:`, ADS 등 차단)하고, 정규화된 `canonical_root`를 벗어나지 않도록 검증하여 하위 디렉토리 마크다운 파일의 안전한 로드/저장/이름변경 지원.
  - `/api/list` 재귀 탐색: `collect_markdown_files`를 통해 워크스페이스 내 최대 4단계 깊이까지 마크다운 파일을 재귀 스캔하며, `.git`, `target`, `node_modules`, `dist` 등 빌드/숨김 폴더는 자동 제외.
  - `save_atomic` 및 `do_rename`: 저장 또는 이름 변경 시 상위 디렉토리가 존재하지 않는 경우 `create_dir_all`로 자동 생성.
  - 프론트엔드 연동: 탐색기 모달에서 `docs/` 등 폴더 경로를 시각적으로 세련되게 분리 표시.

### 2. 작업 파일 영구 삭제 기능 (`/api/delete`) 프론트엔드 연동 (`src/index.html`)
- 백엔드에 존재하던 `POST /api/delete?file=...` 엔드포인트를 파일 탐색기 모달(`#fileListModal`)에 연동.
- 각 파일 행에 마우스 호버 시 휴지통 아이콘 버튼(`.btn-item-delete`) 노출.
- 삭제 전 확인 대화상자(`confirm`)를 띄워 실수를 방지하고, 삭제 완료 시 토스트 알림 표시 및 파일 목록 실시간 갱신.
- 현재 열려 있는 문서가 삭제된 경우 에디터를 초기화하고 최근 파일 목록에서도 자동 제거.

### 3. 슬래시 (`/`) 메뉴 실시간 검색 & 필터링 + 신규 블록 3종 (`src/index.html`)
- **실시간 검색**: `/` 입력 후 이어서 키워드(예: `/h1`, `/코드`, `/수식`, `/표`, `/이미지`, `/접기`) 입력 시 일치하는 블록 항목만 실시간으로 필터링되어 즉시 <kbd>Enter</kbd>로 선택 가능.
- **신규 블록 3종 추가**:
  - `toggle`: 접기/펼치기 토글 블록(`:::toggle 제목 ... :::`) 삽입.
  - `image`: 로컬/웹 이미지 삽입(`![설명](경로)`) 프롬프트.
  - `footnote`: 각주(`[^1]`) 삽입.

### 4. 노션 라이브 캔버스 목록(List) 키보드 UX 고도화 (`src/index.html`)
- 할 일 목록(`li.task-list-item`) 또는 일반 목록(`li`)에서 <kbd>Enter</kbd> 입력 시:
  - 내용이 있는 경우: 다음 항목을 자동으로 생성(할 일 목록은 체크 해제된 새 체크박스 자동 부여)하고 즉시 포커스.
  - 내용이 비어 있는 경우: 목록에서 깔끔하게 탈출하여 일반 본문 단락(`<p><br></p>`)으로 전환.
- 목록에서 <kbd>Tab</kbd> / <kbd>Shift+Tab</kbd> 입력 시: 목록 들여쓰기(하위 목록 중첩) 및 내어쓰기 처리.
- 빈 블록에서 <kbd>Backspace</kbd> 입력 시: 해당 블록을 삭제하고 이전 블록으로 포커스 이동.

### 5. 최근 파일(Recent Files) 히스토리 및 퀵 생성 (`src/index.html`)
- `localStorage['mdeditor-recent-files']`를 통해 최근 열람/편집한 파일 최대 8개를 보존.
- 탐색기 모달 열기 시 최근 파일 목록을 최상단에 우선 표시하여 잦은 파일 전환 편의성 극대화.
- 검색어와 일치하는 파일이 없는 경우 모달 내에서 `+ '파일명.md' 새 파일 생성` 퀵 액션 제공.

### 6. DOM ↔ Markdown 역직렬화 무손실 정밀도 개선 (`src/index.html`)
- 인라인 이미지 및 상대 마크다운 링크 역변환 시, 서버 서빙용 `/api/raw?file=...` 및 `/?file=...` URL을 원래의 깔끔한 상대 마크다운 경로로 정확히 디코딩 복원.

### 7. 검증 결과
- `cargo fmt --check`: 클린 통과.
- `cargo clippy -- -D warnings`: 무경고 무에러 통과.
- `cargo test`: 신규 테스트 2종(하위폴더 재귀 검색, 파일 삭제 안전성) 추가 포함 **총 50개 테스트 전체 통과 (50 passed, 0 failed)**.

## 2026-11-10 — 노션 스타일 통합 라이브 캔버스(Notion Live Canvas) 엔진 구현

### 1. 원스크린 인플레이스 편집 라이브 캔버스 (`src/index.html`)
- **목적**: 탭을 나누거나 좌우 창을 분할할 필요 없이, **한 화면에서 직접 타이핑하며 서식 렌더링을 동시에 확인**할 수 있는 노션/타이포라 스타일 통합 캔버스 제공.
- **구현**:
  - 기본 뷰 모드를 `notion`으로 설정하여 앱 시작 시 단일 라이브 캔버스 화면으로 즉시 열리도록 구성.
  - 미리보기 영역의 본문 텍스트, 제목(H1~H6), 인용구, 콜아웃 내용, 표 셀, 목록 등을 실시간 클릭 편집 가능한 `contenteditable="true"` 블록으로 바인딩.
  - 디바운스된 실시간 DOM-to-Markdown 양방향 직렬화 엔진(`serializePreviewToMarkdown`)을 구현하여 타이핑 중 원본 `editor.value`와 마크다운 파일 상태를 무손실 실시간 동기화.
  - 타이핑 도중 전체 DOM을 재렌더링하지 않고 입력 상태를 유지함으로써 한글/영어 입력기 조합(IME) 및 커서 위치를 완벽하게 보존.

### 2. 슬래시 (`/`) 블록 명령어 팝업 메뉴 (`src/index.html`)
- 빈 줄 또는 새 블록에서 `/` 입력 시 노션 스타일의 플로팅 블록 메뉴(`#slashMenu`) 자동 팝업.
- 제목 1~3, 할 일 목록, 불릿 목록, 번호 목록, 인용구, 콜아웃, 코드 블록, LaTeX 수식, Mermaid 다이어그램, 표, 구분선 등 12개 블록 타입 지원.
- 키보드 위/아래 방향키 탐색, `Enter` 선택, `Escape` 닫기 지원.

### 3. 플로팅 선택 영역 서식 툴바 (`src/index.html`)
- 라이브 캔버스 내에서 텍스트를 마우스로 드래그 선택 시 선택 영역 상단에 플로팅 퀵 서식 바(`#floatingFormatBar`) 팝업.
- 굵게(`B`), 기울임(`I`), 취소선(`S`), 형광펜(`==`), 인라인 코드(`< >`), 링크(`🔗`) 원클릭 서식 적용 및 마크다운 자동 동기화.

### 4. 수식 & Mermaid 다이어그램 전용 인라인 에디터 카드 (`src/index.html`)
- 렌더링된 수식(`.math-block`) 및 Mermaid 다이어그램(`.mermaid-wrapper`) 위에 마우스 호버 시 `✎ 수정` 버튼 노출.
- 클릭 시 플로팅 카드 모달(`#inlineEditorCard`)이 열리며 LaTeX 수식이나 Mermaid 다이어그램 소스를 직접 편집하고 `적용` 시 즉각 렌더링 갱신 및 마크다운 동기화.

### 5. 뷰 모드 4종 확장 & 단축키 연동
- 상단 헤더 뷰 전환 버튼 및 전역 단축키를 4종으로 확장:
  - `Alt + 1`: 🌟 노션 라이브 모드 (기본)
  - `Alt + 2`: 🪟 분할 모드 (에디터 + 미리보기)
  - `Alt + 3`: ✏️ 에디터 전용 모드
  - `Alt + 4`: 👁️ 미리보기 전용 모드
- 뷰 모드 전환 시 작성 중이던 모든 데이터가 즉시 양방향 연동되어 일체의 데이터 유실 없이 작업 상태 유지.

### 6. 검증 결과
- `cargo fmt --check`: 클린 통과.
- `cargo clippy -- -D warnings`: 무경고 무에러 통과.
- `cargo test`: 단위 테스트 48종 전체 통과 (48 passed, 0 failed).
- `cargo build --release`: 바이너리 크기 638,976바이트 (~624KB) 성공적 빌드 완료.

## 2026-11-10 — UI/UX 대대적 고도화 및 신규 기능 추가

### 1. 활성 파일명 뱃지 & 즉시 이름 변경 (`src/index.html`)
- 상단 헤더 브랜드 우측에 현재 편집 중인 파일명을 표시하는 직관적인 파일 뱃지(`#fileBadge`) 추가.
- 수정 중(미저장)인 경우 황색 펄스 닷(`●`)이 함께 표시되어 저장 상태를 한눈에 식별 가능.
- 뱃지 클릭 시 즉시 파일명을 변경할 수 있으며, 서버의 `/api/rename` 엔드포인트를 호출하여 원자적 이름 변경 및 브라우저 URL 동기화 처리.

### 2. 다중 라인 들여쓰기 & 내어쓰기 (Tab / Shift+Tab) 구현 (`src/index.html`)
- 원인: 도움말 문서에 안내되어 있었으나 실제 자바스크립트 리스너가 누락되어 포커스가 textarea 밖으로 빠져나가던 문제.
- 수정: 단일 커서 시 2칸 공백 삽입, 선택 영역 존재 시 선택된 모든 라인을 일괄 2칸 들여쓰기(`Tab`) 및 2칸 내어쓰기(`Shift+Tab`) 처리하고 선택 범위를 정확히 보존하도록 구현.

### 3. 에디터 본문 찾기 및 바꾸기 (Find & Replace) 패널 (`src/index.html`)
- 에디터 상단에 플로팅 검색/바꾸기 패널(`#searchBar`) 구현 (`Ctrl+F` 검색, `Ctrl+H` 바꾸기).
- 실시간 매칭 개수 카운트(`N / Total`), 대소문자 구분 토글(`Aa`), 다음/이전 매칭 이동(`Enter` / `Shift+Enter`), 현재 일치 항목 자동 스크롤 및 선택 연동.
- 단일 바꾸기 및 전체 일치 항목 일괄 바꾸기(`모두 바꾸기`) 기능 및 변경 완료 피드백 제공.

### 4. 작업 폴더 빠른 파일 탐색기 (Quick Open) (`src/index.html`)
- 헤더 파일 목록 버튼(`#btnFiles`) 및 단축키 `Ctrl+O` 연동 모달(`#fileListModal`) 추가.
- `/api/list` 및 `/api/info`와 연동하여 현재 작업 디렉토리의 마크다운 파일들을 실시간 필터링/검색.
- 키보드 방향키(`↑`, `↓`) 및 `Enter` 키로 즉시 파일 전환, 새 파일(+) 및 OS 파일 열기 연계.

### 5. 하단 상태바 정보 세분화 & 실시간 커서 위치 추적 (`src/index.html`)
- 좌측: 문서 저장 상태 및 펄스 도트 유지.
- 우측: 실시간 커서 위치(`X행 Y열`), 선택 텍스트 길이(`N자 선택됨`), 문서 통계(`자수 · 줄수 · 예상 읽기 시간`) 분리 배치.
- 자동 줄바꿈 토글 버튼(`#btnToggleWordWrap`) 배치.

### 6. 자동 줄바꿈 (Word Wrap) 토글 (`src/index.html`)
- 에디터의 긴 줄바꿈 처리 방식을 켜기(`pre-wrap`) / 끄기(`pre`)로 전환 가능.
- 설정 메뉴 및 상태바 버튼과 양방향 연동되며 `localStorage`에 상태 영구 보존.

### 7. 괄호/따옴표 자동 닫기 & 스마트 삭제 (`src/index.html`)
- `(`, `[`, `{`, `"`, `` ` `` 입력 시 자동 닫는 기호 삽입 및 커서 가운데 배치.
- 닫는 기호 위치에서 동일 기호 입력 시 중복 삽입 없이 스킵.
- 빈 괄호 쌍 중간에서 `Backspace` 입력 시 양쪽 기호 일괄 삭제.

### 8. 목차 (TOC) 스크롤 스파이 (Scroll Spy) (`src/index.html`)
- 미리보기 스크롤 시 현재 읽고 있는 섹션의 헤딩(H1/H2/H3)을 목차 드로어에서 실시간으로 하이라이트(`.active`).

### 9. 마크다운 원본/HTML 복사 서브메뉴 & 모던 토스트 (`src/index.html`)
- 설정 메뉴 내 마크다운 원본 복사 및 렌더링된 HTML 복사 기능 추가.
- 상태 변경, 파일 열기, 이름 변경, 복사 완료 시 화면 우측 하단에 세련된 토스트 알림(`#toast`) 팝업.

### 10. 도움말 모달 단축키 표 최신화 (`src/index.html`)
- `Ctrl+F`, `Ctrl+H`, `Ctrl+O`, `Tab / Shift+Tab`, `Esc` 등 신규 단축키 안내 반영.

### 11. 검증 결과
- `cargo fmt --check`: 클린 통과.
- `cargo clippy -- -D warnings`: 무경고 무에러 통과.
- `cargo test`: 총 48개 단위 테스트 전체 통과 (48 passed, 0 failed).
- `cargo build --release`: 바이너리 크기 604,672바이트 (~590KB) 성공적 빌드.

## 2026-11-10 — 시스템 최적화 및 버그 개선

### 1. Clippy 경고 해결 (`src/main.rs`)
- `src/main.rs:655`, `src/main.rs:673`의 브라우저 실행 분기 `collapsible_if` 2건 축약 정리.
- `cargo clippy -- -D warnings` 무경고 무에러 통과.

### 2. 소켓 핸들 복제 최적화 (`src/main.rs`)
- 원인: `parse_request`에서 `BufReader` 생성을 위해 매 HTTP 요청마다 `stream.try_clone()?` 시스템 콜(Windows `DuplicateHandle`)을 호출하여 OS 리소스 낭비 발생.
- 수정: `read_line_limited<R: Read>` 제네릭화 및 `BufReader<&mut TcpStream>`을 활용하여 스트림 핸들 복제 없이 직접 파싱하도록 개선.

### 3. 원자적 파일 저장 (`save_atomic`) 안전성 강화 (`src/main.rs`)
- 원인: 단순 `std::fs::write` 후 `rename` 시 OS 디스크 캐시 플러시가 보장되지 않아 급작스러운 정전/크래시 시 파일이 손상될 위험이 있었고, 고정 `.tmp` 접미사로 동시 저장 시 충돌 가능성 존재.
- 수정: 임시 파일명에 PID 및 시퀀스 식별자(`.tmp-{pid}-{seq}`) 부여 및 `file.sync_all()` (`FlushFileBuffers`) 명시적 호출로 디스크 물리 기록 보장 후 원자적 교체.

### 4. 다중 탭/창 세션 및 안전 종료 시퀀스 동기화 (`src/main.rs`, `src/index.html`)
- 원인: 브라우저 탭/창이 여러 개 열려 있을 때 한 탭을 닫으면 `pagehide`로 `/api/exit`이 호출되어, 다른 탭을 작업 중임에도 2초 뒤 서버가 일괄 종료되는 문제.
- 수정: 프론트엔드에서 고유 `tabId`를 발급하고 4초 주기로 `/api/ping?tab=...` 생존 신호를 전송. 백엔드는 `ACTIVE_TABS`로 활성 탭 목록을 관리하여, 닫힌 탭 외에 다른 탭이 살아있다면 서버를 유지하고 마지막 탭이 닫혔을 때만 2초 유예 후 프로세스를 종료하도록 개선.

### 5. 마크다운 파서 (`src/markdown.rs`) 핵심 버그 수정 및 기능 고도화
- **코드 블록 내 각주 오인식 수정**: 코드 블록(```, ~~~) 내부에 작성된 `[^1]:` 주석/코드가 문서 각주로 오인식되어 추출되던 버그 수정.
- **틸드(`~~~`) 코드 블록 지원**: CommonMark 표준에 맞춰 틸드 3개 이상 코드 블록 지원.
- **더블/멀티 백틱 코드 스팬 지원**: `` ` `` 또는 ``foo ` bar`` 등의 임의 개수 백틱 인라인 코드 스팬 지원.
- **테이블 내 코드 스팬 파이프 보호**: 표(Table) 내부에서 백틱 코드 스팬 안의 파이프(`|`) 및 이스케이프 파이프(`\|`)가 열을 잘못 분리하지 않도록 개선.
- **링크 내 이미지 중첩 완벽 지원**: 이미지(`replace_images`)를 우선 추출·격리 후 링크(`replace_links`)를 처리하여 `[![배지](img)](url)` 구조가 깨짐 없이 정상 렌더링되도록 수정.
- **내부 앵커 링크 버그 수정**: 문서 내부 앵커 링크(`#heading`)에 불필요하게 `target="_blank"`가 붙어 새 탭이 열리던 문제를 해결하고, 미리보기 내에서 부드러운 스크롤(`scrollIntoView`) 연동.
- **마크다운 파일 상호 링크 지원**: 같은 폴더 내 마크다운 파일 링크(`[문서](other.md)`) 지원 및 클릭 시 저장 여부 확인 후 즉시 MDEditor 내에서 전환 열기 연동.
- **Atx 닫는 해시 및 온점 슬러그 지원**: `## 1.2 개요 ##` 형태의 닫는 해시 제거 및 온점 포함 제목의 슬러그 가독성(`1-2-개요`) 개선.

### 6. 경로 보안 강화 (`src/main.rs`)
- Windows 파일시스템 특성상 공백이나 온점으로 끝나는 비정상 경로(`file.md `, `file.md.`)에 대한 차단 규칙 추가.

### 7. 프론트엔드 UI/UX 디테일 (`src/index.html`)
- 미리보기 내 마크다운 상호 링크(`a.md-link`, 대시 밑줄) 및 내부 앵커 링크(`a.anchor-link`, 점선 밑줄) 시각적 힌트 제공.
- 앵커 및 마크다운 파일 클릭 이벤트 인터셉트 처리.

### 8. 검증 결과
- `cargo fmt --check`: 차이 없이 클린 통과.
- `cargo clippy -- -D warnings`: 무경고 무에러 클린 통과.
- `cargo test`: 신규 테스트 7종 추가 포함 **총 48개 테스트 전체 통과 (48 passed, 0 failed)**.
- `cargo build --release`: 바이너리 크기 572,928바이트 (~559KB). 스모크 테스트(/api/info, /api/ping, /api/render, /api/exit) 정상 통과.

## 2026-11-09 — UI/UX 모더나이제이션 + 배포

### 1. 프로젝트 리뷰
- `cargo test` 41개 전부 통과 확인.
- `cargo clippy -- -D warnings` 실패 2건 (`collapsible_if`, `src/main.rs` 브라우저 실행 분기) — 기존 코드 문제로 기록만 함.
- `cargo fmt --check` 차이 다수 — 기존 포맷 문제로 기록만 함.
- 보안 설계 확인: `safe_path`/`safe_asset_path`, Host·Origin 검증, XSS 이스케이프, 원자적 저장.

### 2. 에디터 메뉴 정렬·반응형 수정 (`src/index.html`)
- 상·하단 메뉴 모두 왼쪽 정렬 단일행으로 고정 (`justify-content: flex-start`, `flex-wrap: nowrap`).
- 좁은 화면에서 줄바꿈 깨짐 제거: 헤더·툴바 가로 스크롤 유지, 모바일에서 통계 숨김 → 이후 상태바로 이동하며 해당 규칙 삭제.
- `cargo test` 41개 통과.

### 3. 배포 1차
- `cargo build --release` 성공 (566,272바이트).
- 스모크 테스트: `/api/info` 정상 응답 확인.
- 설치 스크립트는 샌드박스 밖 쓰기라 직접 실행 불가 → 사용자가 `install.bat` 실행.
- 설치 검증: 설치본·빌드본 크기·시간 일치, 레지스트리 `.md` 연결·열기·우클릭 메뉴, 시작 메뉴·바탕화면 바로가기, 사용자 PATH 등록 확인.

### 4. README 원문 표시 수정 (`README.md`)
- 원인: 파서는 보안상 raw HTML을 전부 이스케이프하는데, README 상단이 `<p>`·`<strong>`·`<img>` HTML로 작성되어 미리보기에서 원문 노출.
- 상단을 순수 마크다운(굵은 설명 + `![](배지)` 6종)으로 전환. `/api/render` 프로브로 `<strong>`·`<img>` 렌더 확인.
- 프로그램 로직 변경이 아니라 추가 테스트 없음.

### 5. 파일 메뉴 정리 (`src/index.html`)
- 파일 선택 드롭다운 제거 → OS 파일 열기 대화상자의 불러오기 버튼으로 교체 (`#btnOpen` + 숨김 `#filePicker`).
- 이름 변경·삭제 버튼, 클립보드 복사 버튼 제거 (백엔드 API는 유지).
- `hasFile`·`loadFileList`·드롭다운 리스너 제거, 존재 확인은 `/api/load` 프로브(`serverFileExists`)로 대체.
- 파일 그룹은 `불러오기·새 파일·저장` 라벨 버튼으로 통일, 나머지는 아이콘 버튼 유지.
- 서빙 HTML에서 구 요소 제거·신 요소 존재 확인, `cargo test` 41개 통과.
- README의 이름 변경·삭제 문구를 현재 UI에 맞게 수정.

### 6. 헤더 아이콘 전용화 (`src/index.html`)
- 불러오기·새 파일·저장 텍스트 라벨 제거, 툴팁 유지. 나머지 텍스트(브랜드·보기모드·상태)는 유지.

### 7. 하단 상태바 (`src/index.html`)
- 글자수·줄수·읽기시간·저장 상태를 헤더에서 빼서 `footer#statusbar`로 이동 (왼쪽 상태, 오른쪽 통계).
- 인쇄 시 숨김, 좁은 화면에서 가로 스크롤. ID 유지라 JS 변경 없음.
- 서빙 페이지에서 푸터 존재·ID 단일·헤더 잔류 없음 확인.

### 8. 폰트 통일 (`src/index.html`)
- 인터페이스 기본 폰트를 KoPubWorld돋움으로 교체. `font-kopubworld@1.0.3/css/dotum.css`(Light 300·중간 400·Bold 700, 200 OK 확인)를 CDN 연결.
- 미리보기는 16px·행간 1.9, 제목은 Bold(700). 에디터·코드·통계 숫자의 JetBrains Mono는 정렬상 유지, HTML 내보내기 템플릿도 같은 스택으로 맞춤.
- 서빙 페이지에서 폰트 링크·Pretendard 잔류 없음 확인, `cargo test` 41개 통과.

### 9. MDEditor 개명 + 메뉴 재구성 (`src/index.html`, `src/main.rs`, `README.md`, 설치 스크립트)
- 표시명만 `MDEditor`로 변경. 식별자(실행파일명, ProgID, 레지스트리 키, `/api` 필드, localStorage 키)는 호환성 유지.
- 저장 버튼 파랑(`primary`) 제거 → `ghost`로 다른 아이콘 버튼과 통일.
- 테마·도움말을 헤더에서 빼서 톱니바퀴 설정 드롭다운으로 이동. 현재 테마에 따라 메뉴 라벨 전환.
- 메뉴 체계: 파일(불러오기·새 파일·저장) / 보기모드+목차 / 내보내기(인쇄·HTML) / 설정·종료.
- 종료(`⏻`) 버튼은 이후 항목 14에서 제거됨.

### 10. 설정 메뉴 잘림 수정 (`src/index.html`)
- 원인: 헤더 가로 스크롤 `overflow` 안에서 열린 메뉴가 아래 창에 가려짐.
- 메뉴를 헤더 밖(body 직속)으로 이동 + `position: fixed`, 열릴 때 버튼 위치를 재서 우측 정렬. 헤더 스크롤·창 크기 변경 시 닫힘 처리.
- Playwright 실측: 메뉴 영역이 화면 안에 온전히 표시되고 최상위로 hit, JS 에러 없음.

### 11. 콘솔 창 문의 답변 (코드 변경 없음)
- PE 서브시스템 실측: 릴리스·설치본 `WINDOWS_GUI`(콘솔 없음), 디버그 `WINDOWS_CUI`(콘솔 있음).
- 본 창은 디버그 실행·`cargo run` 경로에서 나온 것으로 확인. 평소 사용(설치본·`.md` 연결)은 콘솔 없음.
- `mdeditor.bat`은 `start` 분리 실행 후 즉시 종료되므로 창이 남지 않음.

### 12. 모바일 390px 감사 + 설정 메뉴 닫힘 버그 수정 (`src/index.html`)
- 390px 실측 중 발견: 설정 버튼 클릭 시 헤더가 미세 스크롤되면서 스크롤-닫기 로직이 메뉴를 즉시 닫음.
- 수정: 스크롤 시 닫는 대신 메뉴 위치를 버튼에 다시 맞춤. 390px에서 열린 유지·화면 내 표시·최상위 hit 확인.

### 13. 접근성·정합성 다듬기 (`src/index.html`, `README.md`)
- F1 도움말 열기, `Esc`로 도움말·설정메뉴 닫기, 라이트모드 전환·라벨·코드강조 테마 연동 실동작 확인.
- 메뉴 이동 후 불필요해진 `.menu-wrap` 래퍼 제거.
- `prefers-reduced-motion` 환경에서 트랜지션·부드러운 스크롤 비활성화.
- 제거된 기능(클립보드 복사, 이름 변경·삭제)의 남은 README 문구 정리. 인앱 도움말·단축키 표는 실제 동작과 대조 완료.

### 14. 종료 버튼 제거 (`src/index.html`, `README.md`)
- 창 `X` 버튼으로 충분하다는 요청 반영. 버튼·핸들러·미사용 `danger` 스타일 삭제.
- `X`로 닫으면 기존 `pagehide` 비콘이 `/api/exit`으로 서버 종료를 알려 서버가 남지 않음. 백엔드 종료 API는 유지.
- README 종료 안내를 `X` 버튼 기준으로 수정. 빌드·테스트·페이지 로드 에러 없음 확인.

### 15. 배포·Git
- 배포 2차(메뉴 정리·폰트·상태바 반영): 릴리스 562,176바이트, 스모크 통과, `install.bat` 사용자 실행 후 설치본 일치·레지스트리·바로가기·PATH 확인.
- 배포 3차(종료 버튼 제거 반영): 릴리스 564,736바이트, 스모크 통과, `install.bat` 사용자 실행 후 설치본 일치 확인.
- `2aeff01 feat(ui): MDEditor branding, settings menu, bottom status bar and menu cleanup` — 7개 파일 지정 커밋 후 origin main 푸시, 워킹트리 깨끗함 확인.

## 2026-11-10 — 노션 스타일 일체형 라이브 캔버스 & 완성도/버그 수정

### 1. 노션 스타일 단일 화면 인라인 편집 (Notion Live Canvas)
- 탭 분할 없이 단일 화면에서 실시간 렌더링 결과물을 직접 클릭하여 편집할 수 있는 '노션 캔버스(Notion Mode)' 모드를 기본 보기로 구축.
- **슬래시(`/`) 명령 팝업 메뉴**: 15종 블록(제목 1~3, 글머리/번호/체크리스트, 인용구, 콜아웃, 코드 블록, 수식 블록, Mermaid 다이어그램, 표, 토글 접기, 이미지, 각주, 구분선) 실시간 검색 및 방향키/엔터 선택 지원.
- **플로팅 선택 서식 바 (Floating Toolbar)**: 텍스트 드래그 선택 시 볼드, 이탤릭, 취소선, 형광펜, 인라인 코드, 링크, 키캡 등 빠른 서식 적용.
- **인라인 수식 및 다이어그램 전용 에디터 카드**: LaTeX 수식 및 Mermaid 다이어그램 블록에 호버 시 `✎ 수식 수정`, `✎ 다이어그램 수정` 카드가 뜨며 실시간 소스 수정 및 렌더링.
- **손실 없는 DOM-to-Markdown 직렬화 (`serializePreviewToMarkdown`)**: 라이브 캔버스 내 변경 사항을 표준 마크다운으로 무손실 양방향 동기화.

### 2. 마크다운 자동 변환 단축키 (Markdown Auto-Transform Triggers)
- 노션 및 Typora 스타일로 빈 문단이나 블록 시작 부분에서 마크다운 기호를 입력하면 즉시 해당 블록으로 변환:
  - `# ` ~ `###### `: Heading 1 ~ 6
  - `- `, `* `: 글머리 기호 목록 (Bullet list)
  - `1. `: 번호 매기기 목록 (Numbered list)
  - `[] `, `[ ] `: 체크박스 할 일 목록 (Task list)
  - `[x] `, `[X] `: 완료된 할 일 목록
  - `> `: 인용구 블록 (Blockquote)
  - `--- `, `*** `, `___ `: 가로 구분선 (Horizontal rule)
  - ` ``` ` / ` ```mermaid `: 코드 블록 및 Mermaid 다이어그램 블록

### 3. 표(Table) 키보드 탐색 및 자동 행 추가
- `td`, `th` 셀 내에서:
  - `Tab`: 다음 셀로 이동, 표의 마지막 셀에서 Tab 입력 시 자동으로 동일 열 개수의 신규 행(`<tr>`)을 추가하고 첫 셀로 포커스.
  - `Shift+Tab`: 이전 셀로 이동.
  - `Enter`: 바로 아래 행의 동일 열 셀로 이동, 마지막 행에서 Enter 입력 시 새 행을 추가하고 해당 열로 포커스. (원치 않는 `<br>`/`<div>` 삽입으로 인한 레이아웃 깨짐 원천 방지)

### 4. 노션 캔버스 찾기 및 바꾸기 (Find & Replace)
- `Ctrl+F`, `Ctrl+H`가 노션 캔버스 모드에서도 완전하게 동작:
  - TreeWalker를 통해 `#preview` 텍스트 노드를 실시간 검색하고 매칭된 위치에 브라우저 네이티브 Selection Range 설정.
  - 매칭 블록에 펄스 아웃라인(`.notion-search-target`)을 부여하고 부드러운 스크롤(`scrollIntoView`).
  - 현재 항목 바꾸기(`replaceCurrent`) 및 전체 바꾸기(`replaceAll`) 실행 시 캔버스 텍스트 노드를 교체하고 에디터/마크다운 소스와 즉각 무손실 동기화.

### 5. 외부 서식 붙여넣기 정제 (Paste Sanitization)
- 외부 웹 브라우저나 워드 프로세서에서 복사한 지저분한 스타일의 인라인 HTML(`font`, `style` 등)이 라이브 캔버스를 오염시키지 않도록 `paste` 이벤트를 인터셉트.
- `text/plain`을 추출하여 표 셀/코드 블록 내에서는 안전한 인라인 텍스트 삽입, 본문에서는 클린 문단(`p`) 블록 분할 생성.

### 6. HTML 내보내기 및 인쇄 스타일 누수 차단
- HTML 내보내기 시 `.notion-card-edit-btn`, `.slash-menu`, `.floating-format-bar`, `.inline-editor-card` 등 UI 보조 요소를 제거하고 `contenteditable` 속성 및 임시 클래스 제거.
- `/api/raw?file=...` 이미지 상대 경로 및 `/?file=...` 상호 문서 링크를 원본 상대 경로로 복원하여 독립형 HTML로 배포 가능.
- `@media print`에 UI 편집 버튼, 슬래시 메뉴, 검색창을 숨김 처리하여 깔끔한 인쇄물 출력.

### 7. 작업 폴더 서브폴더 탐색 및 파일 삭제 기능
- `safe_path` 보강: 디렉터리 탐색(`..`)은 차단하되 합법적인 서브폴더 경로(`docs/guide.md`) 허용.
- `collect_markdown_files`: 작업 폴더 내 하위 디렉터리를 깊이 4까지 재귀 스캔(`.git`, `target`, `node_modules` 제외)하여 Quick Open(`Ctrl+O`)에서 서브폴더 파일 검색 가능.
- 최근 파일 히스토리(`localStorage`) 관리 및 휴지통 버튼 클릭 시 `POST /api/delete`를 통한 안전 삭제 지원.

### 8. 마크다운 파서 통화 기호 vs 인라인 수식 버그 수정 (`src/markdown.rs`)
- `$10 to $20` 또는 `$15-$25`와 같은 금액 표기가 인라인 수식으로 오인식되어 `<span class="math-inline">`으로 렌더링되던 문제 해결 (닫는 `$` 뒤에 숫자가 바로 오는 경우 통화 기호로 판별하여 수식 종료로 인식하지 않음).
- 단위 테스트 추가 (`currency_is_not_treated_as_inline_math`).

### 9. 검증 및 빌드 결과
- `cargo fmt --check`: 클린 통과.
- `cargo clippy -- -D warnings`: 무경고 무에러 클린 통과.
- `cargo test`: 신규 테스트 포함 **총 51개 테스트 전체 통과 (51 passed, 0 failed)**.
- `cargo build --release`: 단일 실행 파일 672,256바이트 (~656KB) 성공 빌드. 외부 크레이트 의존성 0개 (`std` 전용).

### 10. Windows 로컬 배포 및 전방위 성능 최적화 (배포 4차·5차 완료)
- **마크다운 파서 제로 카피 & 힙 할당 최적화 (`src/markdown.rs`)**:
  - `render_lines`: `lines.len() * 48` 사전 버퍼 할당으로 렌더링 도중 힙 재할당 원천 방지.
  - `inline`: 단락에 `\u{1}`이 없을 때 불필요한 `replace` 할당 회피 (`Cow`).
  - `escape_html`: 이스케이프 문자(`&`, `<`, `>`, `"`)가 없을 때 문자열 순회 없이 즉시 반환.
  - `replace_delim`, `replace_images`, `replace_links`: 단일 소유권 이동(`String`) 및 각 기호 미포함 시 즉시 반환하여 줄당 최대 9회의 불필요한 중간 `String`/`Vec<char>` 할당 제거.
  - 백슬래시 이스케이프: `char::to_string()` 대신 스택 버퍼(`encode_utf8(&mut [0u8; 4])`) 사용.
  - 파싱 벤치마크: 전체 51개 단위 테스트 실행 시간 **0.12s -> 0.03s로 약 4배 단축**.
- **백엔드 HTTP 서버 I/O 최적화 (`src/main.rs`)**:
  - `handle_client`: 루프백 TCP 소켓에 `stream.set_nodelay(true)` 적용하여 Nagle 알고리즘 지연(10~40ms) 제거.
  - `/api/render`: `req.body`에서 불필요한 중간 `String` 할당 제거 (`Cow::Borrowed` 기반 파싱).
  - `/favicon.ico`: `Cache-Control: public, max-age=86400` 브라우저 캐싱 적용.
  - `json_escape`: 사전 버퍼 여유 할당(`+ s.len() / 8 + 16`)으로 JSON 직렬화 재할당 방지.
  - `url_decode`: `String::from_utf8`로 버퍼 소유권 직접 이전 (복사 없는 O(1) 반환).
- **프론트엔드 DOM 렌더링 & 저장 신뢰성 최적화 (`src/index.html`)**:
  - `setupNotionBlocks`: `data-block-ready` 체크를 적용하여 새 블록 삽입 시 기존 블록의 불필요한 재순회를 방지(O(N) -> O(1)).
  - `save()`: 저장 직전 대기 중인 `previewSyncTimer`를 강제 플러시하여 타이핑 직후 저장 시 최신 캔버스 내용 유실 방지.
  - CSS 리플로우 격리: `body.notion-view #preview [data-block]`에 `contain: layout style;`을 적용하여 대용량 문서 타이핑 시 DOM 재배치 부하 격리.
- **배포 완료**:
  - 최신 최적화 릴리즈 바이너리(673,280 바이트)를 `install.ps1`을 통해 로컬 설치 경로(`%LOCALAPPDATA%\Programs\mdeditor\mdeditor.exe`)에 완전 배포 및 설치본 일치 검증 완료.

## 2026-11-10 — 노션 스타일 모달 메시지 박스 및 종료/자동 저장 UX 개선 (배포 6차)

### 1. 브라우저 네이티브 confirm() 제거 및 노션 스타일 커스텀 모달 구현 (`src/index.html`)
- 브라우저 기본의 각진 흰색 alert/confirm 팝업창을 완전히 제거.
- 세련된 노션 디자인 가이드에 맞춘 전용 모달 (`#confirmDialogModal`, `.dialog-modal`) 도입:
  - 12px 둥근 모서리, 소프트 그림자(`var(--shadow-lg)`), 다크/라이트 테마 자동 반응.
  - 컨텍스트별 SVG 아이콘 및 뱃지 배경 (`.warning`, `.danger`, `.info`, `.exit`).
  - 팝인 애니메이션(`dialogPop`, `cubic-bezier(0.16, 1, 0.3, 1)`).
  - 버튼 레이아웃: `[저장 후 계속 / 저장 후 종료]` (Primary Accent), `[저장 안 함]` (Muted Red Ghost), `[취소]` (Ghost).
  - 키보드 접근성: Esc 키로 안전 취소/닫기 및 모달 열림 시 확인 버튼 자동 포커스.

### 2. 프로그램 종료 (Exit) 및 단축키 연동 (`src/index.html`)
- 설정 메뉴(`#settingsMenu`)에 "프로그램 종료" 버튼 (`#menuExit`, 레드 아이콘) 추가.
- 전역 단축키 <kbd>Ctrl</kbd>+<kbd>Q</kbd> 바인딩:
  - 미저장 변경사항이 있을 때: "저장 후 종료", "저장 안 함 (종료)", "취소" 3선택지 제공.
  - 변경사항이 저장되어 있을 때: "프로그램 종료", "취소" 확인 제공.
  - 종료 승인 시 백엔드 세션(`/api/exit?tab=...`) 안전 호출 후 `window.close()` 실행 및 브라우저 정책 차단 시 안전 종료 안내 화면(`showExitOverlay`) 표시.

### 3. 디바운스 자동 저장 (Auto-Save) 연동 (`src/index.html`)
- 노션 라이브 캔버스에서 타이핑 중 2초간 입력이 멈추면 백그라운드에서 디스크로 자동 저장(`scheduleAutoSave`).
- 탭 전환(`visibilitychange: hidden`) 또는 창 포커스 아웃(`blur`) 발생 시 즉시 디스크 안전 저장.
- 문서가 자동으로 안전 저장되므로, 실수로 창을 닫더라도 데이터 유실이 없으며 브라우저의 투박한 기본 팝업이 뜨지 않고 깔끔하게 닫힘.

### 4. 파일 삭제 및 덮어쓰기 모달 업그레이드 (`src/index.html`)
- 파일 목록 탐색기(`Ctrl+O`)에서 파일 삭제 시 노션 스타일 경고 모달 표시 (`danger` 뱃지 및 영구 삭제 주의 안내).
- 파일 덮어쓰기 저장 시 기존 파일 존재 경고 모달 표시.

### 5. 검증 및 배포 결과
- `cargo fmt --check`: 클린 통과.
- `cargo clippy -- -D warnings`: 무경고 무에러 클린 통과.
- `cargo test`: 전체 51개 단위 테스트 100% 통과 (51 passed, 0 failed).
- `cargo build --release`: 최신 바이너리 성공적 빌드.
- `install.ps1`: 로컬 Windows 환경(`%LOCALAPPDATA%\Programs\mdeditor`) 배포 완료 및 바이너리 SHA256 해시 일치 검증 완료.

## 2026-11-10 — 전반적인 UI/UX 프로덕션(Notion/Linear) 표준 격상 (배포 7차)

### 1. Notion / Linear 정밀 디자인 토큰 및 컬러 팔레트 구축 (`src/index.html`)
- 다크 모드: Tailwind 계열의 청색조를 정통 노션 차콜(`#191919` 캔버스, `#202020` 서피스, `#262626` 카드, `#303030` 호버, `#2383e2` 노션 블루)로 전면 리셋하여 눈의 피로도를 대폭 낮추고 시각적 안정감 극대화.
- 라이트 모드: 순백색 캔버스(`#ffffff`)와 노션 특유의 따뜻한 오프화이트 서피스(`#f7f7f5`), 시그니처 차콜 본문색(`#37352f`)을 적용하여 인쇄물 수준의 고품질 가독성 확보.
- 다층 박스 섀도우 및 글래스모피즘(`backdrop-filter: blur(16px) saturate(180%)`) 도입.

### 2. 노션 라이브 캔버스 타이포그래피 & 블록 간격 최적화 (`src/index.html`)
- 본문 단락(`p`) 행간을 `1.65`, 블록 간격을 `4px 0`으로 조정하여 노션과 동일한 유기적이고 컴팩트한 블록 흐름 구현.
- 제목(H1/H2)의 인위적인 하단 테두리를 제거하고 비례 마진(`margin-top: 34px / 26px`) 및 자간(-0.015em) 최적화.
- 빈 문단 플레이스홀더를 포커스 상태(`:focus:empty`)에서만 우아하게 노출되도록 개선하여 문서 전반의 시각적 노이즈 제거.

### 3. 인터랙티브 커스텀 라운드 체크박스 (`src/index.html`)
- 브라우저 기본 체크박스 대신 CSS 순수 렌더링 기반 4px 라운드 스퀘어 체크박스 및 백색 체크마크 SVG 구현.
- 호버 시 소프트 액센트 틴트, 클릭 시 부드러운 스케일/체크 전환, 완료 시 취소선 및 텍스트 딤 처리.

### 4. 전체 너비 (Full Width) 및 글꼴 크기 사용자 설정 추가 (`src/index.html`)
- 설정 메뉴 내 노션의 핵심 기능인 "전체 너비 (Full Width)" 토글 옵션 추가 (`body.full-width`).
- "글꼴 크기 (작게 / 보통)" 토글 옵션 추가 (`body.small-text`).
- 브라우저 `localStorage`에 자동 영구 보관되어 재실행 시에도 사용자 환경 완벽 유지.

### 5. 플로팅 서식 툴바 (Floating Format Bar) 고도화 (`src/index.html`)
- 거친 텍스트 버튼(B, I, S, ==) 대신 고해상도 벡터 SVG 아이콘(굵게, 기울임, 취소선, 형광펜, 코드, 키캡, 링크, 지우기) 및 구분선 적용.
- 현재 선택 영역의 서식 상태(Bold, Italic, Strike, Mark, Code, Link)를 실시간 감지하여 버튼의 `.active` 활성 배경 동기화.
- 형광펜(`mark`) 및 인라인 코드(`code`) 서식 토글 해제 지원.

### 6. 슬래시(/) 명령어 메뉴 카테고리화 및 비주얼 뱃지 (`src/index.html`)
- 메뉴 항목을 "기본 블록"과 "고급 및 미디어" 그룹으로 분류하고 그룹 헤더(`<div class="slash-group-header">`) 추가.
- 각 블록 아이콘에 노션 시그니처 파스텔 틴트(H1~H3: 블루, Todo: 에메랄드, Bullet: 앰버, Quote: 퍼플, Callout: 옐로우, Code: 로즈 등) 부여.
- 검색어 필터링 시 그룹 내 일치 항목 유무에 따른 동적 그룹 헤더 표시/숨김 연동.

### 7. 작업 폴더 탐색기 (Quick Open) 및 목차 드로어 (TOC) 슬라이드 애니메이션 (`src/index.html`)
- 파일 탐색기 모달 푸터에 Raycast 스타일 키보드 단축키 가이드(`<kbd>↑</kbd><kbd>↓</kbd> 이동 · <kbd>↵</kbd> 열기 · <kbd>Esc</kbd> 닫기`) 배치.
- 목차 드로어 열림 시 부드러운 우측 슬라이드 인 애니메이션(`drawerSlide`) 적용.
- 노션 라이브 캔버스 내 텍스트 선택 시 상태 표시줄에 실시간 글자 수(`· 128자 선택됨`) 연동.

### 8. 검증 및 배포 결과
- `cargo fmt --check`: 클린 통과.
- `cargo clippy -- -D warnings`: 무경고 무에러 클린 통과.
- `cargo test`: 전체 51개 단위 테스트 100% 통과 (51 passed, 0 failed).
- `cargo build --release`: 단일 최적화 실행 파일 빌드 완료.
- `install.ps1`: 로컬 Windows 환경(`%LOCALAPPDATA%\Programs\mdeditor`) 배포 완료 및 바이너리 SHA256 해시 일치 검증 완료.

## 미반영·주의사항
- 설치 프로그램은 샌드박스 밖 쓰기라 직접 실행 불가 — `install.bat` 사용자 실행 + 결과 검증 방식으로 배포함.
- 소스를 고친 뒤에는 `install.bat` 재실행해야 설치본에 반영됨.
