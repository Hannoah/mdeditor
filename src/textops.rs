// 순수 텍스트 연산: 문자 인덱스 기준 편집, 인라인/블록 파서, 표, 목록, HTML 내보내기.
// egui에 의존하지 않아 단위 테스트와 퍼즈 테스트가 쉽다.
use std::ops::Range;

// ---------- 편집 연산 (문자 인덱스 기준, 순수 함수) ----------

pub(crate) type Sel = (usize, usize); // (시작, 끝) 문자 인덱스, 시작 <= 끝

pub(crate) fn byte_idx(s: &str, ci: usize) -> usize {
    s.char_indices().nth(ci).map_or(s.len(), |(b, _)| b)
}

pub(crate) fn char_len(s: &str) -> usize {
    s.chars().count()
}

pub(crate) fn line_start(s: &str, ci: usize) -> usize {
    let b = byte_idx(s, ci);
    s[..b].rfind('\n').map_or(0, |i| char_len(&s[..=i]))
}

pub(crate) fn line_end(s: &str, ci: usize) -> usize {
    let b = byte_idx(s, ci);
    s[b..]
        .find('\n')
        .map_or(char_len(s), |i| ci + char_len(&s[b..b + i]))
}

pub(crate) fn replace_range(s: &str, (a, b): Sel, with: &str) -> String {
    let (ba, bb) = (byte_idx(s, a), byte_idx(s, b));
    format!("{}{}{}", &s[..ba], with, &s[bb..])
}

// 선택 영역을 prefix/suffix로 감싼다. 선택이 없으면 placeholder를 넣고 선택해 둔다.
// 이미 같은 기호로 감싸져 있으면 벗긴다(토글). 링크는 바로 주소를 칠 수 있게 URL 자리를 선택한다.
pub(crate) fn wrap_selection(
    s: &str,
    (a, b): Sel,
    pre: &str,
    suf: &str,
    placeholder: &str,
) -> (String, Sel) {
    let (ba, bb) = (byte_idx(s, a), byte_idx(s, b));
    let (before, sel, after) = (&s[..ba], &s[ba..bb], &s[bb..]);
    let (pl, sl) = (char_len(pre), char_len(suf));
    if a != b {
        // 기울임(*)이 굵게(**)의 별표를 벗기지 않게 한다
        let bold_not_italic = pre == "*" && before.ends_with("**") && !before.ends_with("***");
        if before.ends_with(pre) && after.starts_with(suf) && !bold_not_italic {
            let out = format!(
                "{}{sel}{}",
                &before[..before.len() - pre.len()],
                &after[suf.len()..]
            );
            return (out, (a - pl, b - pl));
        }
        let inner_bold = pre == "*" && sel.starts_with("**") && !sel.starts_with("***");
        if sel.len() >= pre.len() + suf.len()
            && sel.starts_with(pre)
            && sel.ends_with(suf)
            && !inner_bold
        {
            let core = &sel[pre.len()..sel.len() - suf.len()];
            return (format!("{before}{core}{after}"), (a, b - pl - sl));
        }
    }
    let inner = if a == b { placeholder } else { sel };
    let out = format!("{before}{pre}{inner}{suf}{after}");
    let start = a + pl;
    let end = start + char_len(inner);
    match suf.strip_prefix("](").and_then(|u| u.strip_suffix(')')) {
        Some(url) if a != b => (out, (end + 2, end + 2 + char_len(url))),
        _ => (out, (start, end)),
    }
}

// 선택 영역에서 인라인 마크다운 서식(**굵게**, __굵게__, *기울임*, ~~취소선~~, ==강조==, `코드`, [[키]], [글자](주소))만 벗기고
// 수식 곱셈(3 * 4)이나 이스케이프된 기호(\*)는 그대로 보존한다.
pub(crate) fn clear_inline_formatting(s: &str) -> String {
    let mut lines_out = Vec::new();
    for line in s.split('\n') {
        let mut stripped = String::with_capacity(line.len());
        let mut cur = 0usize;
        for (os, bs, be, ce, kind) in inline_spans(line) {
            if kind == Inline::FootRef {
                continue;
            }
            if cur < os {
                stripped.push_str(&line[cur..os]);
            }
            let inner = &line[bs..be];
            // 중첩된 인라인 서식(예: ~~==강조==~~)도 재귀적으로 벗긴다
            if inner
                .bytes()
                .any(|b| matches!(b, b'*' | b'_' | b'~' | b'=' | b'`' | b'['))
            {
                stripped.push_str(&clear_inline_formatting(inner));
            } else {
                stripped.push_str(inner);
            }
            cur = ce;
        }
        if cur < line.len() {
            stripped.push_str(&line[cur..]);
        }
        // 키보드 키캡 [[키]] 제거 (내부에 '['나 공백 줄바꿈이 없는 쌍만)
        let mut final_line = String::with_capacity(stripped.len());
        let bytes = stripped.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] == b'\\' && i + 1 < bytes.len() {
                let step = 1 + stripped[i + 1..].chars().next().map_or(1, char::len_utf8);
                final_line.push_str(&stripped[i..i + step]);
                i += step;
                continue;
            }
            if bytes[i..].starts_with(b"[[") {
                if let Some(rel) = stripped[i + 2..].find("]]") {
                    let key_body = &stripped[i + 2..i + 2 + rel];
                    if !key_body.is_empty() && !key_body.contains('[') {
                        final_line.push_str(key_body);
                        i += 2 + rel + 2;
                        continue;
                    }
                }
            }
            let step = stripped[i..].chars().next().map_or(1, char::len_utf8);
            final_line.push_str(&stripped[i..i + step]);
            i += step;
        }
        // 선택 범위가 서식 기호 바로 안팎으로 걸친 경우 양끝의 짝 잃은 기호 정리
        let mut trimmed = final_line.as_str();
        for tok in ["**", "__", "~~", "==", "[[", "]]", "`", "*"] {
            if let Some(r) = trimmed
                .strip_prefix(tok)
                .and_then(|r| r.strip_suffix(tok))
                .filter(|r| !r.is_empty())
            {
                trimmed = r;
            }
        }
        lines_out.push(trimmed.to_string());
    }
    lines_out.join("\n")
}

