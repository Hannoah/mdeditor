# MDEditor 작업 기록

## 2026-11-23 — v0.12.3: 블록 지정 스크롤 선택 유지 및 목록·빈 줄·IME 편집 버그 일괄 수정

- **블록 지정 중 위로 스크롤 시 뒤쪽(시작점) 선택 끊김 수정 (`src/ui.rs`, `src/main.rs`)**:
  - 드래그 시작 좌표를 화면 고정 좌표가 아닌 `ScrollArea` 콘텐츠 상대 좌표(`drag_origin_rel`)와 시작 블록(`drag_anchor_block`) 기준으로 추적해, 자동 스크롤이나 마우스 휠로 위·아래 스크롤하며 블록을 지정해도 시작점 선택이 끊기지 않고 유지되도록 수정.
- **한글 IME 조합 플래그(`ime_composing`) 고착 해소 (`src/ui.rs`, `src/main.rs`)**:
  - 한글 입력 후 같은 블록 내 다른 줄 클릭, 한/영 전환 후 일반 문자 입력(`Event::Text`), 단일 커서(`sel.0 == sel.1`) 상태에서 `ime_composing`이 즉시 해제되도록 수정해 `Backspace`/`Delete` 및 커서 보정이 가로막히지 않도록 개선.
- **빈 목록·빈 줄 삭제 및 줄 경계 편집 정밀화 (`src/textops.rs`, `src/main.rs`, `src/ui.rs`)**:
  - 빈 목록·인용 줄 바로 아래 줄(`6. ...` 등) 맨 앞에서 `Backspace`를 누르면 현재 줄 기호를 보존한 채 위의 빈 줄만 삭제하고, 윗줄 끝에서 `Delete`를 누르면 아래 빈 목록 줄을 삭제하거나 다음 줄의 숨김 기호를 벗겨 자연스럽게 병합하도록 개선.
  - 블록 첫 줄·중간·끝의 빈 목록/인용 줄에서 `Enter`·`Backspace`·`Delete` 시 3중 줄바꿈(`\n\n\n`) 고아 빈 줄 생성을 방지하고, 블록 사이 빈 줄 슬롯 삭제 시 이웃 블록이 강제로 합쳐지지 않게(`\n\n` 유지) 수정 및 `backspace_blank_above` 추가.
  - 줄 왼쪽 여백(불릿/번호 위) 재클릭 및 `Home` 입력 시 윗줄로 커서가 튀지 않게 `snap_out_of_prefix` 적용, 단일 줄 전체 선택 후 서식(`Ctrl+B`) 적용 시 줄머리 기호 보존.
- **검증**: `drag_block_selection_while_scrolling_up_preserves_bottom_anchor` 등 회귀 테스트 추가, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`(57개 통과).

## 2026-11-23 — v0.12.2: 빈 목록 줄 커서 진입·삭제 버그 수정

- **빈 목록 줄 커서 진입 불가 수정 (`src/textops.rs`)**:
  - `hidden_runs()`가 빈 편집 가능 항목(`- `, `1. `, `- [ ] `, `# `, `> `, `:::toggle `, `[^1]: `)에서도 뒤따르는 줄바꿈(`\n`)을 숨김 구간에 삼키면서 다음 줄 접두사(예: `6. ...`)와 하나의 숨김 구간으로 합쳐지던 문제를 수정.
  - `is_collapsed_line()`을 분리해 실제로 접히는 비편집 줄(`> [!...]`, `---`, `:::`, 이미지 줄)만 줄바꿈을 포함해 숨기도록 변경하여, 번호 목록 앞의 빈 불릿 줄(`- `)에도 커서가 정확히 진입하도록 해결.
- **빈 목록 줄 `Delete` 삭제 지원 (`src/textops.rs`, `src/main.rs`)**:
  - `delete_empty_list_line()` 및 `App::delete_empty_list()`를 추가해 빈 목록 기호 줄에서 `Delete` 키를 누를 때 다음 줄의 번호·불릿 접두사를 훼손하지 않고 빈 줄만 깔끔하게 제거.
- **검증**: `empty_bullet_before_numbered_line_can_be_focused_and_deleted` 회귀 테스트 추가, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`(56개 통과).

## 2026-11-23 — v0.12.1: 불릿·연번 블록 메뉴, 목록 계층·줄바꿈 일치화 및 전반 안정성·보안 수정

- **글머리(불릿)·번호(연번) 목록 원클릭 메뉴 및 다중 블록 변환 (`src/theme.rs`, `src/ui.rs`, `src/main.rs`, `src/textops.rs`)**:
  - 전용 `BULLET_LIST`·`NUMBERED_LIST` 아이콘 및 왼쪽 서랍 모양의 `TOC` 아이콘을 추가해 목차와 불릿 아이콘을 명확히 구분.
  - 상단 툴바, 선택 영역 플로팅 서식 막대, 우클릭 컨텍스트 메뉴 상단 빠른 버튼 줄 3곳에 글머리·번호 목록 버튼 추가.
  - 여러 블록을 지정해 불릿/번호 목록으로 변환할 때 블록 사이 빈 줄을 건너뛰도록 수정하고 재클릭 토글 지원.
- **목록 줄바꿈 편집↔보기 일치화 & 숫자 아래 불릿 계층·`Shift+Tab` 개선 (`src/ui.rs`, `src/textops.rs`, `src/main.rs`)**:
  - `layout_hanging_galley`로 목록·인용·접기·각주 블록의 자동 줄바꿈 2번째 줄 이하도 보기 모드와 동일한 내어쓰기(`LIST_STEP`), 줄바꿈 폭, 줄간격(`body * 1.6`), 항목 간격(`LIST_ROW_GAP`)으로 통일하고 `Event::SoftBreak`도 줄바꿈으로 일치시킴.
  - `1. 항목` 아래 `- 하위 불릿`(빈 줄로 나뉜 블록 포함)에서 `Tab`으로 정확히 3칸 하위 계층화되고, 한글 IME 조합 중·포커스 이탈·`\t` 들여쓰기에서도 `Shift+Tab` 역들여쓰기가 안정적으로 동작하도록 개선.
- **UI 인터랙션 버그 일괄 수정 (`src/ui.rs`, `src/main.rs`)**:
  - **탭 텍스트 클릭 불가 수정**: 탭 라벨·파일명 배지·상태바 알약·이미지 드롭존의 `Label` 클릭 가로채기(`selectable_labels`)를 해제하고 탭 전체 영역 클릭 지원.
  - **블록 지정 시 스크롤 하강 버그 수정**: 비활성 블록 드래그 시작 커서를 `origin`에 배치하고 드래그 중 `scroll_to_rect` 억제, 가장자리 자동 스크롤 조건 정밀화, 캔버스 `ScrollArea`에 `DragScroll::Never` 적용.
  - **표 에디터 공백 삭제·툴바 포커스 버그 수정**: 편집 중 셀의 미저장 공백 드래프트 유지 및 직전 포커스 셀(`last_table_cell`) 기억으로 `- 행`/`- 열`/`정렬`이 선택 셀에 적용되도록 개선.
  - **인라인 캐시 충돌·우클릭 맞춤법 제안·링크 토스트 루프·충돌 다이얼로그 덮어쓰기 수정**: `MdInlineBuf::finish_cached`에 링크 URL·취소선·밑줄·정렬 해시 추가, 오타 우클릭 시 `last_sel` 즉시 갱신, `last_link_hint`로 2.5초 반복 토스트 차단, `Dialog::Conflict` 보호.
- **크래시 패닉 방어·보안 강화·크로스플랫폼 빌드 복구 (`src/spell.rs`, `src/textops.rs`, `src/mermaid.rs`, `src/main.rs`, `Cargo.toml`)**:
  - 표 구분선 `:` 패닉, 맞춤법 전각 공백(`\u{3000}`) UTF-8 패닉, Mermaid 한글 노드 ID·잘못된 hex 색상·`Note` UTF-8 패닉·`DoubleCircle` 오타·`inf`/`NaN` 파이 방어, `shift_arrow`+`leave_blank` 오프셋 보정, `canvas()` 우클릭 패닉, `spellcheck_modal` stale 범위 패닉 수정.
  - `handle_links` 외부 URL 스킴 화이트리스트(`http`/`https`/`mailto`), HTML 내보내기 `data:text/html`·`vbscript:`·제어문자 난독화 `javascript:` 차단.
  - 비-Windows 빌드용 `arboard = "3"` 의존성 추가.
- **검증**: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`(55개 통과).

