// 맞춤법 및 띄어쓰기 검사 엔진 (완전 오프라인: 내장 한국어/영문 규칙 + Windows 기본 ISpellChecker 연동)
use std::collections::HashSet;
use std::ops::Range;

use crate::textops::{callout_kind, footnote_def, heading_level, is_hr_line, list_marker_bytes};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IssueKind {
    Spelling,
    Spacing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpellIssue {
    pub range: Range<usize>, // 입력 문자열 기준 UTF-8 바이트 구간
    pub word: String,
    pub suggestions: Vec<String>,
    pub reason: String,
    pub kind: IssueKind,
}

pub fn check_document(text: &str, ignore: &HashSet<String>) -> Vec<SpellIssue> {
    check_text(text, ignore, true)
}

pub fn check_block(block: &str, ignore: &HashSet<String>) -> Vec<SpellIssue> {
    check_text(block, ignore, false)
}

// 검사 결과에는 OS 맞춤법 검사기의 제안이 비어 있다 (느려서). 화면에 보여 줄 때만 채운다.
// ponytail: 단어별 캐시는 세션 동안 계속 커진다 — 오타 단어 수만큼이라 작다
pub fn fill_suggestions(issue: &mut SpellIssue) {
    if issue.kind != IssueKind::Spelling || !issue.suggestions.is_empty() {
        return;
    }
    #[cfg(windows)]
    {
        use std::cell::RefCell;
        use std::collections::HashMap;
        thread_local! {
            static CACHE: RefCell<HashMap<String, Vec<String>>> = RefCell::default();
        }
        issue.suggestions = CACHE.with(|c| {
            c.borrow_mut()
                .entry(issue.word.clone())
                .or_insert_with(|| win_spell::suggest(&issue.word))
                .clone()
        });
    }
}

fn check_text(text: &str, ignore: &HashSet<String>, skip_frontmatter: bool) -> Vec<SpellIssue> {
    let mut issues = Vec::new();
    for seg in prose_segments(text, skip_frontmatter) {
        let slice = &text[seg.clone()];
        check_segment(slice, seg.start, ignore, &mut issues);
    }
    issues.sort_by_key(|i| (i.range.start, i.range.end));
    dedup_overlapping(issues)
}

fn dedup_overlapping(issues: Vec<SpellIssue>) -> Vec<SpellIssue> {
    let mut out: Vec<SpellIssue> = Vec::with_capacity(issues.len());
    for issue in issues {
        if let Some(prev) = out.last_mut() {
            if issue.range.start < prev.range.end {
                // 더 구체적인 제안이 있거나 내장 규칙(Spacing/고정 제안)이면 우선한다
                if prev.suggestions.is_empty() && !issue.suggestions.is_empty() {
                    *prev = issue;
                }
                continue;
            }
        }
        out.push(issue);
    }
    out
}

// 마크다운 구문(코드 블록, 인라인 코드, 수식, URL, 링크 주소, 이미지, 구분선, 머리 기호)을 제외한
// 순수 본문 바이트 구간 목록을 추출한다.
fn prose_segments(text: &str, skip_frontmatter: bool) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut fence: Option<&str> = None;
    let mut math_fence = false;
    let mut pos = 0usize;
    let mut first_line = true;
    let mut in_frontmatter = false;

    for raw_line in text.split_inclusive('\n') {
        let line_start = pos;
        pos += raw_line.len();
        let line = raw_line.trim_end_matches(['\n', '\r']);
        let t = line.trim_start();

        if first_line {
            first_line = false;
            if skip_frontmatter && t.trim_end() == "---" {
                in_frontmatter = true;
                continue;
            }
        } else if in_frontmatter {
            if t.trim_end() == "---" || t.trim_end() == "..." {
                in_frontmatter = false;
            }
            continue;
        }

        if let Some(close) = fence {
            if t.starts_with(close) {
                fence = None;
            }
            continue;
        }
        if math_fence {
            if t.trim_end() == "$$" {
                math_fence = false;
            }
            continue;
        }
        if t.starts_with("```") {
            fence = Some("```");
            continue;
        }
        if t.starts_with("~~~") {
            fence = Some("~~~");
            continue;
        }
        if t.trim_end() == "$$" {
            math_fence = true;
            continue;
        }
        if line.trim().is_empty() || is_hr_line(line) || t.trim_end() == ":::" {
            continue;
        }
        if t.starts_with('>') && callout_kind(t).is_some() {
            continue;
        }
        // 마크다운 표 구분선 (| :--- | :---: |) 건너뛰기
        if t.starts_with('|')
            && t.bytes()
                .all(|b| matches!(b, b'|' | b'-' | b':' | b' ' | b'\t'))
        {
            continue;
        }

        let content_off = line_prose_offset(line);
        if content_off >= line.len() {
            continue;
        }
        collect_inline_prose(&line[content_off..], line_start + content_off, &mut out);
    }
    out
}

fn line_prose_offset(line: &str) -> usize {
    if let Some(m) = list_marker_bytes(line) {
        return m;
    }
    let t = line.trim_start();
    let lead = line.len() - t.len();
    if let Some(lv) = heading_level(t) {
        return (lead + lv + 1).min(line.len());
    }
    if let Some(r) = t.strip_prefix('>') {
        return lead + 1 + usize::from(r.starts_with(' '));
    }
    if let Some(r) = t.strip_prefix(":::toggle") {
        return lead + ":::toggle".len() + (r.len() - r.trim_start_matches(' ').len());
    }
    if let Some((_, m)) = footnote_def(line) {
        return m;
    }
    lead
}