// Tab / Shift+Tab: 선택된 줄 전체를 2칸 들여쓰기·내어쓰기
pub(crate) fn indent_lines(s: &str, (a, b): Sel, outdent: bool) -> (String, Sel) {
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

// 목록 줄의 (들여쓰기, 내용 시작 열). CommonMark에서 하위 항목은 부모의 "내용 시작 열"까지
// 들여써야 자식이 된다: "- a" → 2칸, "1. a" → 3칸, "10. a" → 4칸. (할 일 "- [ ] "도 2칸 기준)
pub(crate) fn list_cols(line: &str) -> Option<(usize, usize)> {
    let ind = line.len() - line.trim_start_matches(' ').len();
    let rest = &line[ind..];
    if ["- ", "* ", "+ "].iter().any(|b| rest.starts_with(b)) {
        return Some((ind, ind + 2));
    }
    let d = rest.bytes().take_while(u8::is_ascii_digit).count();
    ((1..10).contains(&d) && (rest[d..].starts_with(". ") || rest[d..].starts_with(") ")))
        .then_some((ind, ind + d + 2))
}

// 목록 줄 하나를 읽어 (깊이, 표시 번호)를 낸다. stack: 열린 상위 항목들의 (내용 열, 번호).
// 렌더러(pulldown-cmark)와 같은 규칙: 부모 내용 열 이상 들여써야 자식, 번호는 같은 목록 앞 항목 + 1.
pub(crate) fn list_item_step(
    stack: &mut Vec<(usize, Option<u64>)>,
    line: &str,
) -> Option<(usize, Option<u64>)> {
    let (ind, col) = list_cols(line)?;
    let digits = &line[ind..col - 2];
    let mut sib = None;
    while stack.last().is_some_and(|t| ind < t.0) {
        sib = stack.pop();
    }
    let num = (!digits.is_empty()).then(|| match sib {
        Some((_, Some(n))) => n + 1,
        _ => digits.parse().unwrap_or(1),
    });
    let depth = stack.len();
    stack.push((col, num));
    Some((depth, num))
}

// 코드 블록의 코드 부분(여는·닫는 ``` 줄 제외) 문자 구간. 펜스 블록이 아니면 None.
pub(crate) fn code_body_range(block: &str) -> Option<(usize, usize)> {
    let t = block.trim_start();
    if !(t.starts_with("```") || t.starts_with("~~~")) {
        return None;
    }
    let fence = &t[..3];
    let nl = block.find('\n')?;
    // 목록 안 코드 블록("  ```")은 본문도 같은 만큼 들여써 있다 → 그 들여쓰기 뒤부터
    let lead = block.len() - t.len();
    let ind = block[nl + 1..]
        .bytes()
        .take(lead)
        .take_while(|&b| b == b' ')
        .count();
    let start = char_len(&block[..=nl]) + ind;
    let last = block.rfind('\n').map_or(0, |i| i + 1);
    let end = if last > nl && block[last..].trim_start().starts_with(fence) {
        char_len(&block[..last - 1]).max(start)
    } else {
        char_len(block)
    };
    Some((start, end))
}

// 수식 블록 ($$ 줄로 감싼 여러 줄)
pub(crate) fn is_math_block(block: &str) -> bool {
    let t = block.trim_start();
    t.split('\n').next().is_some_and(|l| l.trim() == "$$") && t.contains('\n')
}

// 접기 블록 (:::toggle 제목 … :::)
pub(crate) fn is_toggle_block(block: &str) -> bool {
    block.trim_start().starts_with(":::toggle")
}

// 알림 상자 종류 ("> [!NOTE]" → "NOTE")
pub(crate) fn callout_kind(block: &str) -> Option<&str> {
    let first = block.trim_start().split('\n').next()?.trim();
    first
        .strip_prefix('>')?
        .trim()
        .strip_prefix("[!")?
        .strip_suffix(']')
}

// 구분선 줄 (---, ***, ___)
pub(crate) fn is_hr_line(line: &str) -> bool {
    let t = line.trim();
    t.len() >= 3
        && matches!(t.as_bytes()[0], b'-' | b'*' | b'_')
        && t.bytes().all(|b| b == t.as_bytes()[0])
}

// 각주 정의 줄 "[^이름]: " → (이름, 기호 바이트 길이)
pub(crate) fn footnote_def(line: &str) -> Option<(&str, usize)> {
    let rest = line.strip_prefix("[^")?;
    let close = rest.find("]:")?;
    let name = &rest[..close];
    (!name.is_empty() && !name.contains(char::is_whitespace)).then(|| {
        let m = 2 + close + 2;
        (name, m + usize::from(line[m..].starts_with(' ')))
    })
}

// 앞뒤 줄을 숨기는 블록(코드 ``` / 수식 $$ / 접기 :::toggle)의 보이는 문자 구간
pub(crate) fn fenced_body_range(block: &str) -> Option<(usize, usize)> {
    if let Some(r) = code_body_range(block) {
        return Some(r);
    }
    let (start, close) = if is_math_block(block) {
        (char_len(&block[..=block.find('\n')?]), "$$")
    } else if is_toggle_block(block) {
        let lead = block.len() - block.trim_start().len();
        let after = &block[lead + ":::toggle".len()..];
        let sp = after.len() - after.trim_start_matches(' ').len();
        (char_len(&block[..lead + ":::toggle".len() + sp]), ":::")
    } else {
        return None;
    };
    let nl = block.find('\n').unwrap_or(block.len());
    let last = block.rfind('\n').map_or(0, |i| i + 1);
    let end = if last > nl && block[last..].trim() == close {
        char_len(&block[..last - 1]).max(start)
    } else {
        char_len(block)
    };
    Some((start, end))
}

// 이미지만 있는 줄 "![설명](경로)" → 경로. 편집 화면에서 원문 대신 이미지를 그린다.
pub(crate) fn image_line(line: &str) -> Option<&str> {
    let (alt, url) = line
        .trim()
        .strip_prefix("![")?
        .strip_suffix(')')?
        .split_once("](")?;
    (!alt.contains(']') && !url.contains([')', '\n']) && !url.trim().is_empty()).then(|| url.trim())
}

// 편집 화면에서 숨기는 줄머리 기호의 바이트 길이:
// 목록 "- ", "1. ", "- [ ] " / 제목 "## " / 인용 "> " / 알림 상자 제목 줄·구분선(줄 전체) / 각주 "[^1]: " / 접기 ":::toggle "
pub(crate) fn hidden_prefix(line: &str) -> usize {
    if let Some(m) = list_marker_bytes(line) {
        return m;
    }
    let t = line.trim_start();
    let lead = line.len() - t.len();
    if let Some(lv) = heading_level(t) {
        return (lead + lv + 1).min(line.len());
    }
    if let Some(r) = t.strip_prefix('>') {
        if callout_kind(t).is_some() {
            return line.len();
        }
        return lead + 1 + usize::from(r.starts_with(' '));
    }
    if is_hr_line(line) || t.trim_end() == ":::" || image_line(line).is_some() {
        return line.len();
    }
    if let Some(r) = t.strip_prefix(":::toggle") {
        return lead + ":::toggle".len() + (r.len() - r.trim_start_matches(' ').len());
    }
    footnote_def(line).map_or(0, |(_, m)| m)
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Inline {
    Code,
    Bold,
    Mark,
    Strike,
    Italic,
    Link,    // [글자](주소) — 글자만 보임
    FootRef, // [^1] — "^"만 숨겨 [1] 배지로 보임
}

// 줄 안의 인라인 서식 (**굵게**, __굵게__, *기울임*, ~~취소선~~, ==강조==, `코드`):
// (여는 기호 시작, 내용 시작, 내용 끝, 닫는 기호 끝, 종류) 바이트 위치.
// 편집 화면 서식(append_inline_md_source)과 커서 이동(hidden_runs)이 함께 쓴다.
pub(crate) fn inline_spans(line: &str) -> Vec<(usize, usize, usize, usize, Inline)> {
    let b = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < b.len() {
        if b[i] == b'\\' && i + 1 < b.len() {
            i += 1;
            i += line[i..].chars().next().map_or(1, char::len_utf8);
            continue;
        }
        if b[i] == b'`' {
            let run = b[i..].iter().take_while(|&&c| c == b'`').count();
            let fence = &line[i..i + run];
            let bs = i + run;
            if let Some(off) = line[bs..].find(fence) {
                if off > 0 && !line[bs..bs + off].contains('\n') {
                    out.push((i, bs, bs + off, bs + off + run, Inline::Code));
                    i = bs + off + run;
                    continue;
                }
            }
            i += run;
            continue;
        }
        if b[i] == b'[' && !(i > 0 && b[i - 1] == b'!') {
            if let Some(close) = line[i + 1..].find(']') {
                let label = &line[i + 1..i + 1 + close];
                let after = i + 2 + close; // "]" 다음
                if let Some(name) = label.strip_prefix('^') {
                    if !name.is_empty()
                        && !name.contains(char::is_whitespace)
                        && !line[after..].starts_with(':')
                    {
                        out.push((i + 1, i + 2, after - 1, after - 1, Inline::FootRef));
                        i = after;
                        continue;
                    }
                } else if !label.is_empty()
                    && !label.contains('[')
                    && line[after..].starts_with('(')
                {
                    if let Some(rp) = line[after..].find(')') {
                        out.push((i, i + 1, after - 1, after + rp + 1, Inline::Link));
                        i = after + rp + 1;
                        continue;
                    }
                }
            }
        }
        let two = b.get(i..i + 2);
        if matches!(two, Some(b"**" | b"__" | b"~~" | b"==")) {
            let mark = &line[i..i + 2];
            let prev_alnum = mark == "__"
                && line[..i]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_alphanumeric);
            let inner = &line[i + 2..];
            if !prev_alnum && !inner.starts_with(char::is_whitespace) {
                if let Some(off) = inner.find(mark) {
                    let body = &inner[..off];
                    if !body.is_empty()
                        && !body.contains('\n')
                        && !body.ends_with(char::is_whitespace)
                    {
                        let kind = match mark {
                            "**" | "__" => Inline::Bold,
                            "==" => Inline::Mark,
                            _ => Inline::Strike,
                        };
                        out.push((i, i + 2, i + 2 + off, i + 4 + off, kind));
                        i += 4 + off;
                        continue;
                    }
                }
            }
            i += 2;
            continue;
        }
        if b[i] == b'*' {
            let inner = &line[i + 1..];
            if !inner.is_empty() && !inner.starts_with(char::is_whitespace) {
                let mut from = 0usize;
                let mut end = None;
                while let Some(rel) = inner[from..].find('*') {
                    let pos = from + rel;
                    if inner[pos..].starts_with("**") {
                        from = pos
                            + inner.as_bytes()[pos..]
                                .iter()
                                .take_while(|&&c| c == b'*')
                                .count();
                        continue;
                    }
                    let cand = &inner[..pos];
                    if !cand.is_empty()
                        && !cand.contains('\n')
                        && !cand.ends_with(char::is_whitespace)
                    {
                        end = Some(pos);
                    }
                    break;
                }
                if let Some(off) = end {
                    out.push((i, i + 1, i + 1 + off, i + 2 + off, Inline::Italic));
                    i += 2 + off;
                    continue;
                }
            }
        }
        i += line[i..].chars().next().map_or(1, char::len_utf8);
    }
    out
}

// 줄에서 인라인 서식을 읽기 시작하는 바이트 위치 (md_job과 같은 규칙).
// 제목·구분선·접기 제목/닫는 줄·알림 상자 제목 줄은 인라인 서식 없음(None).
pub(crate) fn inline_start(line: &str) -> Option<usize> {
    let t = line.trim_start();
    if heading_level(t).is_some()
        || is_hr_line(line)
        || t.starts_with(":::")
        || (t.starts_with('>') && callout_kind(t).is_some())
    {
        return None;
    }
    Some(hidden_prefix(line))
}

// 블록 안 인라인 서식들의 문자 위치 [여는 시작, 내용 시작, 내용 끝, 닫는 끝]과 종류. 코드·수식 블록은 없음.
pub(crate) fn block_inline_spans(block: &str) -> Vec<([usize; 4], Inline)> {
    let mut out = Vec::new();
    if is_math_block(block) {
        return out;
    }
    let (mut ci, mut fence) = (0, false);
    for line in block.split('\n') {
        let t = line.trim_start();
        let is_fence = t.starts_with("```") || t.starts_with("~~~");
        if is_fence || fence {
            fence ^= is_fence;
        } else if let Some(st) = inline_start(line) {
            let base = ci + char_len(&line[..st]);
            let rest = &line[st..];
            let cc = |b: usize| base + char_len(&rest[..b]);
            for (a, bs, be, e, k) in inline_spans(rest) {
                out.push(([cc(a), cc(bs), cc(be), cc(e)], k));
            }
        }
        ci += char_len(line) + 1;
    }
    out
}

// 편집 화면에서 숨긴 기호 구간(문자 위치): (시작, 끝, 줄머리 기호인지). 붙어 있는 줄머리 구간은 하나로 합친다.
pub(crate) fn hidden_runs(block: &str) -> Vec<(usize, usize, bool)> {
    let mut pre_runs: Vec<(usize, usize, bool)> = Vec::new();
    if !is_math_block(block) {
        let lines: Vec<&str> = block.split('\n').collect();
        let (mut ci, mut fence) = (0, false);
        for (i, line) in lines.iter().enumerate() {
            let t = line.trim_start();
            let is_fence = t.starts_with("```") || t.starts_with("~~~");
            if is_fence || fence {
                fence ^= is_fence;
            } else {
                let pre = hidden_prefix(line);
                if pre > 0 {
                    let mut e = ci + char_len(&line[..pre]);
                    if pre == line.len() && i + 1 < lines.len() {
                        e += 1; // 줄 전체를 숨기면 줄바꿈도 함께 (접힌 줄에 커서가 서지 않게)
                    }
                    match pre_runs.last_mut() {
                        Some(last) if last.1 == ci => last.1 = e,
                        _ => pre_runs.push((ci, e, true)),
                    }
                }
            }
            ci += char_len(line) + 1;
        }
    }
    let mut out = pre_runs;
    for ([a, bs, be, e], _) in block_inline_spans(block) {
        out.push((a, bs, false));
        if be < e {
            out.push((be, e, false));
        }
    }
    out
}

// 편집 화면에서 커서가 갈 수 있는 블록 안 범위 (숨긴 줄머리 기호·코드 펜스 바깥)
pub(crate) fn visible_bounds(block: &str) -> (usize, usize) {
    fenced_body_range(block).unwrap_or_else(|| {
        let start = hidden_runs(block)
            .iter()
            .find(|r| r.2 && r.0 == 0)
            .map_or(0, |r| r.1);
        (start, char_len(block))
    })
}

// 커서가 숨긴 기호 안에 들어가면 보이는 자리로 옮긴다.
// 줄머리 기호: 내용 시작으로 (내용 시작에서 왼쪽으로 온 거라면 윗줄 끝으로).
// 인라인 기호: 움직이던 방향의 끝으로, 화면상 제자리걸음이면 한 글자 더 간다.
pub(crate) fn clamp_hidden(s: &str, c: usize, prev: Option<usize>) -> usize {
    let runs = hidden_runs(s);
    let len = char_len(s);
    let mut c = c;
    for _ in 0..4 {
        if let Some(&(a, e, _)) = runs.iter().find(|&&(a, e, pre)| pre && c >= a && c < e) {
            c = if prev == Some(e) && a > 0 { a - 1 } else { e };
            continue;
        }
        if let Some(&(a, e, _)) = runs.iter().find(|&&(a, e, pre)| !pre && c > a && c < e) {
            c = if prev.is_some_and(|p| p >= e) { a } else { e };
            continue;
        }
        let stuck = prev.filter(|&p| p != c).and_then(|p| {
            runs.iter()
                .find(|&&(a, e, pre)| !pre && (a..=e).contains(&c) && (a..=e).contains(&p))
                .map(|&(a, e, _)| {
                    if c > p {
                        (e + 1).min(len)
                    } else {
                        a.saturating_sub(1)
                    }
                })
        });
        match stuck {
            Some(n) if n != c => c = n,
            _ => break,
        }
    }
    c
}

// 숨긴 인라인 기호 옆 Backspace/Delete: 화면에 보이는 이웃 글자를 지운다 (내용이 한 글자뿐이면 서식째).
// Some(Ok((블록, 커서))) = 직접 처리, Some(Err(c)) = 커서만 c로 옮기고 기본 동작에 맡김.
pub(crate) fn delete_near_hidden(
    block: &str,
    c: usize,
    back: bool,
) -> Option<Result<(String, usize), usize>> {
    for ([os, bs, be, ce], kind) in block_inline_spans(block) {
        if kind == Inline::FootRef {
            // [^1] 배지는 한 덩어리: 바로 뒤 Backspace·바로 앞 Delete로 통째로 지운다
            let (whole_a, whole_e) = (os - 1, ce + 1);
            if (back && c == whole_e) || (!back && c == whole_a) {
                return Some(Ok((replace_range(block, (whole_a, whole_e), ""), whole_a)));
            }
            continue;
        }
        let (at_start, at_end) = (c == os || c == bs, c == be || c == ce);
        let target = match (back, at_start, at_end) {
            (true, _, true) => be - 1, // 뒤에서 Backspace → 내용 마지막 글자
            (false, true, _) => bs,    // 앞에서 Delete → 내용 첫 글자
            (true, true, _) => return (c != os).then_some(Err(os)),
            (false, _, true) => return (c != ce).then_some(Err(ce)),
            _ => continue,
        };
        if be - bs == 1 {
            return Some(Ok((replace_range(block, (os, ce), ""), os)));
        }
        return Some(Ok((replace_range(block, (target, target + 1), ""), target)));
    }
    None
}

// 목록·제목 내용 맨 앞에서 Backspace (노션식): 제목은 일반 문단으로, 들여쓴 항목은 한 단계 내어쓰기,
// 블록 첫 항목은 기호만 지워 일반 문단으로, 그 밖의 항목은 윗줄과 합친다.
pub(crate) fn backspace_list_marker(s: &str, c: usize) -> Option<(String, usize)> {
    let ls = line_start(s, c);
    let line = &s[byte_idx(s, ls)..byte_idx(s, line_end(s, c))];
    let m = hidden_prefix(line);
    if m == 0 || c != ls + char_len(&line[..m]) {
        return None;
    }
    let t = line.trim_start();
    let lb = byte_idx(s, ls);
    let bstart = s[..lb].rfind("\n\n").map_or(0, |i| i + 2);
    let bend = s[lb..].find("\n\n").map_or(s.len(), |i| lb + i);
    if t.starts_with('>') {
        // 인용·알림 상자의 첫 줄 맨 앞 → 블록 전체를 일반 문단으로 / 그 밖의 줄 → 윗줄과 합치기
        let block = &s[bstart..bend];
        let after_title =
            callout_kind(block).is_some() && !s[bstart..lb.saturating_sub(1)].contains('\n');
        if lb == bstart || after_title {
            let mut lines: Vec<&str> = block.split('\n').collect();
            if callout_kind(block).is_some() {
                lines.remove(0);
            }
            let plain: Vec<&str> = lines
                .iter()
                .map(|l| {
                    l.trim_start()
                        .strip_prefix('>')
                        .map_or(*l, |r| r.strip_prefix(' ').unwrap_or(r))
                })
                .collect();
            let out = format!("{}{}{}", &s[..bstart], plain.join("\n"), &s[bend..]);
            return Some((out, char_len(&s[..bstart])));
        }
        return Some((replace_range(s, (ls - 1, c), ""), ls - 1));
    }
    if t.starts_with(":::toggle") {
        // 접기 제목 맨 앞 → 접기를 풀어 일반 문단으로 (닫는 ::: 줄도 함께 지움)
        let block = &s[bstart..bend];
        let body = block
            .trim_end()
            .strip_suffix(":::")
            .map_or(block, |b| b.trim_end_matches('\n'));
        let body = body
            .get(block.len() - block.trim_start().len() + hidden_prefix(t)..)
            .unwrap_or("");
        let out = format!("{}{}{}", &s[..bstart], body, &s[bend..]);
        return Some((out, char_len(&s[..bstart])));
    }
    if list_marker_bytes(line).is_none() {
        return Some((replace_range(s, (ls, c), ""), ls)); // 제목·구분선·각주 → 문단
    }
    if line.starts_with(' ') {
        return indent_list(s, (c, c), true).map(|(t, sel)| (t, sel.0));
    }
    let first = ls == 0 || s[..byte_idx(s, ls)].ends_with("\n\n");
    if first {
        Some((replace_range(s, (ls, ls + m), ""), ls))
    } else {
        Some((replace_range(s, (ls - 1, ls + m), ""), ls - 1))
    }
}

// 코드 블록 편집이 숨긴 ``` 줄을 건드렸으면 그 부분만 되돌린다 (코드 부분의 변경은 유지).
// 반환: (고친 블록, 커서 문자 위치). 펜스가 멀쩡하면 None.
pub(crate) fn keep_code_fences(old: &str, new: &str) -> Option<(String, usize)> {
    let (s0, e0) = fenced_body_range(old)?;
    // 빈 블록("```\n```")에서 친 글자는 닫는 줄 앞에 붙는다 → 닫는 ```를 다음 줄로 돌려놓는다
    let close = &old[old.rfind('\n').map_or(old.len(), |i| i + 1)..];
    if s0 == e0 && ["```", "~~~", "$$", ":::"].contains(&close.trim()) && new.ends_with(close) {
        let at = new.len() - close.len();
        if at > byte_idx(old, s0)
            && !new[..at].ends_with('\n')
            && new.starts_with(&old[..byte_idx(old, s0)])
        {
            return Some((format!("{}\n{}", &new[..at], close), char_len(&new[..at])));
        }
    }
    let (p, s) = (byte_idx(old, s0), old.len() - byte_idx(old, e0));
    if new.len() >= p + s && new.starts_with(&old[..p]) && new.ends_with(&old[old.len() - s..]) {
        return None;
    }
    let mut cp = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(cp) || !new.is_char_boundary(cp) {
        cp -= 1;
    }
    let max_cs = (old.len() - cp).min(new.len() - cp);
    let mut cs = old
        .bytes()
        .rev()
        .zip(new.bytes().rev())
        .take(max_cs)
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(old.len() - cs) || !new.is_char_boundary(new.len() - cs) {
        cs -= 1;
    }
    let ins = &new[cp..new.len() - cs];
    let a = cp.clamp(p, old.len() - s);
    let b = (old.len() - cs).clamp(a, old.len() - s);
    let out = format!("{}{ins}{}", &old[..a], &old[b..]);
    let caret = char_len(&out[..a + ins.len()]);
    Some((out, caret))
}

// 번호 목록을 계층별로 1, 2, 3… 다시 매긴다 (맨 위 단계의 첫 번호는 사용자가 쓴 값 유지)
pub(crate) fn renumber_lists(lines: &mut [String]) {
    let mut stack: Vec<(usize, bool, u64)> = Vec::new(); // (들여쓰기, 번호 목록?, 다음 번호)
    for line in lines.iter_mut() {
        let Some((ind, col)) = list_cols(line) else {
            continue; // 이어지는 문단 줄
        };
        let digits = &line[ind..col - 2];
        let ordered = !digits.is_empty();
        while stack.last().is_some_and(|t| t.0 > ind) {
            stack.pop();
        }
        if stack.last().is_some_and(|t| t.0 == ind && t.1 != ordered) {
            stack.pop(); // 같은 단계에서 기호 종류가 바뀌면 새 목록
        }
        if !stack.last().is_some_and(|t| t.0 == ind) {
            let start = if stack.is_empty() {
                digits.parse().unwrap_or(1)
            } else {
                1
            };
            stack.push((ind, ordered, start));
        }
        let top = stack.last_mut().unwrap();
        if ordered {
            let n = top.2.to_string();
            top.2 += 1;
            if n != digits {
                line.replace_range(ind..col - 2, &n);
            }
        }
    }
}

// Tab / Shift+Tab (목록 줄): 계층 구조대로 들여쓰기·내어쓰기.
// Tab → 바로 위 형제 항목의 내용 열로 (형제가 없으면 첫 항목이라 들여쓸 수 없음),
// Shift+Tab → 부모 항목의 들여쓰기로. 하위 항목도 함께 움직이고 번호는 다시 매긴다.
// 첫 줄이 목록이 아니면 None.
// ponytail: 문서 전체를 줄로 나눈다(O(n)). Tab은 드물어서 충분하다.
pub(crate) fn indent_list(s: &str, (a, b): Sel, outdent: bool) -> Option<(String, Sel)> {
    let orig: Vec<&str> = s.split('\n').collect();
    let pos = |c: usize| {
        let mut acc = 0;
        for (i, l) in orig.iter().enumerate() {
            let n = char_len(l);
            if c <= acc + n {
                return (i, c - acc);
            }
            acc += n + 1;
        }
        (orig.len() - 1, 0)
    };
    let ((la, ca), (lb, cb)) = (pos(a), pos(b));
    let (ind, _) = list_cols(orig[la])?;
    let above = || {
        orig[..la]
            .iter()
            .rev()
            .take_while(|l| !l.trim().is_empty())
            .filter_map(|l| list_cols(l))
    };
    let target = if outdent {
        above().find(|&(i, _)| i < ind).map_or(0, |(i, _)| i)
    } else {
        match above().find(|&(i, _)| i <= ind) {
            Some((i, col)) if i == ind => col,
            _ => ind, // 형제 없음: 그대로
        }
    };
    let delta = target as isize - ind as isize;
    if delta == 0 {
        return Some((s.to_string(), (a, b)));
    }
    let lead = |l: &str| l.len() - l.trim_start_matches(' ').len();
    // 선택 줄 + 그 아래의 하위 항목·이어지는 줄까지 함께 이동
    let mut end = lb;
    while end + 1 < orig.len() && !orig[end + 1].trim().is_empty() && lead(orig[end + 1]) > ind {
        end += 1;
    }
    let mut lines: Vec<String> = orig.iter().map(|l| l.to_string()).collect();
    for l in &mut lines[la..=end] {
        if delta > 0 {
            l.insert_str(0, &" ".repeat(delta as usize));
        } else {
            let n = lead(l).min((-delta) as usize);
            l.drain(..n);
        }
    }
    let run_start = (0..=la)
        .rev()
        .find(|&i| i == 0 || lines[i - 1].trim().is_empty())
        .unwrap_or(0);
    let run_end = (end..lines.len())
        .find(|&i| i + 1 == lines.len() || lines[i + 1].trim().is_empty())
        .unwrap_or(end);
    renumber_lists(&mut lines[run_start..=run_end]);
    // 커서: 같은 줄에서 줄 길이 변화만큼 이동 (변화는 모두 내용 앞에서 일어난다)
    let at = |line: usize, col: usize| {
        let diff = char_len(&lines[line]) as isize - char_len(orig[line]) as isize;
        let col = (col as isize + diff).clamp(0, char_len(&lines[line]) as isize) as usize;
        lines[..line].iter().map(|l| char_len(l) + 1).sum::<usize>() + col
    };
    let sel = (at(la, ca), at(lb, cb));
    Some((lines.join("\n"), sel))
}

// 목록 줄 분석: (들여쓰기, 다음 줄에 쓸 기호, 현재 기호 길이(문자), 내용이 비었는지)
pub(crate) fn list_marker(line: &str) -> Option<(String, String, usize, bool)> {
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

// 힙 할당 없이 줄 앞 목록 기호의 바이트 길이만 계산한다 (md_job 레이아웃 전용)
pub(crate) fn list_marker_bytes(line: &str) -> Option<usize> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    let rest = &line[indent..];
    for task in ["- [ ] ", "- [x] ", "- [X] ", "* [ ] ", "* [x] "] {
        if rest.starts_with(task) {
            return Some(indent + task.len());
        }
    }
    for bullet in ["- ", "* ", "+ "] {
        if rest.starts_with(bullet) {
            return Some(indent + bullet.len());
        }
    }
    let digits = rest.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits > 0 && digits < 10 {
        let after = &rest[digits..];
        if after.starts_with(". ") || after.starts_with(") ") {
            return Some(indent + digits + 2);
        }
    }
    None
}

// Enter: 목록이면 다음 기호를 이어 쓰고, 빈 항목이면 목록을 끝낸다. 목록이 아니면 None.
pub(crate) fn smart_enter(s: &str, cursor: usize) -> Option<(String, usize)> {
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
    Some(renumber_block_at(&out, cursor + char_len(&ins)))
}

// 커서(문자 위치)가 있는 블록의 번호 목록을 다시 매긴다. 커서는 같은 줄·같은 글자 뒤에 남는다.
pub(crate) fn renumber_block_at(s: &str, c: usize) -> (String, usize) {
    let b = byte_idx(s, c);
    let bs = s[..b].rfind("\n\n").map_or(0, |i| i + 2);
    let be = s[b..].find("\n\n").map_or(s.len(), |i| b + i);
    let mut lines: Vec<String> = s[bs..be].split('\n').map(str::to_string).collect();
    let li = s[bs..b].matches('\n').count();
    let line_start = s[..b].rfind('\n').map_or(0, |i| i + 1).max(bs);
    let from_end = char_len(&lines[li]) - char_len(&s[line_start..b]);
    renumber_lists(&mut lines);
    let caret = char_len(&s[..bs])
        + lines[..li].iter().map(|l| char_len(l) + 1).sum::<usize>()
        + char_len(&lines[li])
        - from_end;
    (
        format!("{}{}{}", &s[..bs], lines.join("\n"), &s[be..]),
        caret,
    )
}

// 구분선·이미지: 글자를 넣을 수 없는 한 덩어리 블록
pub(crate) fn is_atomic_block(b: &str) -> bool {
    !b.contains('\n') && (is_hr_line(b) || image_line(b).is_some())
}

// 앞뒤 블록과 글자를 합치면 구조가 깨지는 블록 (코드·수식·접기·표·구분선·이미지)
pub(crate) fn is_unmergeable_block(b: &str) -> bool {
    let t = b.trim_start();
    is_atomic_block(b)
        || ["```", "~~~", "$$", ":::toggle", "|"]
            .iter()
            .any(|p| t.starts_with(p))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum BlockKind {
    Paragraph,
    Heading(usize),
    Todo,
    Bullet,
    Numbered,
    Quote,
    Code,
    Callout,
    Toggle,
}

// 줄 앞의 마크다운 블록 기호 분석: (들여쓰기 바이트 길이, 현재 종류, 기호 바이트 길이)
pub(crate) fn detect_line_kind(line: &str) -> (usize, Option<BlockKind>, usize) {
    let indent = line.len() - line.trim_start_matches(' ').len();
    let rest = &line[indent..];
    if let Some(lv) = heading_level(rest) {
        let m = (lv + 1).min(rest.len());
        return (indent, Some(BlockKind::Heading(lv)), m);
    }
    for task in ["- [ ] ", "- [x] ", "- [X] ", "* [ ] ", "* [x] ", "* [X] "] {
        if rest.starts_with(task) {
            return (indent, Some(BlockKind::Todo), task.len());
        }
    }
    for bullet in ["- ", "* ", "+ "] {
        if rest.starts_with(bullet) {
            return (indent, Some(BlockKind::Bullet), bullet.len());
        }
    }
    let digits = rest.bytes().take_while(|b| b.is_ascii_digit()).count();
    if (1..10).contains(&digits) {
        for sep in [". ", ") "] {
            if rest[digits..].starts_with(sep) {
                return (indent, Some(BlockKind::Numbered), digits + sep.len());
            }
        }
    }
    if let Some(after) = rest.strip_prefix('>') {
        let m = 1 + usize::from(after.starts_with(' '));
        return (indent, Some(BlockKind::Quote), m);
    }
    (indent, None, 0)
}

// 선택된 줄들의 블록 유형(제목·목록·할일·인용·일반 문단)을 전환한다 (이미 같은 유형이면 일반 문단으로 토글)
pub(crate) fn convert_lines_kind(s: &str, (a, b): Sel, kind: BlockKind) -> (String, Sel) {
    let (ls, le) = (line_start(s, a), line_end(s, b));
    let block = &s[byte_idx(s, ls)..byte_idx(s, le)];
    let lines_vec: Vec<&str> = block.split('\n').collect();
    let all_same = kind != BlockKind::Paragraph
        && lines_vec
            .iter()
            .filter(|l| !l.trim().is_empty() || lines_vec.len() == 1)
            .all(|l| detect_line_kind(l).1 == Some(kind))
        && lines_vec
            .iter()
            .any(|l| !l.trim().is_empty() || lines_vec.len() == 1);
    let target = if all_same { BlockKind::Paragraph } else { kind };
    let mut first_delta: isize = 0;
    let out_lines: Vec<String> = lines_vec
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let (indent, _, marker) = detect_line_kind(l);
            let body = &l[indent + marker..];
            let keep_indent = if matches!(target, BlockKind::Heading(_)) {
                ""
            } else {
                &l[..indent]
            };
            let prefix = match target {
                BlockKind::Paragraph => String::new(),
                BlockKind::Heading(n) => format!("{} ", "#".repeat(n.clamp(1, 6))),
                BlockKind::Todo => "- [ ] ".to_string(),
                BlockKind::Bullet => "- ".to_string(),
                BlockKind::Numbered => format!("{}. ", i + 1),
                BlockKind::Quote => "> ".to_string(),
                _ => String::new(),
            };
            let new_line = format!("{keep_indent}{prefix}{body}");
            if i == 0 {
                first_delta = char_len(&new_line) as isize - char_len(l) as isize;
            }
            new_line
        })
        .collect();
    let new_block = out_lines.join("\n");
    let out = replace_range(s, (ls, le), &new_block);
    let new_le = ls + char_len(&new_block);
    let na = (a as isize + first_delta).max(ls as isize) as usize;
    let nb = if a == b {
        na
    } else {
        (b as isize + (new_le as isize - le as isize)).max(na as isize) as usize
    };
    (out, (na.min(new_le), nb.min(new_le)))
}