## 2026-11-22 — v0.12.0: 오프라인 맞춤법 및 띄어쓰기 검사

- **마크다운 인식 오프라인 맞춤법 검사 엔진 (`src/spell.rs`)**:
  - 코드 블록·인라인 코드·수식·URL·표/링크/각주 문법 토큰을 제외한 본문 구간만 UTF-8 바이트 오프셋으로 추출(`prose_segments`)하여 검사.
  - 한국어 자주 틀리는 맞춤법·띄어쓰기 규칙(`KO_RULES`), 연속 공백·중복 단어 검사, 영어 오타 사전(`EN_TYPOS`)을 내장해 전 플랫폼에서 즉시 동작.
  - Windows(`#[cfg(windows)]`)에서는 추가 크레이트 없이 `ole32.dll`을 통한 `ISpellCheckerFactory` / `ISpellChecker` COM FFI를 연동해 OS에 설치된 언어(`en-US`, `ko-KR`) 맞춤법 검사를 함께 활용.
- **실시간 물결 밑줄·우클릭 제안·전체 검사 모달 (`src/main.rs`, `src/ui.rs`, `src/theme.rs`)**:
  - 편집 중인 블록에서 맞춤법 오류(빨강)와 띄어쓰기 오류(주황) 아래에 실시간 물결 밑줄(`paint_spell_wavy`) 표시 및 `Shift+F7`로 즉시 토글(`spellcheck` 설정 영구 저장).
  - 블록 우클릭 시 커서를 클릭 위치에 맞추고 `canvas_context_menu` 상단에 최대 3개 교정 제안 및 `사전에 무시`(`spell_ignore` 영구 저장) 제공.
  - `F7`·상단 바(`theme::SPELL`)·설정 메뉴·하단 상태바(`맞춤법`)에서 여는 `spellcheck_modal`로 문서 전체 이슈 탐색, 위치 이동, 개별/일괄 자동 교정(`apply_all_spell_fixes`), 사전 무시 지원.
- **검증 및 문서**: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`(49개 통과, `spellcheck_modal_fix_ignore_and_toggle` 포함) 및 `README.md` 갱신.

## 2026-11-21 — v0.11.2: 코드 리뷰 전 항목 수정 및 `ui.rs` 거대 함수 분해

- **Mermaid UTF-8 문자 경계 패닉 방지 (`src/mermaid.rs`)**: `to_lowercase()` 시 바이트 길이가 변하는 유니코드 문자(예: `İ`)가 포함된 라인을 슬라이싱할 때 패닉이 발생할 수 있던 문제를 `to_ascii_lowercase()`로 통일해 해결하고, `render_pie`에도 가로 스크롤(`ScrollArea::horizontal`)과 동적 범례 폭 측정을 적용.
- **크로스플랫폼 클립보드 텍스트 읽기 (`src/theme.rs`)**: 비-Windows(macOS 등)에서 `None`만 반환하던 `theme::clipboard_text()`를 `arboard::Clipboard`로 연결해 우클릭 메뉴 `붙여넣기`가 모든 OS에서 동작하도록 수정.
- **백그라운드 스크린샷 붙여넣기 탭 고정 (`src/main.rs`)**: `insert_pasted_image` 스레드에 요청 당시 탭 키를 함께 전달해, 인코딩 도중 다른 탭으로 전환해도 원래 탭에 이미지 링크가 삽입되도록 수정.
- **구조 블록 전환 안전장치 및 인라인 서식 지우기 정밀화 (`src/main.rs`, `src/textops.rs`)**:
  - 시각적 표 편집 중이나 코드·콜아웃·토글 블록에서 `Ctrl+Alt+1~7`(제목·목록·인용) 전환 시 블록 구조가 깨지지 않도록 래퍼를 먼저 해제하거나 표를 보호.
  - `Ctrl+\`(`Fmt::Clear`)가 단순 문자열 치환 대신 `inline_spans()` 파서 기반 `clear_inline_formatting()`을 사용해 `[링크](주소)`·`[[키캡]]`·중첩 서식은 벗기고 수식 곱셈(`3 * 4`)과 이스케이프(`\*`)는 보존.
- **임시 메모 저장 즉시 반영·마지막 파일 복원·하위 폴더 이미지 상대 경로 (`src/main.rs`)**:
  - 임시 메모를 파일로 저장(`save_scratch_as`)하거나 닫을 때 `draft_dirty`로 스토리지 `"draft"`/`"drafts"`를 즉시 갱신.
  - 종료 시 저장한 마지막 파일(`storage["file"]`)을 재실행 시 보조 탭으로 복원하고, 하위 폴더 문서(`sub/note.md`)에서 작업 폴더 안의 이미지를 넣을 때 중복 복사 대신 `../` 상대 경로를 계산.
- **렌더링·함수 분해 및 문서 동기화 (`src/ui.rs`, `README.md`)**:
  - `paint_block_decor`에서 화면(`clip_rect`) 밖 줄의 장식·이미지 그리기를 생략.
  - 편집 중인 블록을 그리는 `block_editor`를 860줄에서 403줄로 줄이고, `canvas`(`handle_canvas_drag`, `paint_block_selection`), `render_table_editor`(`table_editor_toolbar`), `render_block`(`render_toggle_view`, `render_code_card_view`)도 보조 메서드로 분해.
  - `README.md` 아키텍처(`src/textops.rs`, `src/sas.rs`) 및 테스트 개수(46개) 갱신.

## 2026-11-19 — v0.11.1: 폴더 스캔 비동기, 단축키 처리 분해

- **파일 목록 갱신**(새 파일·이름 변경·삭제·`Ctrl+O`)을 다른 스레드에서 한다. 큰 폴더에서도 UI가 멈추지 않는다. 세대 번호로 오래된 스캔 결과는 버린다. 시작 때 첫 파일을 정해야 할 때만 동기 스캔.
- **인쇄 임시 파일**(`mdeditor-print-<pid>.html`)은 시작할 때 10분 지난 것을 지운다.
- **`handle_shortcuts` 734줄 → 342줄**: Ctrl 조합(`shortcuts_cmd`)과 다중 블록 선택(`shortcuts_block_sel`)을 메서드로 분리, 한 프레임의 키 상태는 `Keys`로 전달.
- 남은 것: `block_editor`(860줄)는 지역 변수가 얽혀 있어 그대로, 코드 서명.

## 2026-11-19 — v0.11.0: 데이터 손실·보안 점검 일괄 수정

프로그램 전체 리뷰에서 나온 항목을 고침.
- **저장 충돌 방지**: 열어 둔 뒤 다른 프로그램이 파일을 바꿨으면 자동 저장이 덮어쓰지 않고 "내 내용으로 덮어쓰기 / 디스크 내용 불러오기 / 취소"를 묻는다(`Dialog::Conflict`). 시각만 바뀌고 내용이 같으면(클라우드 동기화) 그냥 저장. 창 복귀 때 현재 탭만이 아니라 열린 모든 탭을 확인.
- **`save_atomic`**: 프로세스별 임시 이름, 잠긴 파일 짧은 재시도(4회), 심볼릭 링크는 실제 파일에 저장.
- **HTML 내보내기·인쇄**: 문서 속 원시 HTML(`<script>`, `onerror=`)은 글자 그대로 표시, `javascript:` 링크 차단. 인쇄 임시 파일은 프로세스별 이름, `file://` 경로는 퍼센트 인코딩.
- **링크**: `%20`만 풀던 것을 전체 퍼센트 디코딩(한글·`#`·`%` 경로)으로.
- **임시 메모 여러 탭**: 종료 후에도 모든 탭 복원(`drafts`; 예전 `draft` 형식도 읽음).
- **죽기 직전 복구**: 저장 안 된 문서를 1초마다 스냅샷해 두고 패닉 때 `*.recovered.md`로 남김.
- **이미지 붙여넣기**: PNG 인코딩·저장을 다른 스레드에서(큰 스크린샷에도 멈추지 않음).
- **구조**: 순수 텍스트 연산 1,588줄을 `textops.rs`로 분리, 탭별 뷰 상태를 `TabView`로 묶음.
- **테스트 45개**: 무작위 마크다운 퍼즈(패닉·UTF-8 경계·블록 순서), HTML 무해화, 저장 충돌, 임시 메모 복원 추가.
- **위생**: `fmt`·`clippy -D warnings` 통과, GitHub Actions CI(Windows·macOS) 추가, README 배지 갱신.
- 남은 것: 폴더 스캔(최대 5000개·깊이 4)은 아직 UI 스레드, `handle_shortcuts`/`block_editor` 거대 함수 분해, 코드 서명.