fn collect_inline_prose(line: &str, base: usize, out: &mut Vec<Range<usize>>) {
    let b = line.as_bytes();
    let mut i = 0usize;
    let mut seg_start = 0usize;

    let push_seg = |s: usize, e: usize, out: &mut Vec<Range<usize>>| {
        if s < e && !line[s..e].trim().is_empty() {
            out.push((base + s)..(base + e));
        }
    };

    while i < b.len() {
        if b[i] == b'\\' && i + 1 < b.len() {
            let step = 1 + line[i + 1..].chars().next().map_or(1, char::len_utf8);
            i += step;
            continue;
        }
        // 인라인 코드 `...`
        if b[i] == b'`' {
            let run = b[i..].iter().take_while(|&&c| c == b'`').count();
            let fence = &line[i..i + run];
            if let Some(off) = line[i + run..].find(fence) {
                push_seg(seg_start, i, out);
                i = i + run + off + run;
                seg_start = i;
                continue;
            }
            i += run;
            continue;
        }
        // 인라인 수식 $...$
        if b[i] == b'$' && b.get(i + 1) != Some(&b'$') {
            if let Some(off) = line[i + 1..].find('$') {
                if off > 0 && !line[i + 1..i + 1 + off].starts_with(' ') {
                    push_seg(seg_start, i, out);
                    i = i + 2 + off;
                    seg_start = i;
                    continue;
                }
            }
        }
        // 키보드 키캡 [[Ctrl]]
        if b[i..].starts_with(b"[[") {
            if let Some(off) = line[i + 2..].find("]]") {
                if !line[i + 2..i + 2 + off].contains('[') {
                    push_seg(seg_start, i, out);
                    i = i + 4 + off;
                    seg_start = i;
                    continue;
                }
            }
        }
        // 이미지 ![alt](url) 전체 제외
        if b[i..].starts_with(b"![") {
            if let Some(close) = line[i + 2..].find(']') {
                let after = i + 2 + close + 1;
                if line[after..].starts_with('(') {
                    if let Some(rp) = line[after..].find(')') {
                        push_seg(seg_start, i, out);
                        i = after + rp + 1;
                        seg_start = i;
                        continue;
                    }
                }
            }
        }
        // 각주 [^1] 및 링크 [텍스트](URL)
        if b[i] == b'[' {
            if let Some(close) = line[i + 1..].find(']') {
                let label = &line[i + 1..i + 1 + close];
                let after = i + 2 + close;
                if label.starts_with('^') && !label.contains(char::is_whitespace) {
                    push_seg(seg_start, i, out);
                    i = after;
                    seg_start = i;
                    continue;
                }
                if !label.is_empty() && !label.contains('[') && line[after..].starts_with('(') {
                    if let Some(rp) = line[after..].find(')') {
                        push_seg(seg_start, i, out);
                        // 링크 보이는 텍스트 안은 검사하고 (URL) 부분만 건너뛴다
                        collect_inline_prose(label, base + i + 1, out);
                        i = after + rp + 1;
                        seg_start = i;
                        continue;
                    }
                }
            }
        }
        // URL (https://..., http://..., mailto:...)
        if line[i..].starts_with("http://")
            || line[i..].starts_with("https://")
            || line[i..].starts_with("mailto:")
        {
            push_seg(seg_start, i, out);
            let url_len = line[i..]
                .find(|c: char| c.is_whitespace() || matches!(c, ')' | '>' | '"' | '\''))
                .unwrap_or(line.len() - i);
            i += url_len;
            seg_start = i;
            continue;
        }
        // 마크다운 인라인 강조 기호(** __ ~~ == *)는 단어 경계로 취급해 분리한다
        if matches!(b.get(i..i + 2), Some(b"**" | b"__" | b"~~" | b"==")) {
            push_seg(seg_start, i, out);
            i += 2;
            seg_start = i;
            continue;
        }
        if b[i] == b'*' {
            push_seg(seg_start, i, out);
            i += 1;
            seg_start = i;
            continue;
        }

        i += line[i..].chars().next().map_or(1, char::len_utf8);
    }
    push_seg(seg_start, line.len(), out);
}

fn check_segment(seg: &str, base: usize, ignore: &HashSet<String>, out: &mut Vec<SpellIssue>) {
    check_korean_rules(seg, base, ignore, out);
    check_duplicate_and_spacing(seg, base, ignore, out);
    check_english_words(seg, base, ignore, out);
    #[cfg(windows)]
    win_spell::check_segment(seg, base, ignore, out);
}

fn is_ignored(word: &str, ignore: &HashSet<String>) -> bool {
    ignore.contains(word) || ignore.contains(&word.to_ascii_lowercase())
}

// ============================================================================
// 1. 내장 한국어 맞춤법·띄어쓰기 규칙 엔진
// ============================================================================

struct KoRule {
    needle: &'static str,
    replace: &'static str,
    reason: &'static str,
    kind: IssueKind,
    whole_word: bool,
}