// Ctrl+Enter: 할 일 체크박스 완료/미완료 토글 (일반 줄이면 할 일 항목으로 전환)
pub(crate) fn toggle_todo_lines(s: &str, (a, b): Sel) -> (String, Sel) {
    let (ls, le) = (line_start(s, a), line_end(s, b));
    let block = &s[byte_idx(s, ls)..byte_idx(s, le)];
    let mut first_delta: isize = 0;
    let out_lines: Vec<String> = block
        .split('\n')
        .enumerate()
        .map(|(i, l)| {
            let indent = l.len() - l.trim_start_matches(' ').len();
            let (ind, rest) = l.split_at(indent);
            let new_line = if let Some(body) = rest
                .strip_prefix("- [ ] ")
                .or_else(|| rest.strip_prefix("* [ ] "))
            {
                format!("{ind}- [x] {body}")
            } else if let Some(body) = rest
                .strip_prefix("- [x] ")
                .or_else(|| rest.strip_prefix("- [X] "))
                .or_else(|| rest.strip_prefix("* [x] "))
                .or_else(|| rest.strip_prefix("* [X] "))
            {
                format!("{ind}- [ ] {body}")
            } else {
                let (_, _, marker) = detect_line_kind(l);
                format!("{ind}- [ ] {}", &l[indent + marker..])
            };
            if i == 0 {
                first_delta = char_len(&new_line) as isize - char_len(l) as isize;
            }
            new_line
        })
        .collect();
    let new_block = out_lines.join("\n");
    let out = replace_range(s, (ls, le), &new_block);
    let new_le = ls + char_len(&new_block);
    let na = (a as isize + first_delta).max(ls as isize) as usize;
    let nb = if a == b {
        na
    } else {
        (b as isize + (new_le as isize - le as isize)).max(na as isize) as usize
    };
    (out, (na.min(new_le), nb.min(new_le)))
}