## 2026-11-19 — v0.10.2: 편집 키 버그 일괄 수정 (코드 블록 언어칸 입력 등)

블록 종류 13가지 × 모든 커서 위치에서 글자 입력·Backspace·Delete·Enter를 자동으로 돌려 구조가 깨지거나 원문이 드러나는 경우를 찾아 고침 (회귀 테스트 `editing_keys_keep_markdown_structure`).
- **코드 블록 언어칸에 글자가 써지던 문제**: ``` 한 줄만 있는 상태(여는 줄을 치는 중)도 코드 카드로 그리고 그 줄을 숨겨서, 이어 친 글자가 언어로 들어갔음. 이제 줄바꿈이 생겨야(Space/Enter) 코드 카드가 됨.
- **빈 코드 블록(```` ```\n``` ````)**: 친 글자가 닫는 ``` 앞에 붙어 블록이 깨지던 것 → 닫는 줄을 지킴(`keep_code_fences`). 맨 앞 Backspace로 빈 코드·수식 블록 지우기.
- **목록 안 코드 블록**: 본문 들여쓰기 앞에 글자가 들어가던 것 → 들여쓰기 뒤부터(`code_body_range`).
- **Enter**: 굵게·링크·코드 안에서 나누면 기호를 닫고 다시 열어 두 쪽 모두 서식 유지 / 인용·알림 상자 중간은 인용 안에서 줄바꿈 / 제목·인용·알림 상자·각주 맨 앞은 위에 빈 문단(블록 유지) / 번호 목록은 번호 다시 매김(`renumber_block_at`).
- **Delete·Backspace 경계**: 구분선·이미지에 글자가 붙어 원문이 드러나던 것 → 그 블록을 지움. 뒤 블록을 당겨 올 때 `##`·`>`·`[^1]:` 기호는 떼고 내용만(여러 줄 목록·인용은 그대로).
- **블록 선택 중 글자 입력**: Shift+↑ 한 번 뒤 한 글자로 선택된 블록 전체가 사라지던 것 → 노션처럼 무시.

## 2026-11-18 — v0.10.1: 이미지도 편집 중 원문 대신 그림

- **이미지만 있는 줄(`![설명](경로)`, `image_line`)**: 편집 중에도 원문 대신 보기 화면과 같은 크기의 이미지(`image_fit`: 원래 크기, 폭을 넘으면 줄임). 원문 줄은 숨기고 그 줄 높이를 이미지 높이로 맞춰 아래 블록 위치가 그대로(테스트 `image_block_edits_like_view`, 오프스크린 스냅샷으로 확인).
- 선택된 이미지는 노션처럼 테두리로 표시, 상태 표시줄에 경로. 위에서 글자를 치면 아래 새 문단으로, Backspace는 이미지 지우기.
- 이미지는 읽어 들이는 동안 크기가 바뀌므로 편집 갤리 캐시에서 제외.

## 2026-11-18 — v0.10.0: 블록을 클릭·선택해도 마크다운 원문이 드러나지 않음 (노션식 편집 완성)

- **원칙**: 클릭·선택·편집 어느 때든 보기 화면과 같은 모양. 텍스트는 마크다운 그대로라 복사·저장은 마크다운.
- **링크·각주 참조 (`inline_spans`의 `Inline::Link`/`FootRef`)**: 편집 중 `[글자](주소)`는 글자만 링크 모양, `[^1]`은 `[1]` 배지. 커서가 링크 위면 상태 표시줄에 주소. **Ctrl+K**(서식 막대·우클릭 메뉴의 링크 포함)는 링크 대화상자(`Dialog::Link`): 새 링크 넣기 / 기존 링크 글자·주소 고치기 / 링크 해제.
- **인용**: `> ` 숨기고 보기 화면과 같은 세로줄·들여쓰기·흐린 글자. 첫 줄 맨 앞 Backspace는 인용 해제.
- **알림 상자(`> [!NOTE]`)**: 편집 중에도 같은 색 상자, 제목 줄은 상자 머리(클릭하면 종류 변경). 내용 맨 앞 Backspace는 일반 문단으로.
- **구분선**: 글자 대신 선. 높이도 보기 화면과 같고, 위에서 글자를 치면 아래 새 문단으로, Backspace는 지우기.
- **접기(`:::toggle`)**: 같은 카드 모양. 보기 화면의 펼침 상태를 공유(접혀 있으면 제목만 편집), 화살표로 펼치기/접기. 제목 맨 앞 Backspace는 접기 해제.
- **수식(`$$`)**: 같은 카드 안 가운데 정렬, `$$` 줄은 숨김.
- **각주 정의(`[^1]: `)**: 보기 화면과 같은 `[1]:` 표시와 흐린 글자.
- **코드·수식·접기 블록 빠져나가기**: 마지막 줄 ↓(문서 끝이면 새 문단), 또는 **Ctrl+Enter**로 블록 바로 아래 새 문단.
- 높이 맞춤: 수식·구분선 편집 시 TextEdit 최소 한 줄 높이를 없애 보기 화면과 픽셀 단위로 맞춤(오프스크린 스냅샷으로 확인).

## 2026-11-18 — v0.9.1: `==강조==` 보기·편집 화면 통일

- **원인**: 보기 화면은 전처리(`preprocess`)에서 `==x==`를 `**x**`로 바꿔 그려서 굵게만 보이고, 편집 화면은 굵게 + 강조 배경이었음.
- **수정 (`src/ui.rs`)**: 전처리는 길이를 보존하므로, 렌더러가 원문(`MdRenderCtx::raw`)에서 굵게 구간이 `==`로 시작하는지 확인해 강조 배경을 칠함(`MdInlineBuf::mark`). 배경색은 `mark_bg()` 하나로 두 화면이 공유. 인라인 갤리 캐시 키에 배경색 포함.

## 2026-11-18 — v0.9.0: 노션식 편집 화면(마크다운 기호 숨김), 목록 계층 들여쓰기, SAS 구문 강조

### 1. 목록 계층 들여쓰기 (`src/main.rs`)
- **원인**: Tab이 항상 공백 2칸만 넣어, 번호 목록(`1. ` → 3칸 필요) 아래에서는 하위 항목이 되지 않았음.
- **`indent_list`**: Tab은 바로 위 형제 항목의 내용 열까지(CommonMark 규칙), Shift+Tab은 부모 들여쓰기로. 하위 항목도 함께 이동하고 `renumber_lists`로 번호 재정렬(하위 목록은 1부터). 첫 항목은 들여쓰지 않음.
- 들여쓴 빈 항목에서 Enter → 한 단계 내어쓰기, 항목 맨 앞 Backspace → 내어쓰기 / 일반 문단 / 윗줄과 합치기.

### 2. 노션식 편집 화면 (`src/main.rs`, `src/ui.rs`)
- 편집 중에도 보기 화면과 같은 모양: 목록 기호(`-`, `1.`, `- [ ]`), 제목 `#`, 인라인 `**` `*` `~~` `==` `` ` ``를 폭 0·투명으로 숨기고, 글머리(• ◦ ▪)·번호·체크박스를 보기 화면과 같은 자리(`LIST_STEP` 등 공용 상수)에 그림(`paint_list_markers`).
- 텍스트는 마크다운 그대로라 복사·저장 결과도 마크다운.
- 커서가 숨긴 기호 안에 머물지 않도록 `clamp_hidden`, 숨긴 기호 옆 Backspace/Delete는 보이는 글자를 지우도록 `delete_near_hidden`. 제목 맨 앞 Backspace는 일반 문단으로.
- 제목 편집 시 위 여백을 보기 화면과 맞춰 자리 이동 없음.

### 3. 코드 블록 (`src/ui.rs`, `src/main.rs`, `src/sas.rs`)
- 편집 중 ` ``` ` 줄을 접어 숨기고(`md_job`), 커서는 코드 안에만(`code_body_range`), 펜스가 지워지면 되돌림(`keep_code_fences`). 마지막 줄에서 ↓로 빠져나감.
- **SAS 구문 강조** 추가: 주석(`/* */`, `* …;`, `%* …;`)·문자열·숫자·매크로 변수(`&x`, `&&x.`)·`%let`/`%macro`·문 키워드·PROC 이름·함수. 키워드 목록 출처: KDE syntax-highlighting `sas.xml` (MIT, Michael Walshe).