const KO_RULES: &[KoRule] = &[
    // 되/돼
    KoRule {
        needle: "됬",
        replace: "됐",
        reason: "맞춤법: '되었'의 준말은 '됐'입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "되요",
        replace: "돼요",
        reason: "맞춤법: '되어요'의 준말은 '돼요'입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "되서",
        replace: "돼서",
        reason: "맞춤법: '되어서'의 준말은 '돼서'입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "되야",
        replace: "돼야",
        reason: "맞춤법: '되어야'의 준말은 '돼야'입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "되였다",
        replace: "되었다",
        reason: "맞춤법: '되었다'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "돼다",
        replace: "되다",
        reason: "맞춤법: 기본형은 '되다'입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "돼고",
        replace: "되고",
        reason: "맞춤법: '되고'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "돼는",
        replace: "되는",
        reason: "맞춤법: '되는'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "돼니",
        replace: "되니",
        reason: "맞춤법: '되니'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "돼면",
        replace: "되면",
        reason: "맞춤법: '되면'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "안되",
        replace: "안 돼",
        reason: "맞춤법/띄어쓰기: '안 돼'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: true,
    },
    KoRule {
        needle: "안돼다",
        replace: "안 되다",
        reason: "맞춤법/띄어쓰기: '안 되다'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "안돼고",
        replace: "안 되고",
        reason: "맞춤법/띄어쓰기: '안 되고'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "안돼는",
        replace: "안 되는",
        reason: "맞춤법/띄어쓰기: '안 되는'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "안돼면",
        replace: "안 되면",
        reason: "맞춤법/띄어쓰기: '안 되면'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    // 안/않
    KoRule {
        needle: "않되",
        replace: "안 되",
        reason: "맞춤법: 부정은 '안 되-'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "않돼",
        replace: "안 돼",
        reason: "맞춤법: 부정은 '안 돼-'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "않됨",
        replace: "안 됨",
        reason: "맞춤법: '안 됨'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "않하",
        replace: "안 하",
        reason: "맞춤법: '안 하-'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "않할",
        replace: "안 할",
        reason: "맞춤법: '안 할'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "않먹",
        replace: "안 먹",
        reason: "맞춤법: '안 먹-'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "않보",
        replace: "안 보",
        reason: "맞춤법: '안 보-'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "않좋",
        replace: "안 좋",
        reason: "맞춤법: '안 좋-'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "지 안다",
        replace: "지 않는다",
        reason: "맞춤법: '-지 않는다'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "지 안는",
        replace: "지 않는",
        reason: "맞춤법: '-지 않는'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "지 안고",
        replace: "지 않고",
        reason: "맞춤법: '-지 않고'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "지 안아",
        replace: "지 않아",
        reason: "맞춤법: '-지 않아'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "지 안았",
        replace: "지 않았",
        reason: "맞춤법: '-지 않았'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "지 안은",
        replace: "지 않은",
        reason: "맞춤법: '-지 않은'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "지 안을",
        replace: "지 않을",
        reason: "맞춤법: '-지 않을'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "지 안습",
        replace: "지 않습",
        reason: "맞춤법: '-지 않습-'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    // 어떻게/어떡해
    KoRule {
        needle: "어떡게",
        replace: "어떻게",
        reason: "맞춤법: '어떻게'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "어떻해",
        replace: "어떡해",
        reason: "맞춤법: '어떡해'('어떻게 해')가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "어떻하",
        replace: "어떡하",
        reason: "맞춤법: '어떡하-'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    // 웬/왠
    KoRule {
        needle: "왠일",
        replace: "웬일",
        reason: "맞춤법: '웬일'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "왠만",
        replace: "웬만",
        reason: "맞춤법: '웬만-'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "왠걸",
        replace: "웬걸",
        reason: "맞춤법: '웬걸'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "웬지",
        replace: "왠지",
        reason: "맞춤법: '왜인지'의 준말은 '왠지'입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    // 빈출 오타
    KoRule {
        needle: "몇일",
        replace: "며칠",
        reason: "맞춤법: 날짜·기간은 항상 '며칠'입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "금새",
        replace: "금세",
        reason: "맞춤법: '금시에'의 준말은 '금세'입니다",
        kind: IssueKind::Spelling,
        whole_word: true,
    },
    KoRule {
        needle: "역활",
        replace: "역할",
        reason: "맞춤법: '역할'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "어의없",
        replace: "어이없",
        reason: "맞춤법: '어이없-'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "오랫만",
        replace: "오랜만",
        reason: "맞춤법: '오랜만'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "어쨋든",
        replace: "어쨌든",
        reason: "맞춤법: '어쨌든'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "어쨌던",
        replace: "어쨌든",
        reason: "맞춤법: '어쨌든'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "설레임",
        replace: "설렘",
        reason: "맞춤법: '설레다'의 명사형은 '설렘'입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "설레여",
        replace: "설레어",
        reason: "맞춤법: '설레어'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "설레였",
        replace: "설레었",
        reason: "맞춤법: '설레었-'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "바래요",
        replace: "바라요",
        reason: "맞춤법: 희망의 뜻은 '바라요'입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "바랬",
        replace: "바랐",
        reason: "맞춤법: '바라다'의 과거형은 '바랐-'입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "일부로",
        replace: "일부러",
        reason: "맞춤법: '일부러'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: true,
    },
    KoRule {
        needle: "희안하",
        replace: "희한하",
        reason: "맞춤법: '희한하-'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "갯수",
        replace: "개수",
        reason: "맞춤법: 사이시옷 없이 '개수'가 표준어입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "촛점",
        replace: "초점",
        reason: "맞춤법: 사이시옷 없이 '초점'이 표준어입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "댓가",
        replace: "대가",
        reason: "맞춤법: 사이시옷 없이 '대가'가 표준어입니다",
        kind: IssueKind::Spelling,
        whole_word: true,
    },
    KoRule {
        needle: "부딛",
        replace: "부딪",
        reason: "맞춤법: '부딪히다/부딪치다'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "가르키",
        replace: "가리키",
        reason: "맞춤법: 방향·대상을 지시할 때는 '가리키-'입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "가르켰",
        replace: "가리켰",
        reason: "맞춤법: '가리켰-'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "삼가해",
        replace: "삼가",
        reason: "맞춤법: 기본형이 '삼가다'이므로 '삼가-'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "삼가하",
        replace: "삼가",
        reason: "맞춤법: '삼가-'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "일찌기",
        replace: "일찍이",
        reason: "맞춤법: '일찍이'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "깨끗히",
        replace: "깨끗이",
        reason: "맞춤법: 'ㅅ' 받침 뒤 부사는 '-이'('깨끗이')입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "뚜렷히",
        replace: "뚜렷이",
        reason: "맞춤법: 'ㅅ' 받침 뒤 부사는 '-이'('뚜렷이')입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "따뜻히",
        replace: "따뜻이",
        reason: "맞춤법: 'ㅅ' 받침 뒤 부사는 '-이'('따뜻이')입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "느긋히",
        replace: "느긋이",
        reason: "맞춤법: 'ㅅ' 받침 뒤 부사는 '-이'('느긋이')입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "번번히",
        replace: "번번이",
        reason: "맞춤법: '번번이'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "간간히",
        replace: "간간이",
        reason: "맞춤법: '간간이'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "곰곰히",
        replace: "곰곰이",
        reason: "맞춤법: '곰곰이'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "엄연이",
        replace: "엄연히",
        reason: "맞춤법: '엄연히'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "당연이",
        replace: "당연히",
        reason: "맞춤법: '당연히'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: true,
    },
    KoRule {
        needle: "무난이",
        replace: "무난히",
        reason: "맞춤법: '무난히'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: true,
    },
    KoRule {
        needle: "들어나",
        replace: "드러나",
        reason: "맞춤법: 밖으로 나타남은 '드러나다'입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "들어났",
        replace: "드러났",
        reason: "맞춤법: '드러났-'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "치뤄",
        replace: "치러",
        reason: "맞춤법: '치르다'의 활용형은 '치러'입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "치뤘",
        replace: "치렀",
        reason: "맞춤법: '치렀-'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "치루다",
        replace: "치르다",
        reason: "맞춤법: 기본형은 '치르다'입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "치루고",
        replace: "치르고",
        reason: "맞춤법: '치르고'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "치루는",
        replace: "치르는",
        reason: "맞춤법: '치르는'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "잠궈",
        replace: "잠가",
        reason: "맞춤법: '잠그다'의 활용형은 '잠가'입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "잠궜",
        replace: "잠갔",
        reason: "맞춤법: '잠갔-'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "담궈",
        replace: "담가",
        reason: "맞춤법: '담그다'의 활용형은 '담가'입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "담궜",
        replace: "담갔",
        reason: "맞춤법: '담갔-'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "갖어",
        replace: "가져",
        reason: "맞춤법: '가지다'의 활용형은 '가져'입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "갖었",
        replace: "가졌",
        reason: "맞춤법: '가졌-'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "먹을려고",
        replace: "먹으려고",
        reason: "맞춤법: '-(으)려고'가 바른 어미입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "받을려고",
        replace: "받으려고",
        reason: "맞춤법: '-(으)려고'가 바른 어미입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "할려고",
        replace: "하려고",
        reason: "맞춤법: '하려고'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "갈려고",
        replace: "가려고",
        reason: "맞춤법: '가려고'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "볼려고",
        replace: "보려고",
        reason: "맞춤법: '보려고'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "올려고",
        replace: "오려고",
        reason: "맞춤법: '오려고'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "줄려고",
        replace: "주려고",
        reason: "맞춤법: '주려고'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "될려고",
        replace: "되려고",
        reason: "맞춤법: '되려고'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "쓸려고",
        replace: "쓰려고",
        reason: "맞춤법: '쓰려고'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "알맞는",
        replace: "알맞은",
        reason: "맞춤법: 형용사 '알맞다'의 관형사형은 '알맞은'입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "걸맞는",
        replace: "걸맞은",
        reason: "맞춤법: '걸맞은'이 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "내노라",
        replace: "내로라",
        reason: "맞춤법: '내로라'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "띄여",
        replace: "띄어",
        reason: "맞춤법: '띄어'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "뛰어쓰기",
        replace: "띄어쓰기",
        reason: "맞춤법: '띄어쓰기'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "미쳐 못",
        replace: "미처 못",
        reason: "맞춤법: '아직 거기까지'의 뜻은 '미처'입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "미쳐 몰",
        replace: "미처 몰",
        reason: "맞춤법: '아직 거기까지'의 뜻은 '미처'입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "생각컨대",
        replace: "생각건대",
        reason: "맞춤법: '생각건대'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "불리우",
        replace: "불리",
        reason: "맞춤법: '불리다'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "되뇌이",
        replace: "되뇌",
        reason: "맞춤법: '되뇌다'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    KoRule {
        needle: "열락처",
        replace: "연락처",
        reason: "맞춤법: '연락처'가 바른 표기입니다",
        kind: IssueKind::Spelling,
        whole_word: false,
    },
    // 자주 틀리는 띄어쓰기
    KoRule {
        needle: "할수있",
        replace: "할 수 있",
        reason: "띄어쓰기: 의존명사 '수'는 띄어 씁니다",
        kind: IssueKind::Spacing,
        whole_word: false,
    },
    KoRule {
        needle: "할수없",
        replace: "할 수 없",
        reason: "띄어쓰기: 의존명사 '수'는 띄어 씁니다",
        kind: IssueKind::Spacing,
        whole_word: false,
    },
    KoRule {
        needle: "될수있",
        replace: "될 수 있",
        reason: "띄어쓰기: 의존명사 '수'는 띄어 씁니다",
        kind: IssueKind::Spacing,
        whole_word: false,
    },
    KoRule {
        needle: "될수없",
        replace: "될 수 없",
        reason: "띄어쓰기: 의존명사 '수'는 띄어 씁니다",
        kind: IssueKind::Spacing,
        whole_word: false,
    },
    KoRule {
        needle: "볼수있",
        replace: "볼 수 있",
        reason: "띄어쓰기: 의존명사 '수'는 띄어 씁니다",
        kind: IssueKind::Spacing,
        whole_word: false,
    },
    KoRule {
        needle: "갈수있",
        replace: "갈 수 있",
        reason: "띄어쓰기: 의존명사 '수'는 띄어 씁니다",
        kind: IssueKind::Spacing,
        whole_word: false,
    },
    KoRule {
        needle: "있을수",
        replace: "있을 수",
        reason: "띄어쓰기: 의존명사 '수'는 띄어 씁니다",
        kind: IssueKind::Spacing,
        whole_word: false,
    },
    KoRule {
        needle: "없을수",
        replace: "없을 수",
        reason: "띄어쓰기: 의존명사 '수'는 띄어 씁니다",
        kind: IssueKind::Spacing,
        whole_word: false,
    },
    KoRule {
        needle: "것같",
        replace: "것 같",
        reason: "띄어쓰기: '것 같다'는 띄어 씁니다",
        kind: IssueKind::Spacing,
        whole_word: false,
    },
    KoRule {
        needle: "거같",
        replace: "거 같",
        reason: "띄어쓰기: '거 같다'는 띄어 씁니다",
        kind: IssueKind::Spacing,
        whole_word: false,
    },
    KoRule {
        needle: "뿐만아니라",
        replace: "뿐만 아니라",
        reason: "띄어쓰기: '뿐만 아니라'는 띄어 씁니다",
        kind: IssueKind::Spacing,
        whole_word: false,
    },
    KoRule {
        needle: "다름아닌",
        replace: "다름 아닌",
        reason: "띄어쓰기: '다름 아닌'은 띄어 씁니다",
        kind: IssueKind::Spacing,
        whole_word: false,
    },
    KoRule {
        needle: "하기때문",
        replace: "하기 때문",
        reason: "띄어쓰기: 의존명사 '때문'은 앞말과 띄어 씁니다",
        kind: IssueKind::Spacing,
        whole_word: false,
    },
    KoRule {
        needle: "그렇기때문",
        replace: "그렇기 때문",
        reason: "띄어쓰기: 의존명사 '때문'은 앞말과 띄어 씁니다",
        kind: IssueKind::Spacing,
        whole_word: false,
    },
];

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric()
}

fn check_korean_rules(seg: &str, base: usize, ignore: &HashSet<String>, out: &mut Vec<SpellIssue>) {
    if !seg.chars().any(is_hangul) {
        return;
    }
    for rule in KO_RULES {
        let mut from = 0usize;
        while let Some(rel) = seg[from..].find(rule.needle) {
            let at = from + rel;
            let end = at + rule.needle.len();
            from = end;

            let prev_ch = seg[..at].chars().next_back();
            let next_ch = seg[end..].chars().next();
            if rule.whole_word
                && (prev_ch.is_some_and(is_word_char) || next_ch.is_some_and(is_word_char))
            {
                continue;
            }
            // "안되" 뒤가 "다/고/는/면..."이 아닌 정상 어절이나 "안 되어"인 경우는 건너뛴다
            if rule.needle == "됬" && prev_ch == Some('됬') {
                continue;
            }
            // 앞뒤 어절 전체를 확인해 사용자가 무시한 단어면 건너뛴다
            let ws = seg[..at]
                .rfind(|c: char| c.is_whitespace())
                .map_or(0, |i| i + seg[i..].chars().next().map_or(1, char::len_utf8));
            let we = seg[end..]
                .find(|c: char| c.is_whitespace())
                .map_or(seg.len(), |i| end + i);
            let surrounding = seg[ws..we].trim_matches(|c: char| !c.is_alphanumeric());
            if is_ignored(rule.needle, ignore) || is_ignored(surrounding, ignore) {
                continue;
            }
            out.push(SpellIssue {
                range: (base + at)..(base + end),
                word: rule.needle.to_string(),
                suggestions: vec![rule.replace.to_string()],
                reason: rule.reason.to_string(),
                kind: rule.kind,
            });
        }
    }
}

fn is_hangul(c: char) -> bool {
    matches!(c, '\u{AC00}'..='\u{D7A3}' | '\u{3131}'..='\u{318E}')
}

// ============================================================================
// 2. 문장 내 연속 공백 및 동일 단어 중복 검사
// ============================================================================

fn check_duplicate_and_spacing(
    seg: &str,
    base: usize,
    ignore: &HashSet<String>,
    out: &mut Vec<SpellIssue>,
) {
    // 어절 사이의 2칸 이상 연속 공백 (들여쓰기/줄 끝 공백 제외)
    let bytes = seg.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b' ' {
            let start = i;
            while i < bytes.len() && bytes[i] == b' ' {
                i += 1;
            }
            if i - start >= 2 && start > 0 && i < bytes.len() {
                out.push(SpellIssue {
                    range: (base + start)..(base + i),
                    word: seg[start..i].to_string(),
                    suggestions: vec![" ".to_string()],
                    reason: "띄어쓰기: 불필요한 연속 공백입니다".to_string(),
                    kind: IssueKind::Spacing,
                });
            }
        } else {
            i += seg[i..].chars().next().map_or(1, char::len_utf8);
        }
    }

    // 바로 이어지는 동일 영문/한글 단어 중복 (예: "the the", "매우 매우")
    let mut prev: Option<(usize, usize, &str)> = None;
    for (ws, we, w) in word_spans(seg) {
        if let Some((_, pe, pw)) = prev {
            let between = &seg[pe..ws];
            if !between.is_empty()
                && between.chars().all(|c| c == ' ')
                && w.chars().count() >= 2
                && pw.eq_ignore_ascii_case(w)
                && !w.chars().all(|c| c.is_ascii_digit())
                && !is_ignored(w, ignore)
            {
                out.push(SpellIssue {
                    range: (base + ws)..(base + we),
                    word: w.to_string(),
                    suggestions: vec![String::new()],
                    reason: format!("중복 단어: '{pw}'가 연속으로 두 번 쓰였습니다"),
                    kind: IssueKind::Spacing,
                });
            }
        }
        prev = Some((ws, we, w));
    }
}

fn word_spans(s: &str) -> Vec<(usize, usize, &str)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in s.char_indices() {
        if c.is_alphanumeric() || c == '\'' {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(st) = start.take() {
            let w = s[st..i].trim_matches('\'');
            if !w.is_empty() {
                let adj_s = st + (s[st..i].len() - s[st..i].trim_start_matches('\'').len());
                out.push((adj_s, adj_s + w.len(), w));
            }
        }
    }
    if let Some(st) = start {
        let w = s[st..].trim_matches('\'');
        if !w.is_empty() {
            let adj_s = st + (s[st..].len() - s[st..].trim_start_matches('\'').len());
            out.push((adj_s, adj_s + w.len(), w));
        }
    }
    out
}

// ============================================================================
// 3. 영문 빈출 오타 사전 (macOS/Linux 폴백 및 보조 제안)
// ============================================================================

const EN_TYPOS: &[(&str, &str)] = &[
    ("accomodate", "accommodate"),
    ("acheive", "achieve"),
    ("adress", "address"),
    ("alot", "a lot"),
    ("ammend", "amend"),
    ("aparrent", "apparent"),
    ("arguement", "argument"),
    ("becuase", "because"),
    ("beleive", "believe"),
    ("calender", "calendar"),
    ("catagory", "category"),
    ("changable", "changeable"),
    ("collegue", "colleague"),
    ("comming", "coming"),
    ("commited", "committed"),
    ("concious", "conscious"),
    ("definately", "definitely"),
    ("desparate", "desperate"),
    ("docuemnt", "document"),
    ("embarass", "embarrass"),
    ("enviroment", "environment"),
    ("existance", "existence"),
    ("exprot", "export"),
    ("foriegn", "foreign"),
    ("freind", "friend"),
    ("fucntion", "function"),
    ("goverment", "government"),
    ("grammer", "grammar"),
    ("happend", "happened"),
    ("harrass", "harass"),
    ("heigth", "height"),
    ("immediatly", "immediately"),
    ("improt", "import"),
    ("independant", "independent"),
    ("intial", "initial"),
    ("knowlege", "knowledge"),
    ("langauge", "language"),
    ("libary", "library"),
    ("lisence", "license"),
    ("maintainance", "maintenance"),
    ("managment", "management"),
    ("mardown", "markdown"),
    ("markdwon", "markdown"),
    ("millenium", "millennium"),
    ("mispell", "misspell"),
    ("neccessary", "necessary"),
    ("noticable", "noticeable"),
    ("occurence", "occurrence"),
    ("occured", "occurred"),
    ("paralel", "parallel"),
    ("persistant", "persistent"),
    ("posession", "possession"),
    ("prefered", "preferred"),
    ("pronounciation", "pronunciation"),
    ("publically", "publicly"),
    ("realy", "really"),
    ("recieve", "receive"),
    ("recomend", "recommend"),
    ("rediculous", "ridiculous"),
    ("refered", "referred"),
    ("relavant", "relevant"),
    ("remeber", "remember"),
    ("retrun", "return"),
    ("rythm", "rhythm"),
    ("secratary", "secretary"),
    ("seperate", "separate"),
    ("similiar", "similar"),
    ("sincerly", "sincerely"),
    ("speach", "speech"),
    ("strenght", "strength"),
    ("succesful", "successful"),
    ("suprise", "surprise"),
    ("teh", "the"),
    ("thier", "their"),
    ("tommorow", "tomorrow"),
    ("truely", "truly"),
    ("unfortunatly", "unfortunately"),
    ("untill", "until"),
    ("usefull", "useful"),
    ("vacume", "vacuum"),
    ("wheather", "whether"),
    ("wich", "which"),
    ("wierd", "weird"),
    ("writen", "written"),
    ("writting", "writing"),
];

fn match_case(template: &str, replacement: &str) -> String {
    if template.chars().all(|c| c.is_ascii_uppercase()) {
        replacement.to_ascii_uppercase()
    } else if template
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_uppercase())
    {
        let mut chars = replacement.chars();
        match chars.next() {
            Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
            None => String::new(),
        }
    } else {
        replacement.to_string()
    }
}

fn should_check_english_word(w: &str) -> bool {
    if w.len() < 2 || !w.is_ascii() {
        return false;
    }
    if w.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    // 전부 대문자 약어(API, JSON, HTTP 등)나 카멜케이스(camelCase) 식별자는 제외
    if w.chars()
        .all(|c| !c.is_alphabetic() || c.is_ascii_uppercase())
    {
        return false;
    }
    let has_inner_upper = w.chars().skip(1).any(|c| c.is_ascii_uppercase());
    if has_inner_upper {
        return false;
    }
    w.chars().any(|c| c.is_ascii_alphabetic())
}

fn check_english_words(
    seg: &str,
    base: usize,
    ignore: &HashSet<String>,
    out: &mut Vec<SpellIssue>,
) {
    for (ws, we, w) in word_spans(seg) {
        if !should_check_english_word(w) || is_ignored(w, ignore) {
            continue;
        }
        let lower = w.to_ascii_lowercase();
        if let Some(&(_, fix)) = EN_TYPOS.iter().find(|&&(bad, _)| bad == lower) {
            out.push(SpellIssue {
                range: (base + ws)..(base + we),
                word: w.to_string(),
                suggestions: vec![match_case(w, fix)],
                reason: "영문 철자 오류".to_string(),
                kind: IssueKind::Spelling,
            });
        }
    }
}

// ============================================================================
// 4. Windows OS 네이티브 맞춤법 검사기 (ISpellCheckerFactory COM FFI)
// ============================================================================

#[cfg(windows)]
mod win_spell {
    use super::*;
    use core::ffi::c_void;
    use std::cell::RefCell;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Guid {
        data1: u32,
        data2: u16,
        data3: u16,
        data4: [u8; 8],
    }

    const fn guid(d1: u32, d2: u16, d3: u16, d4: [u8; 8]) -> Guid {
        Guid {
            data1: d1,
            data2: d2,
            data3: d3,
            data4: d4,
        }
    }

    const CLSID_SPELL_CHECKER_FACTORY: Guid = guid(
        0x7ab36653,
        0x1796,
        0x484b,
        [0xbd, 0xfa, 0xe7, 0x4f, 0x1d, 0xb7, 0xc1, 0xdc],
    );
    const IID_ISPELL_CHECKER_FACTORY: Guid = guid(
        0x8e018a9d,
        0x2415,
        0x4677,
        [0xbf, 0x08, 0x79, 0x4e, 0xa6, 0x1f, 0x94, 0xbb],
    );

    #[repr(C)]
    struct IUnknownVtbl {
        query_interface:
            unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32,
        add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
    }

    #[repr(C)]
    struct ISpellCheckerFactoryVtbl {
        base: IUnknownVtbl,
        supported_languages: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32,
        is_supported: unsafe extern "system" fn(*mut c_void, *const u16, *mut i32) -> i32,
        create_spell_checker:
            unsafe extern "system" fn(*mut c_void, *const u16, *mut *mut c_void) -> i32,
    }

    #[repr(C)]
    struct ISpellCheckerVtbl {
        base: IUnknownVtbl,
        language_tag: unsafe extern "system" fn(*mut c_void, *mut *mut u16) -> i32,
        check: unsafe extern "system" fn(*mut c_void, *const u16, *mut *mut c_void) -> i32,
        suggest: unsafe extern "system" fn(*mut c_void, *const u16, *mut *mut c_void) -> i32,
        add: unsafe extern "system" fn(*mut c_void, *const u16) -> i32,
        ignore: unsafe extern "system" fn(*mut c_void, *const u16) -> i32,
        auto_correct: unsafe extern "system" fn(*mut c_void, *const u16, *const u16) -> i32,
        get_option_value: unsafe extern "system" fn(*mut c_void, *const u16, *mut u8) -> i32,
        option_ids: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32,
        id: unsafe extern "system" fn(*mut c_void, *mut *mut u16) -> i32,
        localized_name: unsafe extern "system" fn(*mut c_void, *mut *mut u16) -> i32,
        add_changed: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut u32) -> i32,
        remove_changed: unsafe extern "system" fn(*mut c_void, u32) -> i32,
        get_option_desc:
            unsafe extern "system" fn(*mut c_void, *const u16, *mut *mut c_void) -> i32,
        comprehensive_check:
            unsafe extern "system" fn(*mut c_void, *const u16, *mut *mut c_void) -> i32,
    }

    #[repr(C)]
    struct IEnumSpellingErrorVtbl {
        base: IUnknownVtbl,
        next: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32,
    }

    #[repr(C)]
    struct ISpellingErrorVtbl {
        base: IUnknownVtbl,
        start_index: unsafe extern "system" fn(*mut c_void, *mut u32) -> i32,
        length: unsafe extern "system" fn(*mut c_void, *mut u32) -> i32,
        corrective_action: unsafe extern "system" fn(*mut c_void, *mut i32) -> i32,
        replacement: unsafe extern "system" fn(*mut c_void, *mut *mut u16) -> i32,
    }

    #[repr(C)]
    struct IEnumStringVtbl {
        base: IUnknownVtbl,
        next: unsafe extern "system" fn(*mut c_void, u32, *mut *mut u16, *mut u32) -> i32,
        skip: unsafe extern "system" fn(*mut c_void, u32) -> i32,
        reset: unsafe extern "system" fn(*mut c_void) -> i32,
        clone: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32,
    }

    #[link(name = "ole32")]
    extern "system" {
        fn CoInitializeEx(pv_reserved: *mut c_void, dw_co_init: u32) -> i32;
        fn CoCreateInstance(
            rclsid: *const Guid,
            p_unk_outer: *mut c_void,
            dw_cls_context: u32,
            riid: *const Guid,
            ppv: *mut *mut c_void,
        ) -> i32;
        fn CoTaskMemFree(pv: *mut c_void);
    }

    struct ComPtr(*mut c_void);

    impl ComPtr {
        unsafe fn vtbl<T>(&self) -> &T {
            unsafe { &*(*(self.0 as *const *const T)) }
        }
    }

    impl Drop for ComPtr {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    let vtbl = &*(*(self.0 as *const *const IUnknownVtbl));
                    (vtbl.release)(self.0);
                }
            }
        }
    }

    struct WinSpellEngine {
        en: Option<ComPtr>,
        ko: Option<ComPtr>,
    }

    impl WinSpellEngine {
        fn init() -> Self {
            const COINIT_APARTMENTTHREADED: u32 = 0x2;
            const CLSCTX_INPROC_SERVER: u32 = 0x1;
            const S_OK: i32 = 0;
            const S_FALSE: i32 = 1;
            const RPC_E_CHANGED_MODE: i32 = -2147417850; // 0x80010106

            unsafe {
                let hr = CoInitializeEx(core::ptr::null_mut(), COINIT_APARTMENTTHREADED);
                if hr != S_OK && hr != S_FALSE && hr != RPC_E_CHANGED_MODE {
                    return Self { en: None, ko: None };
                }
                let mut factory_raw: *mut c_void = core::ptr::null_mut();
                let hr = CoCreateInstance(
                    &CLSID_SPELL_CHECKER_FACTORY,
                    core::ptr::null_mut(),
                    CLSCTX_INPROC_SERVER,
                    &IID_ISPELL_CHECKER_FACTORY,
                    &mut factory_raw,
                );
                if hr < 0 || factory_raw.is_null() {
                    return Self { en: None, ko: None };
                }
                let factory = ComPtr(factory_raw);
                let create_lang = |tag: &str| -> Option<ComPtr> {
                    let wide: Vec<u16> = tag.encode_utf16().chain([0]).collect();
                    let mut supported: i32 = 0;
                    let vtbl = factory.vtbl::<ISpellCheckerFactoryVtbl>();
                    if (vtbl.is_supported)(factory.0, wide.as_ptr(), &mut supported) < 0
                        || supported == 0
                    {
                        return None;
                    }
                    let mut checker: *mut c_void = core::ptr::null_mut();
                    if (vtbl.create_spell_checker)(factory.0, wide.as_ptr(), &mut checker) < 0
                        || checker.is_null()
                    {
                        return None;
                    }
                    Some(ComPtr(checker))
                };
                Self {
                    en: create_lang("en-US").or_else(|| create_lang("en")),
                    ko: create_lang("ko-KR").or_else(|| create_lang("ko")),
                }
            }
        }
    }

    thread_local! {
        static ENGINE: RefCell<Option<WinSpellEngine>> = const { RefCell::new(None) };
    }

    fn utf16_to_string_and_free(ptr: *mut u16) -> String {
        if ptr.is_null() {
            return String::new();
        }
        unsafe {
            let mut len = 0usize;
            while *ptr.add(len) != 0 {
                len += 1;
            }
            let s = String::from_utf16_lossy(core::slice::from_raw_parts(ptr, len));
            CoTaskMemFree(ptr.cast());
            s
        }
    }

    fn check_with(
        checker: &ComPtr,
        seg: &str,
        base: usize,
        is_english: bool,
        ignore: &HashSet<String>,
        out: &mut Vec<SpellIssue>,
    ) {
        let mut utf16: Vec<u16> = Vec::with_capacity(seg.len() + 1);
        let mut u16_to_u8: Vec<usize> = Vec::with_capacity(seg.len() + 1);
        for (bi, ch) in seg.char_indices() {
            let mut buf = [0u16; 2];
            for u in ch.encode_utf16(&mut buf) {
                utf16.push(*u);
                u16_to_u8.push(bi);
            }
        }
        u16_to_u8.push(seg.len());
        utf16.push(0);

        unsafe {
            let vtbl = checker.vtbl::<ISpellCheckerVtbl>();
            let mut enum_raw: *mut c_void = core::ptr::null_mut();
            if (vtbl.check)(checker.0, utf16.as_ptr(), &mut enum_raw) < 0 || enum_raw.is_null() {
                return;
            }
            let enum_err = ComPtr(enum_raw);
            let enum_vtbl = enum_err.vtbl::<IEnumSpellingErrorVtbl>();
            loop {
                let mut err_raw: *mut c_void = core::ptr::null_mut();
                let hr = (enum_vtbl.next)(enum_err.0, &mut err_raw);
                if hr != 0 || err_raw.is_null() {
                    break;
                }
                let err = ComPtr(err_raw);
                let ev = err.vtbl::<ISpellingErrorVtbl>();
                let (mut start_u16, mut len_u16, mut action) = (0u32, 0u32, 0i32);
                if (ev.start_index)(err.0, &mut start_u16) < 0
                    || (ev.length)(err.0, &mut len_u16) < 0
                    || (ev.corrective_action)(err.0, &mut action) < 0
                {
                    continue;
                }
                if action == 0 {
                    continue;
                }
                let s16 = start_u16 as usize;
                let e16 = s16.saturating_add(len_u16 as usize);
                if e16 >= u16_to_u8.len() || s16 >= e16 {
                    continue;
                }
                let sb = u16_to_u8[s16];
                let eb = u16_to_u8[e16];
                let Some(word) = seg.get(sb..eb) else {
                    continue;
                };
                if is_ignored(word, ignore) {
                    continue;
                }
                if is_english {
                    if !should_check_english_word(word) {
                        continue;
                    }
                } else if !word.chars().any(is_hangul) {
                    continue;
                }

                let mut suggestions = Vec::new();
                if action == 2 {
                    let mut rep_ptr: *mut u16 = core::ptr::null_mut();
                    if (ev.replacement)(err.0, &mut rep_ptr) >= 0 && !rep_ptr.is_null() {
                        let rep = utf16_to_string_and_free(rep_ptr);
                        if !rep.is_empty() && rep != word {
                            suggestions.push(rep);
                        }
                    }
                } else if action == 3 {
                    suggestions.push(String::new());
                }

                // 제안 목록(Suggest)은 오류 하나에 ~11ms라 여기서 받지 않는다: 보여 줄 때 fill_suggestions가 받는다
                out.push(SpellIssue {
                    range: (base + sb)..(base + eb),
                    word: word.to_string(),
                    suggestions,
                    reason: if is_english {
                        "영문 철자 오류".to_string()
                    } else {
                        "한국어 맞춤법 오류".to_string()
                    },
                    kind: IssueKind::Spelling,
                });
            }
        }
    }

    fn suggest_with(checker: &ComPtr, word: &str) -> Vec<String> {
        let mut out = Vec::new();
        let word_w: Vec<u16> = word.encode_utf16().chain([0]).collect();
        unsafe {
            let vtbl = checker.vtbl::<ISpellCheckerVtbl>();
            let mut sug_raw: *mut c_void = core::ptr::null_mut();
            if (vtbl.suggest)(checker.0, word_w.as_ptr(), &mut sug_raw) < 0 || sug_raw.is_null() {
                return out;
            }
            let sug_enum = ComPtr(sug_raw);
            let sv = sug_enum.vtbl::<IEnumStringVtbl>();
            let mut ptrs = [core::ptr::null_mut::<u16>(); 5];
            let mut fetched = 0u32;
            if (sv.next)(sug_enum.0, 5, ptrs.as_mut_ptr(), &mut fetched) >= 0 {
                for p in ptrs.iter().take(fetched as usize) {
                    let s = utf16_to_string_and_free(*p);
                    if !s.is_empty() && s != word && !out.contains(&s) {
                        out.push(s);
                    }
                }
            }
        }
        out
    }

    pub fn suggest(word: &str) -> Vec<String> {
        ENGINE.with(|cell| {
            let mut slot = cell.borrow_mut();
            let eng = slot.get_or_insert_with(WinSpellEngine::init);
            let checker = if word.chars().any(is_hangul) {
                &eng.ko
            } else {
                &eng.en
            };
            checker
                .as_ref()
                .map(|c| suggest_with(c, word))
                .unwrap_or_default()
        })
    }

    pub fn check_segment(
        seg: &str,
        base: usize,
        ignore: &HashSet<String>,
        out: &mut Vec<SpellIssue>,
    ) {
        let has_ascii_word = seg.bytes().any(|b| b.is_ascii_alphabetic());
        let has_ko = seg.chars().any(is_hangul);
        if !has_ascii_word && !has_ko {
            return;
        }
        ENGINE.with(|cell| {
            let mut slot = cell.borrow_mut();
            let eng = slot.get_or_insert_with(WinSpellEngine::init);
            if has_ascii_word {
                if let Some(en) = &eng.en {
                    check_with(en, seg, base, true, ignore, out);
                }
            }
            if has_ko {
                if let Some(ko) = &eng.ko {
                    check_with(ko, seg, base, false, ignore, out);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn korean_and_english_rules_skip_markdown_syntax_and_code() {
        let doc = "---\ntitle: teh ignored\n---\n\n# 됬습니다\n\n이건 **됬**어요 그리고 teh [구글](https://teh.com/됬) `됬다 teh` $a+b=됬$\n\n```rust\nlet teh = \"됬\";\n```\n\n할수있고  어떡게 몇일\n";
        let issues = check_document(doc, &HashSet::new());
        let words: Vec<&str> = issues.iter().map(|i| i.word.as_str()).collect();
        assert!(words.contains(&"됬"), "{words:?}");
        assert!(words.contains(&"teh"), "{words:?}");
        assert!(words.contains(&"할수있"), "{words:?}");
        assert!(words.contains(&"어떡게"), "{words:?}");
        assert!(words.contains(&"몇일"), "{words:?}");
        assert!(words.contains(&"  "), "{words:?}");
        // 코드 블록, 인라인 코드, 수식, URL, 프론트매터는 검사하지 않는다
        for issue in &issues {
            assert_eq!(&doc[issue.range.clone()], issue.word);
            assert!(!doc[..issue.range.start].ends_with("https://"));
        }
        assert!(!issues.iter().any(|i| doc[i.range.clone()].contains("rust")));
        assert!(!issues
            .iter()
            .any(|i| i.range.start < doc.find("# 됬습니다").unwrap()));

        // 무시 목록에 추가하면 제외된다
        let ignore: HashSet<String> = ["teh", "됬"].into_iter().map(str::to_string).collect();
        let filtered = check_document(doc, &ignore);
        assert!(!filtered.iter().any(|i| i.word == "teh" || i.word == "됬"));
    }

    #[test]
    fn unicode_and_duplicate_words_preserve_utf8_boundaries() {
        let s = "한글 😀 됬습니다 the the 끝";
        let issues = check_block(s, &HashSet::new());
        assert!(issues
            .iter()
            .any(|i| i.word == "됬" && i.suggestions == ["됐"]));
        assert!(issues
            .iter()
            .any(|i| i.word == "the" && i.suggestions == [""]));
        for i in issues {
            assert!(s.is_char_boundary(i.range.start) && s.is_char_boundary(i.range.end));
            assert_eq!(&s[i.range], i.word);
        }
    }
}