// 찾기: 모든 일치 구간(문자 인덱스). 대소문자 구분·ASCII 검색은 힙 할당 없이 바이트 단위로 고속 스캔한다.
pub(crate) fn find_all(s: &str, query: &str, case: bool) -> Vec<Sel> {
    if query.is_empty() || query.len() > s.len() {
        return Vec::new();
    }
    if case {
        let q_chars = char_len(query);
        let mut out = Vec::new();
        let (mut last_b, mut ci) = (0, 0);
        for (b, _) in s.match_indices(query) {
            ci += char_len(&s[last_b..b]);
            out.push((ci, ci + q_chars));
            last_b = b + query.len();
            ci += q_chars;
        }
        return out;
    }
    if query.is_ascii() {
        let qb = query.as_bytes();
        let sb = s.as_bytes();
        let q_len = qb.len();
        let mut out = Vec::new();
        let (mut i, mut last_b, mut ci) = (0, 0, 0);
        while i + q_len <= sb.len() {
            if sb[i..i + q_len].eq_ignore_ascii_case(qb)
                && s.is_char_boundary(i)
                && s.is_char_boundary(i + q_len)
            {
                ci += char_len(&s[last_b..i]);
                out.push((ci, ci + q_len));
                i += q_len;
                last_b = i;
                ci += q_len;
            } else {
                i += 1;
            }
        }
        return out;
    }
    let norm = |c: char| c.to_lowercase().next().unwrap_or(c);
    let needle: Vec<char> = query.chars().map(norm).collect();
    let hay: Vec<char> = s.chars().map(norm).collect();
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

pub(crate) fn replace_all(s: &str, matches: &[Sel], with: &str) -> String {
    if matches.is_empty() {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut prev_b = 0usize;
    let mut prev_c = 0usize;
    for &(a, b) in matches {
        let ba = prev_b + byte_idx(&s[prev_b..], a.saturating_sub(prev_c));
        let bb = ba + byte_idx(&s[ba..], b.saturating_sub(a));
        out.push_str(&s[prev_b..ba]);
        out.push_str(with);
        prev_b = bb;
        prev_c = b;
    }
    out.push_str(&s[prev_b..]);
    out
}

// 목차: (레벨, 제목, 줄 번호, 줄 시작 문자 인덱스, 줄 시작 바이트 인덱스). 코드 펜스 안은 제외.
// 일반 줄에서는 UTF-8 문자 수(char_len)를 세지 않고 제목 줄을 만났을 때만 증분 계산한다.
pub(crate) fn outline_with_bytes_into(
    s: &str,
    out: &mut Vec<(usize, String, usize, usize, usize)>,
) {
    out.clear();
    let (mut in_code, mut ci, mut bi, mut last_bi) = (false, 0, 0, 0);
    for (n, line) in s.split('\n').enumerate() {
        let t = line.trim_start();
        match t.as_bytes().first() {
            Some(b'`' | b'~') if t.starts_with("```") || t.starts_with("~~~") => {
                in_code = !in_code;
            }
            Some(b'#') if !in_code => {
                let level = t.bytes().take_while(|&c| c == b'#').count();
                if (1..=6).contains(&level) && t.as_bytes().get(level) == Some(&b' ') {
                    ci += char_len(&s[last_bi..bi]);
                    last_bi = bi;
                    out.push((
                        level,
                        t[level..].trim().trim_end_matches('#').trim().into(),
                        n,
                        ci,
                        bi,
                    ));
                }
            }
            _ => {}
        }
        bi += line.len() + 1;
    }
}

#[cfg(test)]
pub(crate) fn outline(s: &str) -> Vec<(usize, String, usize, usize)> {
    let mut out = Vec::new();
    outline_with_bytes_into(s, &mut out);
    out.into_iter()
        .map(|(l, t, n, ci, _)| (l, t, n, ci))
        .collect()
}

// 미리보기 전용 변환. 바이트 길이를 보존해서 체크박스 위치가 원문과 1:1로 대응한다.
//   :::toggle 제목 → ##### ▸ 제목,  ::: → 공백,  ==형광펜== → **형광펜**,  [[Ctrl]] → ``Ctrl``
// 확장 문법이 없는 일반 블록은 힙 할당 없이 원본 슬라이스(Cow::Borrowed)를 그대로 돌려준다.
pub(crate) fn preprocess(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.contains(":::") && !s.contains("==") && !s.contains("[[") {
        return std::borrow::Cow::Borrowed(s);
    }
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
            for _ in 0..line.len() {
                out.push(' ');
            }
        } else if !line.contains("==") && !line.contains("[[") {
            out.push_str(line);
        } else {
            out.push_str(&preprocess_inline(line));
        }
    }
    debug_assert_eq!(out.len(), s.len());
    std::borrow::Cow::Owned(out)
}