## 2026-11-16 — v0.8.0: 코드 블록 모드(` ``` `) 대폭 강화, 노션식 가운데·전체 너비·집중 보기 모드, 우클릭 메뉴 컴팩트화 및 설치 안정화

### 1. 코드 블록 모드·구문 강조 및 편집 UX 개선 (`src/main.rs`, `src/ui.rs`)
- **즉시 코드 모드 진입 및 탈출(`fence_opener_line`, `enter`, `canvas`)**:
  - 빈 블록이나 문단에서 ` ``` ` + `Space`, 또는 ` ```언어 ` / `~~~언어` + `Enter`를 입력하면 즉시 닫힌 코드 블록(`"```lang\n\n```"`)으로 확장하고 커서를 본문 줄에 배치(문단 중간에서도 앞 문단과 자동 분리).
  - 코드 본문 줄에서 `Enter` 시 현재 줄의 들여쓰기를 자동 유지하고, 닫는 ` ``` ` 줄 위/끝에서 `Enter`를 누르면 코드 블록 아래에 새 빈 블록을 생성해 빠져나옴.
- **전용 코드 블록 에디터 및 뷰어(`block_editor`, `render_block`, `md_job`, `build_code_layout_job`)**:
  - 편집 중인 코드 블록을 전용 `code_bg` 프레임으로 감싸고 상단 헤더(언어 선택 드롭다운·`복사`·`완료 (Esc)`)와 편집 중 실시간 구문 강조를 제공.
  - 읽기 모드 코드 블록 카드에 언어 배지·줄 수·`복사` 버튼과 `galley.rows` 기반 줄 번호 여백(`gutter`)을 추가하고 공통 언어 별칭(`go`, `java`, `kotlin`, `cs`, `powershell`, `zig` 등) 강조 매핑 지원.

### 2. 노션식 가운데·전체 너비·집중 보기 모드 & 빈 문서 단순화 (`src/main.rs`, `src/ui.rs`, `src/theme.rs`)
- **빈 문서 `빠른 시작` 안내 카드 제거**: 빈 문서 진입 시 불필요한 카드 없이 바로 깔끔한 캔버스에서 작성을 시작하도록 단순화.
- **가운데 ↔ 전체 너비 ↔ 집중 크게 보기**:
  - 기본 **가운데 편집** 폭을 노션 표준인 `708px`로 맞추고, 상단 헤더(`theme::WIDTH`)·하단 상태바(`"가운데"` / `"전체 너비"`)·설정 메뉴·`Ctrl+Alt+W`로 즉시 전환 가능.
  - 상단 헤더(`theme::MAXIMIZE`) 및 `F11` / `Ctrl+Shift+F`로 상·하단 바와 목차를 숨기는 **집중 크게 보기**(`focus_mode`)와 우상단 플로팅 제어 바 추가.

### 3. 우클릭 메뉴 컴팩트화, 설치 엔진 수정 및 Teams 배포 (`src/ui.rs`, `src/main.rs`, `src/setup.rs`, `src/install.ps1`, `package.ps1`)
- **본문 우클릭 메뉴 컴팩트화**: 세로 스크롤(`ScrollArea`)과 중복 메뉴를 제거해 상단 글자 서식 아이콘 바 + 핵심 8개 행(`잘라내기`·`복사`·`붙여넣기`·`전체 선택`, `블록 전환 ▸` 하위 메뉴, `삽입 ▸` 하위 메뉴, `블록 복제`·`블록 삭제`)으로 정리.
- **설치 엔진 이중 BOM 버그 수정**: `src/install.ps1`/`src/uninstall.ps1` 선두 BOM 제거, `run_script` BOM 정규화, `-LiteralPath` 경로 대응, `shell32.dll` `SHChangeNotify` 직접 호출로 설치 오류 해결.
- **메신저 공유용 zip 추가**: `package.ps1`이 `MDEditor-Setup-v<버전>.zip`을 함께 생성·검증하도록 개선(PowerShell 5.1 UTF-8 BOM 파싱 오류 방지 포함).

### 4. 한글 IME Enter 버그 수정, 표 직접 편집, 왼쪽 TOC 및 글꼴 선택 (`src/main.rs`, `src/ui.rs`, `src/theme.rs`)
- **한글 IME 조합 중 Enter 버그 수정 (`handle_shortcuts`, `enter`, `App::ui`, `App::canvas`)**:
  - Windows 한글 IME에서 `"하이"` 입력 시 `"이"` 조합 상태에서 `Enter`를 누르면 `handle_shortcuts`가 `TextEdit`의 `Ime::Commit`보다 먼저 블록을 분할하거나 조합 중인 음절 범위를 선택 삭제로 오인해 `"이"`가 다음 줄로 넘어가던 문제를 해결 (`pending_enter`로 `TextEdit` 커밋 직후 같은 프레임에서 분할 + `ime_composing` 상태 보존 및 조합 중 서식 막대 깜빡임 억제).
- **표(Table) 직접 편집 UI (`parse_md_table`, `serialize_md_table`, `render_table_editor`)**:
  - 표 블록을 클릭·삽입하면 마크다운 원문 대신 격자 표 에디터가 열려 셀 직접 입력, `+ 행`/`+ 열`/`- 행`/`- 열`, `Tab`/`Shift+Tab`/`Enter` 셀 이동(마지막 셀 `Tab`/`Enter` 시 행 자동 추가) 및 `마크다운 보기` ↔ `표 직접 편집` 전환을 지원.
- **문서 목차(TOC) 왼쪽 패널 배치 (`egui::Panel::left("toc")`)** 및 **글꼴 선택 기능 (`theme::available_fonts`, `install_fonts_with`, `font_picker_menu`)**:
  - 시스템 설치 글꼴(기본 자동, 맑은 고딕, KoPubWorld 돋움, Pretendard, 나눔고딕, 나눔명조, Noto Sans KR, D2Coding, Arial, Georgia, Consolas) 및 `.ttf`/`.otf` 파일 직접 선택·저장 지원(설정 메뉴·하단 상태바).

### 5. 표 디자인 전면 개편 및 앱 컴포넌트 UI 다듬기 (`src/ui.rs`, `src/theme.rs`, `src/main.rs`)
- **표 뷰어·에디터 통합 디자인 (`render_table_editor`, `render_table_view`, `paint_table_chrome`)**:
  - 이중 외곽선·분리된 입력 상자(`4px` 간격 둥근 박스) 대신 하나의 `8px` 라운드 외곽선 안에 `0px` 간격으로 이어지는 `1px` 실선 격자 표로 전면 재설계.
  - 캔버스 전체 폭(`table_col_width`)을 균등 분할해 비활성 읽기 뷰(`render_table_view`)와 편집 뷰(`render_table_editor`) 간 전환 시 레이아웃 점프가 없도록 통일.
  - 상단 분리형 툴바 스트립(차원 배지 `cols×rows`, `+ 행`/`+ 열`/`- 행`/`- 열`, 열별 정렬 전환 `정렬: 기본`/`왼쪽`/`가운데`/`오른쪽`, `마크다운 보기`, `완료 (Esc)`, `표 삭제` 통삭제 버튼), 헤더 행 틴트·짝수/홀수 은은한 줄무늬·포커스 셀 단독 액센트 아웃라인 적용(원문 마크다운 편집 모드 상단에도 `표 삭제` 버튼 제공).
- **전반적인 컴포넌트 디자인 폴리싱 (`theme.rs`, `ui.rs`)**:
  - `theme::pal` 및 `theme::apply`: 라이트/다크 테두리 대비·그림자·버튼 라운드(`7px`) 및 호버 상태 정돈.
  - 탭 바·TOC·상태바: 활성 탭 하단 선을 캔버스 배경과 자연스럽게 연결하고, 왼쪽 TOC 서랍 우측 경계선 및 활성 항목 액센트 막대·상태바 상단 경계선과 필 버튼을 정리.
  - 코드 블록·토글(`:::toggle`)·카드(`card`): `8px` 라운드와 일관된 헤더 스트립·복사/완료 버튼·토글 펼침 가이드 선 적용.

### 6. 굵은 글씨(`**bold**`) 렌더링 및 편집 강조 수정 (`src/theme.rs`, `src/ui.rs`, `src/main.rs`)
- **근본 원인**: `egui`의 `RichText::strong()`은 글꼴 굵기를 바꾸지 않고 색상만 조절하기 때문에, `egui_commonmark`의 `**bold**`/`__bold__`가 일반 굵기(`Proportional`)로 렌더링되던 문제와 편집 중 `md_job` 및 표 셀(`render_table_view`)에서 인라인 굵기 처리가 빠져 있던 문제를 수정.
- **`theme::bold()` 글꼴 폴백 순서 수정 (`build_fonts`)**: `&ko_bold`(`malgunbd.ttf`)를 `&regular`보다 먼저 배치해 한글이 일반 굵기로 빠지지 않도록 보완.
- **네이티브 인라인/블록 마크다운 렌더러 (`render_markdown_native`, `inline_md_view_job`, `append_inline_md_source`)**:
  - 읽기 뷰(문단·목록·할 일 체크박스·인용문·GFM 알림·제목 인라인·표 셀)와 편집 중 블록(`md_job`) 모두에서 `**굵게**`, `__굵게__`, `==강조==`에 실제 `theme::bold()`를 적용.
  - 한국어 문장부호+조사가 결합된 경우(`**굵은 글씨!**입니다`)에도 `**`가 풀리지 않고 굵게 렌더링되도록 잔여 `**...**` 쌍 처리 추가(`bold_text_uses_bold_font_in_view_edit_and_tables` 테스트 추가).

### 7. 반응속도·렌더링 캐시·증분 편집 및 네이티브 기능 최적화 (`src/main.rs`, `src/ui.rs`, `src/mermaid.rs`)
- **뷰포트 지연 가상화 및 탭 전환 캐시 유지**: `canvas`에서 미측정(`cached_h == None`)이더라도 뷰포트 아래 블록은 `estimate_block_height` + `ui.allocate_space`로 건너뛰어 대용량 문서(3,000블록) 초기 렌더링·리사이즈를 가속화하고, 탭 전환 시 `block_heights`를 보존(상한 `16_384`)하며, `toc`·`files_modal`을 `ScrollArea::show_rows`로 가상화.
- **비활성 블록 파싱·Galley 캐시 및 Mermaid 다중 캐시**:
  - `code_galley_cache`(코드 블록 `Galley`+줄 번호 좌표 재사용), `inline_galley_cache`(문단·목록·제목·표 셀 `Galley` 재사용), `table_cache`(`MdTable` 파싱 캐시)를 도입해 재페인트 비용 제거.
  - `mermaid.rs`의 1-엔트리 `FlowchartCache`를 16-엔트리 LRU 캐시로 확장하고 `sequenceDiagram`/`pie` 파싱 캐시 및 `fonts_mut` 일괄 측정 적용.
- **다중 줄 블록 증분 갱신 및 대량 치환 최적화**: `Doc::apply_block_edit`가 목록·인용구·표·닫힌 코드 블록 등 자기 완결적 다중 줄 블록도 `O(|block|)` 증분 갱신(목차 줄 번호 `item.2` 포함)하도록 확장하고, `replace_all` 단일 패스화 및 확장자·목차 스캔 무할당화 적용.
- **기능·UX 개선**: 네이티브 렌더러(`render_markdown_native`)의 각주 참조(`[1]`)·정의(`[^1]:`)·인라인/블록 수식·내부 표 지원, 로컬 `.md` 링크 앱 내 열기 및 `#제목` 앵커 스크롤 이동, `$$`/`:::toggle` 닫는 줄 `Enter` 탈출, 찾기·바꾸기 중앙 스크롤 및 바꾸기 입력창 `Enter` 실행, 슬래시 메뉴 화면 하단 반전 배치, 빈 줄 제거 후 클릭 Y보정 적용.

### 8. 검증
- `cargo test` 37개 통과(`incremental_block_edit_multiline_and_code_blocks`, `replace_all_and_heading_slug_accuracy`, `native_renderer_handles_links_footnotes_math_and_anchors`, `large_document_virtualization_and_cache_perf` 추가), `cargo clippy --all-targets -- -D warnings` 무경고 통과, `cargo fmt --check` 통과, `cargo build --release` 통과.

## 2026-11-16 — v0.7.0: 우클릭 컨텍스트 메뉴(캔버스·탭·파일 탐색기), 전방위 키보드 단축키 및 네이티브 Mermaid 그래프 렌더러 추가

### 1. 우클릭 컨텍스트 메뉴 (`src/main.rs`, `src/ui.rs`, `src/theme.rs`)
- **노션 라이브 캔버스 우클릭 메뉴(`canvas_context_menu`)**:
  - 편집 중인 블록, 렌더링된 비활성 블록(우클릭 시 자동 활성화 및 클릭 위치 커서 배치), 다중 블록 선택(`block_sel`), 캔버스 하단 빈 영역 어디서든 우클릭 시 화면 경계에 맞춰 클램핑된 컨텍스트 메뉴 표시.
  - 우클릭 시 기존 블록 내 텍스트 선택(`last_sel`)이 해제되지 않도록 보존하며, 메뉴가 열려 있을 때는 떠 있는 서식 막대와의 중첩을 방지.
  - **구성**: 메인 메뉴는 상단 인라인 글자 서식 8종 아이콘 바 + 잘라내기/복사/붙여넣기/전체 선택 + `블록 전환 ▸` 하위 메뉴(블록 배지 8종·할 일 완료 토글·코드/콜아웃/토글 전환·블록 위/아래 이동) + `삽입 ▸` 하위 메뉴(빈 블록 추가·표·Mermaid·구분선·이미지) + 블록 복제/삭제의 컴팩트 구조로 정리(실행 취소·찾기·목차·저장 등은 상단 바/단축키로 일원화).
- **상단 탭 바 우클릭 메뉴(`tab_context_menu`)**:
  - 새 탭 열기, 탭 내용 복제하여 새 탭으로 열기, 파일로 저장/이름 변경, 파일 전체 경로 복사, 탭 닫기, 다른 탭 모두 닫기, 오른쪽 탭 모두 닫기, 파일 영구 삭제 지원.
- **파일 탐색기 모달 항목 우클릭 메뉴(`file_context_menu`)**:
  - 파일 열기(`Enter`), 이름 변경(`F2`), 파일 전체 경로 복사, 파일 삭제(`Delete`) 지원.