pub(crate) fn preprocess_inline(line: &str) -> String {
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
pub(crate) fn apply_preview_change(text: &mut String, before: &str, after: &str) -> bool {
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

// 미리보기와 같은 파서(pulldown-cmark)·같은 전처리를 쓴다. 정적 HTML이라 Mermaid·수식은 원문 그대로 나온다.
pub(crate) fn md_to_html(md: &str) -> String {
    use pulldown_cmark::{html, Event, Options, Parser, Tag};
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_DEFINITION_LIST
        | Options::ENABLE_MATH
        | Options::ENABLE_GFM;
    let src = preprocess(md);
    // 문서 속 원시 HTML(<script>, onerror= 등)은 실행되지 않게 글자 그대로 보여 주고, javascript: 링크는 막는다
    let events = Parser::new_ext(&src, opts).map(|ev| match ev {
        Event::Html(t) | Event::InlineHtml(t) => Event::Text(t),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) if dest_url
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("javascript:") =>
        {
            Event::Start(Tag::Link {
                link_type,
                dest_url: "#".into(),
                title,
                id,
            })
        }
        ev => ev,
    });
    let mut body = String::new();
    html::push_html(&mut body, events);
    body
}

pub(crate) fn export_html(title: &str, md: &str) -> String {
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

pub(crate) fn heading_level(t: &str) -> Option<usize> {
    let n = t.chars().take_while(|&c| c == '#').count();
    ((1..=6).contains(&n) && t[n..].chars().next().is_none_or(|c| c == ' ')).then_some(n)
}

pub(crate) fn fence_opener_line(line: &str) -> Option<(&str, &str, &str)> {
    let body = line.trim_end_matches(['\n', '\r']);
    let indent_len = body.len() - body.trim_start().len();
    let indent = &body[..indent_len];
    let t = &body[indent_len..];
    let fence = if t.starts_with("```") {
        "```"
    } else if t.starts_with("~~~") {
        "~~~"
    } else {
        return None;
    };
    let rest = &t[3..];
    if rest.contains('`') || rest.contains('~') {
        return None;
    }
    let lang = rest.trim();
    (!lang.contains(char::is_whitespace)).then_some((indent, fence, lang))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TableAlign {
    None,
    Left,
    Center,
    Right,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct MdTable {
    pub(crate) headers: Vec<String>,
    pub(crate) aligns: Vec<TableAlign>,
    pub(crate) rows: Vec<Vec<String>>,
}

pub(crate) fn split_table_row(line: &str) -> Option<Vec<String>> {
    let t = line.trim();
    if !t.contains('|') {
        return None;
    }
    let inner = t.strip_prefix('|').unwrap_or(t);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                if next == '|' || next == '\\' {
                    cur.push(next);
                    chars.next();
                    continue;
                }
            }
        }
        if c == '|' {
            cells.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    cells.push(cur.trim().to_string());
    (!cells.is_empty()).then_some(cells)
}

pub(crate) fn parse_table_sep(cell: &str) -> Option<TableAlign> {
    let t = cell.trim();
    let left = t.starts_with(':');
    let right = t.ends_with(':');
    let mid = &t[usize::from(left)..t.len().saturating_sub(usize::from(right))];
    if mid.is_empty() || !mid.bytes().all(|b| b == b'-') {
        return None;
    }
    Some(match (left, right) {
        (true, true) => TableAlign::Center,
        (true, false) => TableAlign::Left,
        (false, true) => TableAlign::Right,
        (false, false) => TableAlign::None,
    })
}

pub(crate) fn parse_md_table(src: &str) -> Option<MdTable> {
    let mut lines = src.trim_end_matches(['\n', '\r']).lines();
    let h_line = lines.next()?;
    let s_line = lines.next()?;
    let mut headers = split_table_row(h_line)?;
    let seps_raw = split_table_row(s_line)?;
    let mut aligns = Vec::with_capacity(seps_raw.len());
    for s in seps_raw {
        aligns.push(parse_table_sep(&s)?);
    }
    let cols = headers.len().max(aligns.len());
    if cols == 0 {
        return None;
    }
    headers.resize(cols, String::new());
    aligns.resize(cols, TableAlign::None);
    let mut rows = Vec::new();
    for line in lines {
        let mut row = split_table_row(line)?;
        row.resize(cols, String::new());
        rows.push(row);
    }
    Some(MdTable {
        headers,
        aligns,
        rows,
    })
}

pub(crate) fn serialize_md_table(tbl: &MdTable) -> String {
    let cols = tbl.headers.len().max(1);
    let escape_cell = |s: &str| s.replace('\r', "").replace('\n', " ").replace('|', "\\|");
    let mut out = String::new();
    let push_row = |out: &mut String, cells: &[String]| {
        out.push('|');
        for i in 0..cols {
            out.push(' ');
            if let Some(c) = cells.get(i) {
                out.push_str(&escape_cell(c));
            }
            out.push_str(" |");
        }
    };
    push_row(&mut out, &tbl.headers);
    out.push('\n');
    out.push('|');
    for i in 0..cols {
        let sep = match tbl.aligns.get(i).copied().unwrap_or(TableAlign::None) {
            TableAlign::None => "---",
            TableAlign::Left => ":---",
            TableAlign::Center => ":---:",
            TableAlign::Right => "---:",
        };
        out.push(' ');
        out.push_str(sep);
        out.push_str(" |");
    }
    for row in &tbl.rows {
        out.push('\n');
        push_row(&mut out, row);
    }
    out
}

// 노션식 캔버스의 블록: 빈 줄로 나뉜 바이트 구간.
// 코드 펜스·$$·:::toggle 안의 빈 줄로는 나누지 않고, 제목 줄은 항상 혼자 한 블록이다.
pub(crate) fn split_blocks_into(s: &str, out: &mut Vec<Range<usize>>) {
    out.clear();
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
        } else if body.is_empty() {
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
}

pub(crate) fn split_blocks(s: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    split_blocks_into(s, &mut out);
    out
}

// 캔버스/커서 이동용 슬롯 목록: 비어 있지 않은 블록 + 빈 줄 블록(start..start)
pub(crate) fn collect_doc_slots_into(
    text: &str,
    blocks: &[Range<usize>],
    active_empty: Option<usize>,
    out: &mut Vec<Range<usize>>,
) {
    fn push_gap(
        out: &mut Vec<Range<usize>>,
        active_empty: Option<usize>,
        start: usize,
        end: usize,
        step_from: usize,
        skip_end: bool,
    ) {
        let mut p = step_from;
        while p < end || (!skip_end && p == end) {
            out.push(p..p);
            p += 2;
        }
        if let Some(a) = active_empty {
            if a >= start && a <= end && (!skip_end || a < end) && !out.contains(&(a..a)) {
                let at = out.partition_point(|r| r.start < a);
                out.insert(at, a..a);
            }
        }
    }
    out.clear();
    if blocks.is_empty() {
        if text.is_empty() {
            if active_empty == Some(0) {
                out.push(0..0);
            }
        } else {
            push_gap(out, active_empty, 0, text.len(), 0, false);
        }
        return;
    }
    push_gap(out, active_empty, 0, blocks[0].start, 0, true);
    for (i, b) in blocks.iter().enumerate() {
        out.push(b.clone());
        let next_start = blocks.get(i + 1).map_or(text.len(), |n| n.start);
        let step_from = (b.end + 2).min(next_start + 1);
        push_gap(
            out,
            active_empty,
            b.end,
            next_start,
            step_from,
            i + 1 < blocks.len(),
        );
    }
}

#[cfg(test)]
pub(crate) fn collect_doc_slots(
    text: &str,
    blocks: &[Range<usize>],
    active_empty: Option<usize>,
) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    collect_doc_slots_into(text, blocks, active_empty, &mut out);
    out
}

pub(crate) fn slot_index(slots: &[Range<usize>], target: &Range<usize>) -> Option<usize> {
    slots
        .binary_search_by(|s| s.start.cmp(&target.start).then(s.end.cmp(&target.end)))
        .ok()
}