### 2. 블록 유형 전환·편집 연산 및 신규 키보드 단축키 (`src/main.rs`, `src/ui.rs`)
- **인라인 서식 단축키 확장**: 취소선(`Ctrl+Shift+S`), 형광펜 강조(`Ctrl+Shift+H`), 인라인 코드(`Ctrl+E`), 키보드 키캡(`Ctrl+Shift+K`), 선택 영역 서식 지우기(`Ctrl+\`).
- **블록 유형 전환 단축키(`convert_block`, `toggle_todo`)**: 일반 텍스트(`Ctrl+Alt+0`), 제목 1~3(`Ctrl+Alt+1~3`), 할 일 목록(`Ctrl+Alt+4`), 글머리 목록(`Ctrl+Alt+5`), 번호 목록(`Ctrl+Alt+6`), 인용구(`Ctrl+Alt+7`), 코드 블록(`Ctrl+Alt+8`), 콜아웃 박스(`Ctrl+Alt+9`), 접기 토글(`Ctrl+Alt+G`), 할 일 체크박스 완료/미완료 토글(`Ctrl+Enter`). 이미 같은 블록 유형이면 일반 텍스트로 토글 해제.
- **블록 삽입 및 조작 단축키**: 표 삽입(`Ctrl+Alt+T`), 구분선 삽입(`Ctrl+Alt+-`), 현재 블록 아래에 새 빈 블록 추가(`Ctrl+Shift+Enter`), 현재/선택 블록 삭제(`Ctrl+Shift+D`, 마지막 블록 삭제 시 앞 줄바꿈 정리 버그 수정 포함).
- **파일·보기·복사 단축키**: 작업 폴더에 새 파일 만들기(`Ctrl+Shift+N`), OS 탐색기로 로컬 파일 열기(`Ctrl+Alt+O`), 문서 목차 토글(`Ctrl+Shift+L`), HTML 내보내기(`Ctrl+Shift+E`), 마크다운 원본 복사(`Ctrl+Shift+C`), HTML 코드 복사(`Ctrl+Alt+C`), 테마 전환(`Ctrl+Alt+D`), 전체 너비 전환(`Ctrl+Alt+W`), 글꼴 크기 전환(`Ctrl+Alt+F`).

### 3. 순수 네이티브 Mermaid 다이어그램 그래프 렌더러 (`src/mermaid.rs`, `src/ui.rs`)
- 브라우저나 외부 엔진 없이 `egui::Painter`로 직접 그리는 순수 네이티브 Mermaid 렌더러(`src/mermaid.rs`) 구현:
  - **순서도 / 방향 그래프 (`graph` / `flowchart` TD·TB·BT·LR·RL)**: 사각형(`[ ]`), 둥근 사각형(`( )`), 스타디움(`([ ])`), 서브루틴(`[[ ]]`), 원통(`[( )]`), 원/이중원(`(( ))`/`((( )))`), 마름모(`{ }`), 육각형(`{{ }}`), 평행사변형(`[/ /]`), 비대칭(`> ]`) 도형과 실선/점선/굵은선 화살표(`-->`, `-.->`, `==>`, `<-->`), 간선 라벨(`-->|텍스트|`, `-- 텍스트 -->`), 체인 연결(`A --> B --> C`), 병렬 연결(`A & B --> C`), `subgraph ... end` 그룹 박스, `style`/`classDef` 색상 지정, Sugiyama 계층 배치 및 Barycenter 교차 최소화 지원.
  - **시퀀스 다이어그램 (`sequenceDiagram`)**: `participant`/`actor` 헤더·푸터 박스, 점선 생명선(Lifeline), 실선/점선 메시지 화살표(`->>`, `-->>`, `->`, `-->`), 자기 호출(Self-Call) 루프 화살표, `Note over/left/right` 카드 렌더링.
  - **상태·클래스·ER 다이어그램 (`stateDiagram`, `classDiagram`, `erDiagram`)**: 시작/종료 상태(`[*]`) 불스아이 노드, 멤버/메서드 구획이 나뉜 클래스 박스 및 관계선 렌더링.
  - **파이 차트 (`pie`)**: 도넛형 부채꼴 폴리곤 차트 및 퍼센트 범례 렌더링.
- **편집 중 실시간 미리보기**: ` ```mermaid ` 블록을 편집할 때도 코드 입력창 바로 아래에 실시간으로 그래프 미리보기 카드를 함께 표시하고, `Esc`로 편집을 마치면 그래프 카드만 깔끔하게 표시. 단축키 `Ctrl+Alt+M` 및 우클릭 메뉴 항목 추가.

### 4. 설치·제거 마법사 및 첫 실행·편집 UX 전면 최적화 (`src/setup.rs`, `src/ui.rs`, `src/main.rs`, `src/theme.rs`)
- **설치·제거 마법사 UI/UX 개편(`src/setup.rs`)**:
  - 창 크기를 `[540.0, 480.0]`으로 확장하고 상단 헤더 상태 배지 및 3단계 진행 표시(`stepper`)를 배치.
  - 진행 중(`Page::Working`) 40ms 주기 자동 갱신(`request_repaint_after`)과 8px 진행률 막대·퍼센트·단계 체크리스트 카드 표시.
  - 완료 화면(`Page::Done`)에 `"100% 완료"` 배너, `"폴더 열기"` 포함 요약 카드, `"지금 MDEditor 실행 (Enter)"` 및 `"닫기"` / `"확인 및 닫기 (Enter)"` 버튼을 제공해 완료 상태와 다음 동작을 명확히 구분.
- **첫 실행 및 편집 사용성 개선(`src/main.rs`, `src/ui.rs`, `src/theme.rs`)**:
  - 인자 없이 실행 시 `Documents\MDEditor` 작업 폴더를 자동 생성하고, 빈 문서 진입 시 빠른 시작 액션(`QuickAction`) 카드를 표시.
  - 상단 헤더에 임시 메모 배지(`"임시 메모 · 저장"`), 실행 취소/다시 실행/표/테마(`MOON` 아이콘) 버튼 및 목차·찾기 토글 강조(`toggle_icon_btn`)를 추가.
  - 코드·Mermaid 블록 카드에 원클릭 `복사` 버튼을 추가하고, 하단 상태 표시줄(`status_bar`)에서 상태·작업 폴더·`전체 너비`/`기본 너비`·`글꼴: 보통`/`작게`를 클릭으로 전환할 수 있게 개선.

### 5. 연속 타이핑 지연·프레임 드랍 근본 해소 및 렌더링·설치 최적화 (`src/main.rs`, `src/ui.rs`, `src/theme.rs`, `src/mermaid.rs`, `src/install.ps1`)
- **OpenGL VSync 스왑 블로킹 및 워킹셋 강제 축출 제거(`src/main.rs`, `src/theme.rs`, `src/ui.rs`)**:
  - `GlowConfiguration { vsync: false, ..Default::default() }`로 설정해 이벤트 기반 타이핑 시 `SwapBuffers` VSync가 Win32 메시지 루프를 16~33ms씩 블로킹하지 않도록 개선.
  - 시작 3프레임·포커스 전환(한글 IME·창 전환)마다 메모리 매핑된 시스템 폰트 페이지를 RAM에서 강제 축출하던 `trim_working_set()`(`SetProcessWorkingSetSize(-1, -1)`)을 제거하고, `App::ui` 상단 입력 이벤트 조회를 단일 `ctx.input` 호출로 통합.
- **키 입력 경로 `O(|block|)` 증분화 및 캐시 수정(`src/main.rs`, `src/ui.rs`)**:
  - `Doc::apply_block_edit`을 도입해 단일 줄·일반 블록 타이핑 시 전체 문서 `String` 비교·복제·재분할·목차 스캔 없이 `undo` 패치, `lines`, `chars`, `blocks`, `outline`만 `O(|block|)`/`O(log B)`로 증분 갱신.
  - `block_editor`에서 `edit_buf` 버퍼 재사용, `editor_galley_cache` 도입(`DefaultHasher` + `pixels_per_point`·`wrap`·`dark`·원문 일치 검증), `active_char_cache` 갱신 버그 수정, `md_job`의 무할당 `list_marker_bytes` 적용, `split_blocks_into` 빈 블록 첫 공백(`" "`) 보존 수정.
  - `render_block` 위젯 ID를 `r.start` 바이트 오프셋에서 블록 순번 `("blk", block_idx)`으로 안정화하고, 비활성 블록 가상화 마진을 240px로 축소하며, `handle_shortcuts`를 단일 `ctx.input` 검사 고속 경로로 전환.
- **커서 반응 속도 극강화 (`src/main.rs`, `src/ui.rs`)**:
  - 비활성 블록 클릭 시 `go_click` + `block_editor` 사전 클릭 커서 계산(`editor_galley_cached`) + `TextEdit::show` 직전 조건부 포커스(`request_focus`)를 결합해, 블록 끝 점프나 다중 프레임 지연 없이 첫 활성화 프레임에 클릭한 글자 위치에 즉시 커서를 표시.
  - 블록 이동·분할·복제·편집 경로를 바이트 오프셋 기반 `pending_byte_caret` / `place_byte_caret`으로 전환해 방향키 블록 경계 이동 시 전체 문서 UTF-8 5회 중복 스캔을 제거(동일 `doc.rev`에서는 이전 블록 시작과의 차이 구간만 증분 계산).
  - 블록 내 `↑`/`↓`/`←`/`→`/`Backspace`/`Delete`/`Alt+↑↓`가 `local_cursor`로 `O(1)`/`O(|block|)` 판정되도록 개선하고, `apply_block_edit` 직후 `cursor_info.doc_rev`를 동기화해 연속 타이핑 직후에도 자동 줄바꿈(Soft-Wrap) 행 이동이 풀리지 않도록 수정.
  - `block_editor` 내 이미 포커스된 상태에서의 `ctx.memory` 조회와 선택 범위 부재 시의 우클릭 `ctx.input` 조회를 생략하고, 화면 상하단 근처 타이핑 시 커서 가시 영역 자동 스크롤(`scroll_to_rect`) 및 순수 한 줄 문단(`plain_para`) 고속 레이블 렌더링을 추가.
- **Mermaid 레이아웃 캐시 및 업데이트 복사 재시도(`src/mermaid.rs`, `src/install.ps1`)**:
  - `render_flowchart`에서 파싱·Sugiyama 계층 배치 결과를 `Arc<Graph>` 캐시(`FlowchartCache`)에 보관해 재페인트 시 레이아웃 재계산을 생략.
  - `src/install.ps1`에서 실행 중인 `mdeditor` 종료 대기 및 복사 재시도 보강.

### 6. 검증
- `cargo test` 30개 통과(`click_caret_softwrap_and_block_arrow_navigation`, `incremental_block_edit_and_leading_space_stay_consistent`, `setup_wizard_pages_render_clear_status_and_actions`, Mermaid 파서·렌더러 테스트, 블록 유형 전환·할 일 토글 테스트, 우클릭 컨텍스트 메뉴·신규 단축키 헤드리스 UI 테스트 포함), `cargo clippy --all-targets -- -D warnings` 무경고 통과, `cargo fmt --check` 통과.

## 2026-11-15 — v0.6.0: 멀티 탭 편집, 빈 줄 Enter 지원, 커서·블록 지정 UX 고도화 및 무할당 최적화

### 1. 멀티 탭 편집 (`src/main.rs`, `src/ui.rs`)
- **상단 탭 바(`tab_bar`)**: 여러 임시 메모(`새 문서`, `새 문서 2`…)와 로컬 `.md` 파일을 동시에 열어 전환·편집할 수 있는 가로 스크롤 탭 바 추가. `+` 버튼, 탭 바 빈 영역 더블클릭, `Ctrl+N`/`Ctrl+T`로 새 탭 생성, `×` 버튼·가운데 클릭·`Ctrl+W`로 탭 닫기, `Ctrl+Tab`/`Ctrl+Shift+Tab`/`Ctrl+PageUp`/`Ctrl+PageDown` 및 `Ctrl+1~9`로 탭 전환 지원.
- **탭별 뷰 상태 보존**: 각 탭(`Doc`)마다 활성 블록(`view_active`), 빈 줄 카운트(`view_blank`), 다중 블록 선택(`view_block_sel`, `view_block_sel_anchor`, `view_block_sel_head`), 마지막 커서 선택(`view_last_sel`)을 독립 보존·복원하며, 빈 보조 메모 탭이 열려 있어도 내용이 있는 메모 탭의 `draft`가 우선 보관되도록 개선.

### 2. 빈 문서·빈 블록에서 `Enter` 빈 줄 생성 및 탐색 (`src/main.rs`, `src/ui.rs`)
- **빈 줄 슬롯 모델(`collect_doc_slots_into`)**: 비어 있지 않은 블록 사이의 `\n\n` 간격과 문서 앞·뒤의 빈 줄을 `start..start` 슬롯으로 함께 구성하여, 아무 내용을 입력하지 않고 `Enter`를 여러 번 눌러도 빈 줄이 시각적으로 즉시 늘어나고 커서가 아래 줄로 내려가도록 수정.
- 빈 줄 슬롯에서도 `↑`/`↓`/`←`/`→` 커서 진입·이탈과 `Backspace`/`Delete`를 통한 빈 줄 한 줄씩 삭제가 자연스럽게 동작하도록 개선.

### 3. 커서 이동 및 다중 블록 지정 UX 개선 (`src/main.rs`, `src/ui.rs`)
- **클릭 위치 정밀 커서 배치(`pending_click_pos`)**: 비활성 블록 클릭 시 무조건 블록 끝으로 이동하던 방식 대신 `galley.cursor_from_pos`로 실제 클릭한 글자 위치에 커서를 배치.
- **시각적 줄(자동 줄바꿈) 인식 및 블록 경계 방향키(`EditorCursorInfo`, `arrow`, `arrow_lr`)**: 블록의 실제 첫 번째/마지막 레이아웃 행에 있을 때만 열(column) 위치를 보존한 채 이웃 블록으로 이동하며, 블록 시작/끝에서 `←`/`→`로 이전/다음 블록 이동, `Ctrl+Home`/`Ctrl+End`로 문서 처음/끝 이동 지원.
- **교차 블록 선택·재배치·복제(`shift_arrow`, `move_blocks`, `duplicate_blocks`)**: `Shift+방향키`, `Shift+클릭`, 마우스 드래그(상·하단 가장자리 자동 스크롤 포함)로 여러 블록을 연속 선택하고, `Tab`/`Shift+Tab` 일괄 들여쓰기·내어쓰기(편집 후 바이트 범위 자동 갱신), `Alt+↑`/`Alt+↓` 블록 순서 이동, `Ctrl+D` 블록 복제, 서식 막대 화면 경계 클램핑·반전을 지원.

### 4. 프레임 경로 무할당 및 탐색 최적화 (`src/main.rs`, `src/ui.rs`)
- `canvas_slots` 버퍼 재사용으로 캔버스 슬롯 계산 시 프레임당 힙 할당 0회 달성.
- `tab_bar`에서 `tabs` 벡터 복제를 제거하고 `tab_base_name`/`tab_title`을 `Cow<'_, str>` 무할당 빌림으로 전환, 호버 툴팁은 마우스 호버 시에만 지연 생성.
- `slot_index` 이진 탐색(`O(log N)`) 및 `active_char_cache` 기반 `block_char_bounds`/`block_byte_of`로 방향키·편집 키 입력 시 문서 전체 UTF-8 재스캔 제거.

### 5. 검증 및 배포
- `cargo test` 24개 통과(멀티 탭·빈 줄 Enter·교차 블록 선택 및 이동 UI 테스트 포함), `cargo clippy --all-targets -- -D warnings` 무경고 통과, `cargo fmt --check` 통과.

## 2026-11-13 — v0.5.0: 메모 앱 모드(새 문서 자동 보관) 및 메모리·반응 속도 극강 최적화

### 1. 메모장 / Notepad++ 스타일 새 문서(임시 메모) 모드 (`src/main.rs`, `src/ui.rs`)
- 인자 없이 앱을 실행하면 기존 파일을 자동으로 열지 않고 항상 **`새 문서`**를 즉시 열어 곧바로 메모를 시작할 수 있게 변경.
- 작성 중인 `새 문서`는 파일로 저장하지 않아도 입력 후 2초마다 및 앱 종료 시 `eframe::Storage["draft"]`에 자동 보관되어, 저장 확인 창 없이 바로 닫고 다시 열어도 그대로 유지됨.
- `새 문서` 상태에서 `Ctrl+S`(저장 버튼·파일 배지 클릭·`F2`)를 누르면 파일 이름을 입력해 실제 `.md` 파일로 저장(`Dialog::SaveScratch`)하며, `Ctrl+N`은 새 빈 메모를 시작(실행 취소 `Ctrl+Z`로 이전 메모 복원 가능).

### 2. 메모리 점유율 극강 최적화 (85.32 MB → 4.39 MB, 94.9% 절감)
- **시스템 폰트 Zero-Heap OS 메모리 매핑 (`src/theme.rs`)**: `std::fs::read`로 약 49.88 MB의 `.ttf` 폰트를 힙(`Vec<u8>`)에 복사하던 방식을 Win32 `CreateFileMappingW` + `MapViewOfFile` 읽기 전용 파일 매핑(`egui::FontData::from_blob`)으로 교체하여 폰트 힙 할당을 0 바이트로 제거. `FontDefinitions::empty()`에서 출발해 미사용 기본 내장 폰트를 제외하고 중복 볼드 폰트(`malgunbd.ttf`) 로드를 생략.
- **Working Set 자동 정리 (`src/theme.rs`, `src/ui.rs`)**: 초기 OpenGL(`glow`) 드라이버가 일회성으로 건드린 콜드 페이지를 첫 렌더링 직후(3프레임) 및 창 포커스 해제 시 `SetProcessWorkingSetSize`로 즉시 반환.
- **차분(Delta Patch) 기반 실행 취소/다시 실행 (`src/main.rs`)**: 200단계의 전체 문서 `String` 복제본 대신 바뀐 바이트 구간만 저장하는 `Patch { start, end, with }` 구조로 교체하고, 연속된 단일 문자 타이핑/삭제를 하나의 패치로 병합.
- **비활성 문서 및 레이아웃 캐시 정리 (`src/main.rs`, `src/ui.rs`)**: 다른 문서를 열 때 수정되지 않은 이전 문서는 메모리에서 내리고(`SCRATCH_KEY`는 유지), 창 폭·글꼴 크기가 바뀔 때 `block_heights` 캐시를 즉시 비움.

### 3. UI 반응 속도 및 프레임 타임 극강 최적화 (`src/main.rs`, `src/ui.rs`)
- **캔버스 Zero-Copy 렌더링**: 매 프레임 수행하던 `doc.text.clone()`, `doc.blocks.clone()`, `doc.outline.clone()`을 제거하고 `std::mem::take` 대여 및 `canvas_items`/`canvas_rects` 버퍼 재사용으로 전환.
- **Markdown 블록 무할당 렌더링**: 확장 문법(`:::`, `==`, `[[`)이 없는 블록은 `preprocess()`가 `Cow::Borrowed`를 반환하고, 체크박스(`[ ]`, `[x]`)가 없는 블록은 복제 없이 `CommonMarkViewer::show`로 즉시 렌더링.
- **메타 갱신·커서·찾기 고속화**: `split_blocks_into`와 `outline_with_bytes_into`가 기존 `Vec` 용량을 재사용하고 제목 줄에서만 `char_len`을 지연 계산. `active_block` 이진 탐색(`partition_point`), 상태바·블록 편집기 문자 오프셋 캐싱, `find_all` 대소문자 구분/ASCII 고속 경로 추가.

### 4. 검증 및 실측 결과
- `cargo test` 21개 통과, `cargo clippy --all-targets -- -D warnings` 무경고 통과, `cargo fmt --check` 통과.
- 릴리즈 실행 파일 실측 메모리: `WorkingSetPrivate` **85.32 MB → 4.39 MB (-94.9%)**, `WorkingSet64` **203.16 MB → 22.82 MB (-88.8%)**, `PrivateBytes` **129.36 MB → 100.56 MB (-22.3%)**.

## 2026-11-11 — v0.4.0: 설치 마법사 재작성, 사용감 개선, 시작 속도

### 1. 설치·제거 (메시지박스 → 앱과 같은 디자인의 마법사)
- 별도 `installer/` 크레이트(메시지박스 2개)와 `install.bat`/`uninstall.bat`(콘솔 창)을 없애고, **`mdeditor.exe` 자체가 설치 프로그램을 겸하게** 함 (`src/setup.rs`). 파일 이름에 `Setup`이 있거나 `--install`이면 설치 마법사, `--uninstall`이면 제거 마법사.
- 설치 화면: 선택 항목 4개(.md 연결 / 우클릭 메뉴 / 바탕화면 바로가기 / PATH), 진행 막대와 단계 표시, 완료 후 실행 체크, 실패 시 사유 + 다시 시도. 이미 설치돼 있으면 "업데이트"로 표시. 작업 중에는 창을 닫지 못함.
- 제거 화면: "설치된 앱"의 제거 버튼이 콘솔 대신 이 창을 띄움. 앱 설정 삭제는 선택(기본은 보존). 임시 폴더의 복사본으로 다시 실행해 설치 폴더를 깨끗이 지우고, 복사본은 스스로 삭제.
- 시스템 등록은 내장 스크립트(`src/install.ps1`, `src/uninstall.ps1`)가 숨김 PowerShell로 수행하고 `@@단계` / `@@ERR 사유` 줄로 화면에 알림.
- 안전성 수정: 실행 중인 앱을 강제 종료하던 것을 **정상 종료 요청(문서 저장) → 4초 뒤에도 안 닫히면 강제 종료**로 변경, 설치 경로의 프로세스만 대상. 제거 시 다른 프로그램이 가져간 `.md` 연결은 지우지 않음. PATH는 바뀔 때만 기록(불필요한 수 초 지연 제거).
- `build.rs`가 아이콘과 **버전 정보 리소스**를 생성(탐색기 속성·작업 관리자에 MDEditor 0.4.0 표시). 버전은 `Cargo.toml` 한 곳에서만 관리(`package.ps1`도 거기서 읽음).

### 2. 사용감
- **문서 단위 실행 취소/다시 실행**(Ctrl+Z / Ctrl+Y): 이전에는 블록 하나 안의 글자 입력만 되돌릴 수 있었음. 이제 블록 나누기·합치기, 서식, 슬래시 메뉴, 모두 바꾸기, 체크박스까지 되돌림. 이어 친 입력은 한 단계로 묶고, 빈 블록용 줄바꿈 같은 보이지 않는 단계는 건너뜀.
- 서식 단축키 토글(굵은 글자에서 Ctrl+B → 해제), Ctrl+K는 링크 생성 후 주소 자리를 선택.
- 자동 저장 알림 제거: 2초마다 "저장됨" 토스트가 뜨던 것을 상태 알약만 바뀌게 함(실패만 알림, 2초 뒤 재시도). 같은 알림이 연속으로 와도 다시 뜸(연속 Ctrl+S).
- 시작 화면: 열린 문서가 없을 때 새 문서 / 폴더 열기 / 파일 열기 버튼. 시작 메뉴에서 처음 실행하면 홈 폴더를 뒤져 아무 `.md`나 여는 대신 `문서\MDEditor`를 기본 작업 폴더로 사용.
- 외부 변경 감지: 창으로 돌아올 때 디스크의 파일이 바뀌었으면 다시 불러오고, 편집 중이던 문서는 덮어쓰지 않고 경고.
- 찾기: 선택한 글자를 검색어로, F3 / Shift+F3 이동. 파일 탐색기: 방향키로 옮긴 선택이 화면 밖이면 스크롤. 이름 변경: 확장자를 뺀 이름이 선택된 상태로 열림. 저장 확인 창에서 Enter = 저장 후 계속.
- 다크 테마에서 창 제목 표시줄도 어둡게. 설정 메뉴에 버전 표시.
- 버그 수정: 패널 구분선 자리(1px)가 칠해지지 않아 라이트 테마에서 헤더 아래·상태바 위에 진한 선이 보이던 문제.

### 3. 성능 (릴리즈, 348KB·3,000블록 문서로 측정)
- 대기 프레임 1.4ms, 입력 프레임 2.4ms, 입력 1회당 메타 갱신 0.43ms → 프레임 경로는 손대지 않음.
- 시작 속도: 열 파일이 정해져 있으면(더블클릭, 지난번 문서) 작업 폴더 스캔을 파일 탐색기를 열 때로 미룸. 블록 높이 캐시 상한 추가.

### 4. 검증
- `cargo test` 19개 통과(실행 취소·토글·설치 엔진의 숨김 PowerShell 실행 포함), `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` 통과.
- 설치 화면 4단계 × 설치/제거를 헤드리스 렌더(egui_kittest wgpu)로 확인.
- 이 PC에서 실제 실행: 무인 설치(0.3.0 → 0.4.0 업데이트, 옛 파일 정리) → 무인 제거(폴더·레지스트리·바로가기·PATH 정리, 설정 보존, 임시 파일 자동 삭제) → 재설치. 마법사 창을 마우스로 눌러 보는 확인은 사람이 해야 함.

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
- 소스를 고친 뒤에는 `package.bat` → `dist\MDEditor-Setup-v<버전>.exe` 실행(또는 `/S`)으로 설치본에 반영.
- 코드 서명 없음: 다른 PC에서 설치 파일을 처음 실행하면 SmartScreen 경고가 뜸. 판매 전 코드 서명 인증서 필요.
- 설치 엔진은 Windows PowerShell 5.1에 의존(Windows 10/11 기본 포함). PowerShell 실행이 정책으로 막힌 PC에서는 설치 실패 화면에 사유가 표시됨.
