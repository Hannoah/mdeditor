// 순수 텍스트 연산: 문자 인덱스 기준 편집, 인라인/블록 파서, 표, 목록, HTML 내보내기.
// egui에 의존하지 않아 단위 테스트와 퍼즈 테스트가 쉽다.
use std::ops::Range;

// ---------- 편집 연산 (문자 인덱스 기준, 순수 함수) ----------

pub(crate) type Sel = (usize, usize); // (시작, 끝) 문자 인덱스, 시작 <= 끝

pub(crate) fn byte_idx(s: &str, ci: usize) -> usize {
    s.char_indices().nth(ci).map_or(s.len(), |(b, _)| b)
}

pub(crate) fn clamp_char_range(s: &str, r: &Range<usize>) -> Range<usize> {
    let len = s.len();
    let mut start = r.start.min(len);
    while start > 0 && !s.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = r.end.clamp(start, len);
    while end > start && !s.is_char_boundary(end) {
        end -= 1;
    }
    start..end
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
    (mut a, mut b): Sel,
    pre: &str,
    suf: &str,
    placeholder: &str,
) -> (String, Sel) {
    let is_link = suf.starts_with("](");
    if a < b && !is_link && s[byte_idx(s, a)..byte_idx(s, b)].contains('\n') {
        return wrap_multiline_selection(s, (a, b), pre, suf, placeholder);
    }
    if a < b {
        let ls = line_start(s, a);
        let le = line_end(s, a);
        let line = &s[byte_idx(s, ls)..byte_idx(s, le)];
        let m = hidden_prefix(line);
        if m > 0 && !is_collapsed_line(line) {
            let content_start = ls + char_len(&line[..m]);
            if a < content_start && b > content_start {
                a = content_start;
            }
        }
        if !is_link {
            let sel_str = &s[byte_idx(s, a)..byte_idx(s, b)];
            let trimmed = sel_str.trim_matches([' ', '\t']);
            if !trimmed.is_empty() {
                let lead_ws = sel_str.len() - sel_str.trim_start_matches([' ', '\t']).len();
                let trail_ws = sel_str.len() - sel_str.trim_end_matches([' ', '\t']).len();
                a += char_len(&sel_str[..lead_ws]);
                b -= char_len(&sel_str[sel_str.len() - trail_ws..]);
            }
        }
    }
    let (pl, sl) = (char_len(pre), char_len(suf));
    let target_kind = (pre == suf)
        .then_some(match pre {
            "**" | "__" => Some(Inline::Bold),
            "==" => Some(Inline::Mark),
            "~~" => Some(Inline::Strike),
            "*" => Some(Inline::Italic),
            "`" => Some(Inline::Code),
            _ => None,
        })
        .flatten();

    if let Some(tk) = target_kind {
        let ls = line_start(s, a);
        let le = line_end(s, b);
        let line = &s[byte_idx(s, ls)..byte_idx(s, le)];
        let m = if is_collapsed_line(line) {
            0
        } else {
            hidden_prefix(line).min(line.len())
        };
        let content_start = ls + char_len(&line[..m]);
        let rest = &line[m..];
        let spans: Vec<(usize, usize, usize, usize)> = inline_spans(rest)
            .into_iter()
            .filter(|sp| sp.4 == tk)
            .map(|(os, bs, be, ce, _)| {
                (
                    content_start + char_len(&rest[..os]),
                    content_start + char_len(&rest[..bs]),
                    content_start + char_len(&rest[..be]),
                    content_start + char_len(&rest[..ce]),
                )
            })
            .collect();

        if a == b {
            if let Some(&(os, bs, be, ce)) =
                spans.iter().find(|&&(os, _, _, ce)| a >= os && a <= ce)
            {
                let inner_str = &s[byte_idx(s, bs)..byte_idx(s, be)];
                let out = replace_range(s, (os, ce), inner_str);
                let na = if a <= bs {
                    os
                } else if a >= be {
                    os + (be - bs)
                } else {
                    a - (bs - os)
                };
                return (out, (na, na));
            }
        } else {
            let overlapping: Vec<(usize, usize, usize, usize)> = spans
                .into_iter()
                .filter(|&(os, bs, be, ce)| (ce > a && os < b) || (bs == a && be == b))
                .collect();
            if overlapping.len() == 1 && a >= overlapping[0].0 && b <= overlapping[0].3 {
                let (os, bs, be, ce) = overlapping[0];
                let ua = a.clamp(bs, be);
                let ub = b.clamp(bs, be);
                let b_bs = byte_idx(s, bs);
                let b_be = byte_idx(s, be);
                let full_inner = &s[b_bs..b_be];
                let rel_ua = byte_idx(s, ua) - b_bs;
                let rel_ub = byte_idx(s, ub) - b_bs;
                let left_trim = full_inner[..rel_ua].trim_end_matches([' ', '\t']);
                let right_trim = full_inner[rel_ub..].trim_start_matches([' ', '\t']);
                let mid_start = left_trim.len();
                let mid_end = full_inner.len() - right_trim.len();
                let mid = &full_inner[mid_start..mid_end];
                let mut rep = String::with_capacity(full_inner.len() + 8);
                let mut sel_offset = os;
                if !left_trim.is_empty() {
                    rep.push_str(pre);
                    rep.push_str(left_trim);
                    rep.push_str(suf);
                    sel_offset += pl + char_len(left_trim) + sl;
                }
                sel_offset += char_len(&full_inner[mid_start..rel_ua]);
                rep.push_str(mid);
                if !right_trim.is_empty() {
                    rep.push_str(pre);
                    rep.push_str(right_trim);
                    rep.push_str(suf);
                }
                let out = replace_range(s, (os, ce), &rep);
                let sel_len = ub - ua;
                return (out, (sel_offset, sel_offset + sel_len));
            } else if !overlapping.is_empty() {
                let union_a = overlapping.first().map_or(a, |sp| a.min(sp.0));
                let union_b = overlapping.last().map_or(b, |sp| b.max(sp.3));
                let mut clean = String::new();
                let mut cur = union_a;
                for (os, bs, be, ce) in overlapping {
                    if cur < os {
                        clean.push_str(&s[byte_idx(s, cur)..byte_idx(s, os)]);
                    }
                    clean.push_str(&s[byte_idx(s, bs)..byte_idx(s, be)]);
                    cur = ce;
                }
                if cur < union_b {
                    clean.push_str(&s[byte_idx(s, cur)..byte_idx(s, union_b)]);
                }
                let clean_trim = clean.trim_matches([' ', '\t']);
                let rep = format!("{pre}{clean_trim}{suf}");
                let out = replace_range(s, (union_a, union_b), &rep);
                let start = union_a + pl;
                let end = start + char_len(clean_trim);
                return (out, (start, end));
            }
        }
    }

    let (ba, bb) = (byte_idx(s, a), byte_idx(s, b));
    let (before, sel, after) = (&s[..ba], &s[ba..bb], &s[bb..]);
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

fn wrap_multiline_selection(
    s: &str,
    (a, b): Sel,
    pre: &str,
    suf: &str,
    placeholder: &str,
) -> (String, Sel) {
    let mut segs: Vec<(usize, usize)> = Vec::new();
    let mut cur_c = line_start(s, a);
    let end_le = line_end(s, b);
    while cur_c <= end_le {
        let le = line_end(s, cur_c);
        let line = &s[byte_idx(s, cur_c)..byte_idx(s, le)];
        let m = if is_collapsed_line(line) {
            0
        } else {
            hidden_prefix(line).min(line.len())
        };
        let content_c = cur_c + char_len(&line[..m]);
        let mut sa = a.max(cur_c);
        let mut sb = b.min(le);
        if sa < content_c && sb > content_c {
            sa = content_c;
        }
        if sa < sb {
            let slice = &s[byte_idx(s, sa)..byte_idx(s, sb)];
            let trimmed = slice.trim_matches([' ', '\t']);
            if !trimmed.is_empty() {
                let lead = slice.len() - slice.trim_start_matches([' ', '\t']).len();
                let trail = slice.len() - slice.trim_end_matches([' ', '\t']).len();
                sa += char_len(&slice[..lead]);
                sb -= char_len(&slice[slice.len() - trail..]);
                segs.push((sa, sb));
            }
        }
        if le >= char_len(s) {
            break;
        }
        cur_c = le + 1;
    }
    if segs.is_empty() {
        return (s.to_string(), (a, b));
    }
    let all_wrapped = segs.iter().all(|&(sa, sb)| {
        let (_, new_sel) = wrap_selection(s, (sa, sb), pre, suf, placeholder);
        (new_sel.1 - new_sel.0) <= (sb - sa) && {
            let (out, _) = wrap_selection(s, (sa, sb), pre, suf, placeholder);
            char_len(&out) < char_len(s)
        }
    });
    let mut cur_s = s.to_string();
    let mut delta: isize = 0;
    let mut first_a = a;
    let mut last_b = b;
    for (idx, (sa, sb)) in segs.into_iter().enumerate() {
        let adj_a = (sa as isize + delta) as usize;
        let adj_b = (sb as isize + delta) as usize;
        let old_len = char_len(&cur_s);
        let (next_s, (na, nb)) = wrap_selection(&cur_s, (adj_a, adj_b), pre, suf, placeholder);
        let new_len = char_len(&next_s);
        let step_diff = new_len as isize - old_len as isize;
        // 일부 줄만 이미 감싸져 있을 때 전체를 감싸는 동작이면 벗겨진 줄을 다시 감싼다
        if !all_wrapped && step_diff < 0 {
            let (rewrapped, (ra, rb)) = wrap_selection(&next_s, (na, nb), pre, suf, placeholder);
            let rw_diff = char_len(&rewrapped) as isize - old_len as isize;
            cur_s = rewrapped;
            delta += rw_diff;
            if idx == 0 {
                first_a = ra.saturating_sub(char_len(pre));
            }
            last_b = rb + char_len(suf);
        } else {
            cur_s = next_s;
            delta += step_diff;
            if idx == 0 {
                first_a = if step_diff > 0 {
                    na.saturating_sub(char_len(pre))
                } else {
                    na
                };
            }
            last_b = if step_diff > 0 {
                nb + char_len(suf)
            } else {
                nb
            };
        }
    }
    (cur_s, (first_a, last_b.max(first_a)))
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

// ---------- 서식 복사 (Format Painter) ----------

// 복사해 둔 서식: 글자 서식(굵게·기울임·취소선·강조·코드)과 줄 유형(제목·목록·인용 등)
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct FormatSample {
    pub(crate) inline: Vec<Inline>,
    pub(crate) block: BlockKind,
}

const FORMAT_MARKS: [(Inline, &str, &str); 5] = [
    (Inline::Bold, "**", "굵게"),
    (Inline::Italic, "*", "기울임"),
    (Inline::Strike, "~~", "취소선"),
    (Inline::Mark, "==", "강조"),
    (Inline::Code, "`", "코드"),
];

fn format_mark_pair(kind: Inline) -> Option<(&'static str, &'static str)> {
    FORMAT_MARKS
        .iter()
        .find(|(k, _, _)| *k == kind)
        .map(|(_, pre, _)| (*pre, *pre))
}

// 선택(또는 커서) 위치의 글자 서식 종류. 선택 양끝 공백은 무시하고, 선택 전체를 감싸는 서식만 센다.
fn selection_inline_kinds(s: &str, (mut a, mut b): Sel) -> Vec<Inline> {
    if a < b {
        let st = &s[byte_idx(s, a)..byte_idx(s, b)];
        let lead = char_len(&st[..st.len() - st.trim_start_matches([' ', '\t']).len()]);
        let trail = char_len(&st[st.trim_end_matches([' ', '\t']).len()..]);
        if lead + trail < b - a {
            a += lead;
            b -= trail;
        }
    }
    let ls = line_start(s, a);
    let le = line_end(s, a);
    let line = &s[byte_idx(s, ls)..byte_idx(s, le)];
    let m = if is_collapsed_line(line) {
        0
    } else {
        hidden_prefix(line).min(line.len())
    };
    let rest = &line[m..];
    let base = ls + char_len(&line[..m]);
    let mut out = Vec::new();
    for (os, _, _, ce, kind) in inline_spans(rest) {
        if format_mark_pair(kind).is_none() {
            continue;
        }
        let (os, ce) = (base + char_len(&rest[..os]), base + char_len(&rest[..ce]));
        if os <= a && b <= ce && !out.contains(&kind) {
            out.push(kind);
        }
    }
    out
}

fn line_block_kind(s: &str, ci: usize) -> BlockKind {
    let ls = line_start(s, ci);
    let le = line_end(s, ci);
    detect_line_kind(&s[byte_idx(s, ls)..byte_idx(s, le)])
        .1
        .unwrap_or(BlockKind::Paragraph)
}

// 선택(또는 커서) 위치의 서식을 견본으로 만든다.
pub(crate) fn sample_format(s: &str, sel: Sel) -> FormatSample {
    FormatSample {
        inline: selection_inline_kinds(s, sel),
        block: line_block_kind(s, sel.0),
    }
}

// 견본 서식을 선택 영역에 입힌다. 줄 유형은 선택한 줄에, 글자 서식은 선택한 글자에 적용한다
// (글자를 고르지 않고 커서만 있으면 줄 유형만 바꾼다). 바뀐 게 없으면 None.
pub(crate) fn apply_format(s: &str, sel: Sel, sample: &FormatSample) -> Option<(String, Sel)> {
    let mut text = s.to_string();
    let mut sel = sel;
    if line_block_kind(&text, sel.0) != sample.block {
        let (out, ns) = convert_lines_kind(&text, sel, sample.block);
        text = out;
        sel = ns;
        if sample.block == BlockKind::Numbered && sel.0 == sel.1 {
            let (renum, c) = renumber_block_at(&text, sel.0);
            text = renum;
            sel = (c, c);
        }
    }
    if sel.0 < sel.1 {
        let picked = &text[byte_idx(&text, sel.0)..byte_idx(&text, sel.1)];
        if picked.contains('\n') {
            // 여러 줄: 기존 글자 서식을 모두 벗기고 견본 서식을 줄마다 입힌다
            let clean = clear_inline_formatting(picked);
            let new_len = char_len(&clean);
            text = replace_range(&text, sel, &clean);
            sel = (sel.0, sel.0 + new_len);
            for kind in &sample.inline {
                if let Some((pre, suf)) = format_mark_pair(*kind) {
                    let ph = FORMAT_MARKS
                        .iter()
                        .find(|m| m.0 == *kind)
                        .map_or("", |m| m.2);
                    let (t, ns) = wrap_selection(&text, sel, pre, suf, ph);
                    text = t;
                    sel = ns;
                }
            }
        } else {
            // 한 줄: 지금 서식과 견본이 다른 종류만 토글(켜기/끄기)한다
            let cur = selection_inline_kinds(&text, sel);
            for (kind, pre, ph) in FORMAT_MARKS {
                if cur.contains(&kind) != sample.inline.contains(&kind) {
                    let (t, ns) = wrap_selection(&text, sel, pre, pre, ph);
                    text = t;
                    sel = ns;
                }
            }
        }
    }
    (text != s).then_some((text, sel))
}

// 상태 표시용 설명 (예: "굵게 · 기울임 · 제목 2")
pub(crate) fn describe_format(f: &FormatSample) -> String {
    let mut parts: Vec<String> = FORMAT_MARKS
        .iter()
        .filter(|(k, _, _)| f.inline.contains(k))
        .map(|(_, _, name)| (*name).to_string())
        .collect();
    match f.block {
        BlockKind::Heading(n) => parts.push(format!("제목 {n}")),
        BlockKind::Todo => parts.push("할 일".into()),
        BlockKind::Bullet => parts.push("글머리 목록".into()),
        BlockKind::Numbered => parts.push("번호 목록".into()),
        BlockKind::Quote => parts.push("인용구".into()),
        _ => {}
    }
    if parts.is_empty() {
        "서식 없음".into()
    } else {
        parts.join(" · ")
    }
}

// Tab / Shift+Tab: 선택된 줄 전체를 2칸 들여쓰기·내어쓰기 (탭 문자 '\t' 내어쓰기 및 다중 블록 빈 줄 보존 지원)
pub(crate) fn indent_lines(s: &str, (a, b): Sel, outdent: bool) -> (String, Sel) {
    let (ls, le) = (line_start(s, a), line_end(s, b));
    let block = &s[byte_idx(s, ls)..byte_idx(s, le)];
    let raw_lines: Vec<&str> = block.split('\n').collect();
    let multi = raw_lines.len() > 1;
    let mut first_delta: isize = 0;
    let lines: Vec<String> = raw_lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let (new, delta) = if multi && l.is_empty() {
                (String::new(), 0)
            } else if outdent {
                if let Some(rest) = l.strip_prefix('\t') {
                    (rest.to_string(), -1)
                } else {
                    let n = (l.len() - l.trim_start_matches(' ').len()).min(2);
                    (l[n..].to_string(), -(n as isize))
                }
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
        (b as isize + (new_le as isize - le as isize)).max(na as isize) as usize
    };
    (out, (na.min(new_le), nb.min(new_le)))
}

// 목록 줄의 (들여쓰기, 내용 시작 열). CommonMark에서 하위 항목은 부모의 "내용 시작 열"까지
// 들여써야 자식이 된다: "- a" → 2칸, "1. a" → 3칸, "10. a" → 4칸. (할 일 "- [ ] "도 2칸 기준)
pub(crate) fn list_cols(line: &str) -> Option<(usize, usize)> {
    let ind = line.len() - line.trim_start_matches(' ').len();
    let rest = &line[ind..];
    if ["- ", "* "].iter().any(|b| rest.starts_with(b)) {
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
    let mut base_col = stack.last().map_or(0, |t| t.0);
    while base_col + 2 <= ind {
        base_col += 2;
        stack.push((base_col, None));
    }
    let num = (!digits.is_empty()).then(|| match sib {
        Some((_, Some(n))) => n + 1,
        _ => digits.parse().unwrap_or(1),
    });
    let depth = stack.len();
    stack.push((ind + 2, num));
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
            if !prev_alnum {
                if let Some(mut off) = inner.find(mark) {
                    if mark == "**" && inner[off..].starts_with("***") && !inner.starts_with('*') {
                        off += 1;
                    }
                    let body = &inner[..off];
                    let ws_ok = if mark == "__" {
                        !body.starts_with(char::is_whitespace)
                            && !body.ends_with(char::is_whitespace)
                    } else {
                        !body.starts_with(char::is_whitespace)
                            || !body.ends_with(char::is_whitespace)
                    };
                    if !body.trim().is_empty() && !body.contains('\n') && ws_ok {
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
// 구분선·접기 제목/닫는 줄·알림 상자 제목 줄은 인라인 서식 없음(None).
pub(crate) fn inline_start(line: &str) -> Option<usize> {
    let t = line.trim_start();
    if is_hr_line(line) || t.starts_with(":::") || (t.starts_with('>') && callout_kind(t).is_some())
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

// 줄 전체가 접혀 있어 커서가 서면 안 되는 비편집 줄 (알림 상자 제목·구분선·접기 닫힘·이미지)
// 빈 목록("- "), 빈 제목("# "), 빈 인용("> ") 등은 사용자가 커서를 두고 입력·삭제할 수 있어야 하므로 제외한다.
fn is_collapsed_line(line: &str) -> bool {
    let t = line.trim_start();
    (t.starts_with('>') && callout_kind(t).is_some())
        || is_hr_line(line)
        || t.trim_end() == ":::"
        || image_line(line).is_some()
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
                    if is_collapsed_line(line) && pre == line.len() && i + 1 < lines.len() {
                        e += 1; // 접힌 줄만 줄바꿈까지 함께 숨겨 커서가 서지 않게 한다
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

// 클릭·드래그·Home 등으로 놓인 커서가 숨긴 줄머리 기호("- ", "6. ", "# ", "> " 등) 안에 있으면 내용 시작으로 옮긴다
pub(crate) fn snap_out_of_prefix(s: &str, c: usize) -> usize {
    for (a, e, pre) in hidden_runs(s) {
        if pre && c >= a && c < e {
            return e;
        }
    }
    c
}

// 다중 줄 블록 안에서 뒤 공백이 빠진 채 남은 빈 목록/인용 기호 줄("-", "*", "+", ">", "1.", "- [ ]")인지 판별
fn is_bare_empty_marker_line(line: &str) -> bool {
    let t = line.trim();
    if matches!(
        t,
        "-" | "*" | ">" | "- [ ]" | "- [x]" | "- [X]" | "* [ ]" | "* [x]" | "* [X]"
    ) {
        return true;
    }
    let d = t.bytes().take_while(u8::is_ascii_digit).count();
    (1..10).contains(&d) && matches!(&t[d..], "." | ")")
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
// 블록 첫 항목은 기호만 지워 일반 문단으로(다중 줄 블록의 첫 줄이 빈 항목이면 그 빈 줄 자체를 삭제),
// 바로 윗줄이 빈 목록·인용 줄이면 현재 줄 기호는 유지한 채 윗 빈 줄만 지우고, 그 밖의 항목은 윗줄과 합친다.
pub(crate) fn backspace_list_marker(s: &str, c: usize) -> Option<(String, usize)> {
    let ls = line_start(s, c);
    let le = line_end(s, c);
    let lb = byte_idx(s, ls);
    let leb = byte_idx(s, le);
    let line = &s[lb..leb];
    let bstart = s[..lb].rfind("\n\n").map_or(0, |i| i + 2);
    let bend = s[leb..].find("\n\n").map_or(s.len(), |i| leb + i);
    let m = hidden_prefix(line);
    if m == 0 {
        if bstart < bend
            && s[bstart..bend].contains('\n')
            && is_bare_empty_marker_line(line)
            && c == le
        {
            if lb == bstart && leb < bend {
                let out = replace_range(s, (ls, le + 1), "");
                let next_le = line_end(&out, ls);
                let next_line = &out[byte_idx(&out, ls)..byte_idx(&out, next_le)];
                let next_m = hidden_prefix(next_line);
                let caret = ls + char_len(&next_line[..next_m]);
                return Some(renumber_block_at(&out, caret));
            } else if lb > bstart {
                let out = replace_range(s, (ls - 1, le), "");
                return Some(renumber_block_at(&out, ls - 1));
            }
        }
        return None;
    }
    let m_chars = char_len(&line[..m]);
    if c != ls + m_chars {
        return None;
    }
    let t = line.trim_start();
    let is_empty_item = line[m..].trim().is_empty();
    let prev_is_empty_item = lb > bstart && {
        let prev_le = ls - 1;
        let prev_ls = line_start(s, prev_le);
        let prev_line = &s[byte_idx(s, prev_ls)..byte_idx(s, prev_le)];
        let pm = hidden_prefix(prev_line);
        (pm > 0 && !is_collapsed_line(prev_line) && prev_line[pm..].trim().is_empty())
            || is_bare_empty_marker_line(prev_line)
    };
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
        if is_empty_item {
            return Some((replace_range(s, (ls - 1, le), ""), ls - 1));
        }
        if prev_is_empty_item {
            let prev_ls = line_start(s, ls - 1);
            let out = replace_range(s, (prev_ls, ls), "");
            return Some((out, prev_ls + m_chars));
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
    if lb == bstart {
        if is_empty_item && leb < bend {
            // 다중 줄 블록의 첫 줄이 빈 목록 줄("- \n6. ...")이면 빈 첫 줄 전체를 삭제해 '\n\n\n' 고아 빈 줄이 생기지 않게 한다
            let out = replace_range(s, (ls, le + 1), "");
            let next_le = line_end(&out, ls);
            let next_line = &out[byte_idx(&out, ls)..byte_idx(&out, next_le)];
            let next_m = hidden_prefix(next_line);
            let caret = ls + char_len(&next_line[..next_m]);
            return Some(renumber_block_at(&out, caret));
        }
        let out = replace_range(s, (ls, le.min(c)), "");
        return Some(renumber_block_at(&out, ls));
    }
    if is_empty_item {
        let out = replace_range(s, (ls - 1, le), "");
        return Some(renumber_block_at(&out, ls - 1));
    }
    if prev_is_empty_item {
        // 바로 윗줄이 빈 목록/인용 줄("- ")이면 현재 줄의 기호("6. " 등)는 보존하고 위의 빈 줄만 지운다
        let prev_ls = line_start(s, ls - 1);
        let out = replace_range(s, (prev_ls, ls), "");
        return Some(renumber_block_at(&out, prev_ls + m_chars));
    }
    let out = replace_range(s, (ls - 1, c), "");
    Some(renumber_block_at(&out, ls - 1))
}

// 빈 목록·인용 줄("- ", "1. ", "- [ ] ", "> ")에서 Delete, 또는 줄 끝에서 다음 목록·인용 줄과 합치는 Delete:
// 다음 줄의 숨긴 기호가 본문에 노출되지 않게 정리하고 번호를 다시 매긴다.
pub(crate) fn delete_empty_list_line(s: &str, c: usize) -> Option<(String, usize)> {
    let ls = line_start(s, c);
    let le = line_end(s, c);
    let lb = byte_idx(s, ls);
    let leb = byte_idx(s, le);
    let line = &s[lb..leb];
    let bstart = s[..lb].rfind("\n\n").map_or(0, |i| i + 2);
    let bend = s[leb..].find("\n\n").map_or(s.len(), |i| leb + i);
    let t = line.trim_start();
    let m = list_marker_bytes(line)
        .or_else(|| (t.starts_with('>') && callout_kind(t).is_none()).then(|| hidden_prefix(line)));
    let is_empty_marked =
        m.is_some_and(|m| line[m..].trim().is_empty() && c >= ls + char_len(&line[..m]));
    let is_bare_empty = m.is_none()
        && bstart < bend
        && s[bstart..bend].contains('\n')
        && is_bare_empty_marker_line(line);
    if is_empty_marked || is_bare_empty {
        if leb < bend {
            // 같은 블록 안 뒤에 줄이 있으면 "빈 목록 줄 + \n"을 지우고 다음 줄 내용 시작으로 커서를 둔다
            let out = replace_range(s, (ls, le + 1), "");
            let next_line_end = line_end(&out, ls);
            let next_line = &out[byte_idx(&out, ls)..byte_idx(&out, next_line_end)];
            let next_m = hidden_prefix(next_line);
            let caret = ls + char_len(&next_line[..next_m]);
            return Some(renumber_block_at(&out, caret));
        }
        if lb > bstart {
            let block = &s[bstart..bend];
            let after_title =
                callout_kind(block).is_some() && !s[bstart..lb.saturating_sub(1)].contains('\n');
            if !after_title {
                let out = replace_range(s, (ls - 1, le), "");
                return Some(renumber_block_at(&out, ls - 1));
            }
        }
        if let Some(m) = m {
            return backspace_list_marker(s, ls + char_len(&line[..m]));
        }
    }
    // 줄 맨 끝에서 Delete: 같은 블록의 바로 다음 줄이 빈 목록/인용 줄이면 그 줄을 지우고,
    // 내용이 있는 목록/인용 줄이면 숨긴 줄머리 기호를 벗겨 자연스럽게 이어 붙인다.
    if c == le && leb < bend {
        let next_ls = le + 1;
        let next_le = line_end(s, next_ls);
        let next_line = &s[byte_idx(s, next_ls)..byte_idx(s, next_le)];
        if is_bare_empty_marker_line(next_line) {
            let out = replace_range(s, (le, next_le), "");
            return Some(renumber_block_at(&out, le));
        }
        let next_m = hidden_prefix(next_line);
        if next_m > 0 && !is_collapsed_line(next_line) {
            if next_line[next_m..].trim().is_empty() {
                let out = replace_range(s, (le, next_le), "");
                return Some(renumber_block_at(&out, le));
            }
            let next_m_chars = char_len(&next_line[..next_m]);
            let out = replace_range(s, (le, next_ls + next_m_chars), "");
            return Some(renumber_block_at(&out, le));
        }
    }
    None
}

// 줄 맨 앞(숨긴 불릿 "- " 뒤나 굵게 "**" 안쪽 등)에서 새 목록 기호("1. ", "- " 등)를 쳤을 때
// 기존 목록 기호를 새 기호로 교체하거나 인라인 서식 바깥(줄 맨 앞)으로 꺼내 즉시 목록 서식화가 되게 한다.
pub(crate) fn normalize_typed_line_marker(s: &str, caret: usize) -> Option<(String, usize)> {
    let ls = line_start(s, caret);
    let le = line_end(s, caret);
    let lb = byte_idx(s, ls);
    let leb = byte_idx(s, le);
    let line = &s[lb..leb];
    let ind = line.len() - line.trim_start_matches(' ').len();
    let rest = &line[ind..];

    // 1) 할 일 체크리스트 입력 단축키: "[] ", "[ ] ", "[x] ", "- [] " 등 입력 시 즉시 체크리스트로 변환
    for bullet in ["-", "*"] {
        let short_task = format!("{bullet} [] ");
        if rest.starts_with(&short_task) && caret == ls + ind + 5 {
            let rep = format!("{bullet} [ ] ");
            let out = replace_range(s, (ls + ind, ls + ind + 5), &rep);
            return Some((out, caret + 1));
        }
    }
    if rest.starts_with("[] ") && caret == ls + ind + 3 {
        let out = replace_range(s, (ls + ind, ls + ind + 3), "- [ ] ");
        return Some((out, caret + 3));
    }
    if rest.starts_with("[ ] ") && caret == ls + ind + 4 {
        let out = replace_range(s, (ls + ind, ls + ind + 4), "- [ ] ");
        return Some((out, caret + 2));
    }
    if (rest.starts_with("[x] ") || rest.starts_with("[X] ")) && caret == ls + ind + 4 {
        let out = replace_range(s, (ls + ind, ls + ind + 4), "- [x] ");
        return Some((out, caret + 2));
    }

    // 2) 이미 목록 기호가 있는 줄에서 바로 앞뒤에 새 목록 기호나 제목·펜스·표가 들어온 경우
    //    (예: "- 1. 텍스트", "1. - 텍스트", "4. - 텍스트", "- ### 제목")
    if let Some(m1) = list_marker_bytes(rest) {
        let after_m1 = &rest[m1..];
        let m1_chars = char_len(&rest[..m1]);
        let c_after_m1 = ls + ind + m1_chars;
        if let Some(m2) = list_marker_bytes(after_m1) {
            let m2_chars = char_len(&after_m1[..m2]);
            let c_after_m2 = c_after_m1 + m2_chars;
            if caret == c_after_m2 {
                let out = replace_range(s, (ls + ind, c_after_m1), "");
                return Some((out, caret - m1_chars));
            } else if caret == c_after_m1 {
                let out = replace_range(s, (c_after_m1, c_after_m2), "");
                return Some((out, caret));
            } else {
                // 붙여넣기 등으로 커서가 줄 끝에 있는 경우: 번호+불릿("4. - ")이면 번호 유지, 불릿+번호("- 1. ")나 불릿+불릿("- - ")이면 뒤 기호 유지
                let m1_is_num = rest.as_bytes().first().is_some_and(u8::is_ascii_digit);
                let m2_is_num = after_m1.as_bytes().first().is_some_and(u8::is_ascii_digit);
                if m1_is_num && !m2_is_num {
                    let out = replace_range(s, (c_after_m1, c_after_m2), "");
                    return Some((out, caret.saturating_sub(m2_chars)));
                } else {
                    let out = replace_range(s, (ls + ind, c_after_m1), "");
                    return Some((out, caret.saturating_sub(m1_chars)));
                }
            }
        } else if heading_level(after_m1).is_some()
            || after_m1.starts_with("```")
            || after_m1.starts_with("~~~")
            || (ind == 0 && after_m1.starts_with('|') && after_m1.contains('|'))
        {
            let out = replace_range(s, (ls + ind, c_after_m1), "");
            return Some((out, caret.saturating_sub(m1_chars)));
        }
    }

    // 2) 줄이 인라인 서식 여는 기호("**", "__", "==", "~~", "*")로 시작할 때 그 바로 안쪽에서 목록/제목 기호를 친 경우
    //    (예: "**1. 텍스트**" → "1. **텍스트**")
    for open in ["**", "__", "==", "~~", "*"] {
        if let Some(after_open) = rest.strip_prefix(open) {
            if open == "*" && after_open.starts_with('*') {
                continue;
            }
            let marker_len = list_marker_bytes(after_open).or_else(|| {
                let lv = heading_level(after_open)?;
                after_open
                    .get(lv..lv + 1)
                    .is_some_and(|c| c == " ")
                    .then_some(lv + 1)
            });
            if let Some(m) = marker_len {
                let body = &after_open[m..];
                if body.contains(open) {
                    let open_chars = char_len(open);
                    let m_chars = char_len(&after_open[..m]);
                    let expected_caret = ls + ind + open_chars + m_chars;
                    if caret == expected_caret || caret == le {
                        let marker_str = &after_open[..m];
                        let swapped = format!("{marker_str}{open}");
                        let out = replace_range(s, (ls + ind, expected_caret), &swapped);
                        return Some((out, caret));
                    }
                }
            }
        }
    }
    None
}

// 외부 챗봇(ChatGPT·Claude·Gemini 등)이나 웹/문서에서 복사한 텍스트를 붙여넣을 때
// 유니코드 불릿(•, ◦, ▪)·탭 구분 기호·굵게 감싸진 연번(**1. 제목**)·블록 경계(제목/표/구분선/코드)를 정규화한다.
pub(crate) fn normalize_pasted_markdown(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    let cleaned = raw
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\u{00a0}', " ")
        .replace(['\u{200b}', '\u{feff}'], "");
    let mut norm_lines: Vec<String> = Vec::new();
    let mut in_fence = false;
    for line in cleaned.split('\n') {
        let t_check = line.trim_start();
        if t_check.starts_with("```") || t_check.starts_with("~~~") {
            in_fence = !in_fence;
            norm_lines.push(t_check.to_string());
            continue;
        }
        if in_fence {
            norm_lines.push(line.to_string());
            continue;
        }
        // 선행 탭(\t)을 2칸 공백으로 변환
        let mut ind_spaces = 0usize;
        let mut byte_consumed = 0usize;
        for ch in line.chars() {
            if ch == ' ' {
                ind_spaces += 1;
                byte_consumed += 1;
            } else if ch == '\t' {
                ind_spaces += 2;
                byte_consumed += 1;
            } else {
                break;
            }
        }
        let indent = " ".repeat(ind_spaces);
        let mut rest = line[byte_consumed..].to_string();

        // 유니코드 글머리 기호(•, ◦, ▪, ‣, ⁃, ·)나 탭이 붙은 불릿(-\t, *\t)을 표준 "- "로 변환
        for u_bullet in [
            "• ", "◦ ", "▪ ", "‣ ", "⁃ ", "· ", "•\t", "◦\t", "▪\t", "‣\t", "-\t", "*\t", "+\t",
        ] {
            if let Some(after) = rest.strip_prefix(u_bullet) {
                rest = format!("- {}", after.trim_start_matches([' ', '\t']));
                break;
            }
        }
        // 번호 뒤 탭("1.\t", "1)\t")을 공백으로 변환
        let d = rest.bytes().take_while(u8::is_ascii_digit).count();
        if (1..10).contains(&d) && (rest[d..].starts_with(".\t") || rest[d..].starts_with(")\t")) {
            let sep = &rest[d..d + 1];
            let after = rest[d + 2..].trim_start_matches([' ', '\t']);
            rest = format!("{}{sep} {after}", &rest[..d]);
        }
        // 챗봇 특유의 굵게 감싼 번호 항목("**1. 제목**" 또는 "**1. 제목:** 설명")을 "1. **제목**"으로 변환
        if let Some(after_bold) = rest.strip_prefix("**").filter(|s| !s.starts_with('*')) {
            if let Some((0, col)) = list_cols(after_bold) {
                if col > 2 && after_bold[col..].contains("**") {
                    rest = format!("{}**{}", &after_bold[..col], &after_bold[col..]);
                }
            }
        }
        // 중복 목록 기호("4. - 항목", "- - 항목") 정리
        if let Some(m1) = list_marker_bytes(&rest) {
            let after_m1 = &rest[m1..];
            if let Some(m2) = list_marker_bytes(after_m1) {
                let m1_is_num = rest.as_bytes().first().is_some_and(u8::is_ascii_digit);
                let m2_is_num = after_m1.as_bytes().first().is_some_and(u8::is_ascii_digit);
                if m1_is_num && !m2_is_num {
                    rest = format!("{}{}", &rest[..m1], &after_m1[m2..]);
                } else {
                    rest = after_m1.to_string();
                }
            }
        }
        if rest.trim().is_empty() {
            norm_lines.push(String::new());
        } else {
            norm_lines.push(format!("{indent}{rest}"));
        }
    }

    if norm_lines.len() <= 1 {
        return norm_lines.into_iter().next().unwrap_or_default();
    }

    // 다중 줄 붙여넣기 시 제목(#)·구분선(---)·표(| ... |) 앞뒤에 빈 줄이 없으면 블록이 섞이지 않게 빈 줄을 보강한다
    let n = norm_lines.len();
    let mut is_tbl = vec![false; n];
    for i in 0..n {
        if i + 1 < n
            && split_table_row(&norm_lines[i]).is_some()
            && split_table_row(&norm_lines[i + 1]).is_some_and(|cells| {
                !cells.is_empty() && cells.iter().all(|c| parse_table_sep(c).is_some())
            })
        {
            is_tbl[i] = true;
            is_tbl[i + 1] = true;
            let mut j = i + 2;
            while j < n
                && !norm_lines[j].trim().is_empty()
                && split_table_row(&norm_lines[j]).is_some()
            {
                is_tbl[j] = true;
                j += 1;
            }
        }
    }

    let mut out_lines: Vec<String> = Vec::with_capacity(n + 8);
    let mut in_fence = false;
    for (i, line) in norm_lines.into_iter().enumerate() {
        let t = line.trim_start();
        let is_fence_line = t.starts_with("```") || t.starts_with("~~~");
        let prev_non_empty = out_lines.last().is_some_and(|l| !l.trim().is_empty());
        if !in_fence && prev_non_empty && !t.is_empty() {
            let prev = out_lines.last().unwrap();
            let prev_t = prev.trim_start();
            let prev_is_heading = heading_level(prev_t).is_some();
            let cur_is_heading = heading_level(t).is_some();
            let prev_is_hr = is_hr_line(prev);
            let cur_is_hr = is_hr_line(&line);
            let prev_tbl = i > 0 && is_tbl[i - 1];
            let cur_tbl = is_tbl[i];
            if prev_is_heading
                || cur_is_heading
                || prev_is_hr
                || cur_is_hr
                || is_fence_line
                || (prev_tbl != cur_tbl)
            {
                out_lines.push(String::new());
            }
        }
        let closing_fence = in_fence && is_fence_line;
        if is_fence_line {
            in_fence = !in_fence;
        }
        out_lines.push(line);
        if closing_fence {
            out_lines.push(String::new());
        }
    }
    // 연속된 3개 이상 빈 줄은 단일 빈 줄(\n\n)로 압축
    let mut final_lines: Vec<String> = Vec::with_capacity(out_lines.len());
    for l in out_lines {
        if l.is_empty() && final_lines.last().is_some_and(String::is_empty) {
            continue;
        }
        final_lines.push(l);
    }
    final_lines.join("\n")
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
// Tab → 바로 위 형제 항목의 내용 열(또는 +2칸 하위 단계)로,
// Shift+Tab → 부모 항목의 들여쓰기(또는 -2칸 상위 단계)로. 하위 항목도 함께 움직이고 번호는 다시 매긴다.
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
    let mut merge_blank_range: Option<Range<usize>> = None;
    let target = if outdent {
        match above().find(|&(i, _)| i < ind) {
            Some((_, col)) if ind > col => (ind - 2).max(col),
            Some((i, _)) => i,
            None => ind.saturating_sub(2),
        }
    } else if let Some((i, col)) = above().find(|&(i, _)| i <= ind) {
        if i == ind {
            col
        } else {
            ind + 2
        }
    } else if above().next().is_some() {
        ind + 2
    } else {
        // 바로 위 블록이 목록(예: "1. 항목\n\n- 하위")이면 빈 줄을 합쳐 하위 계층으로 들여쓴다
        let prev_non_empty = (0..la).rev().find(|&i| !orig[i].trim().is_empty());
        if let Some(p_end) = prev_non_empty.filter(|&p| p + 1 < la && list_cols(orig[p]).is_some())
        {
            merge_blank_range = Some((p_end + 1)..la);
            match orig[..=p_end]
                .iter()
                .rev()
                .take_while(|l| !l.trim().is_empty())
                .filter_map(|l| list_cols(l))
                .find(|&(i, _)| i <= ind)
            {
                Some((i, col)) if i == ind => col,
                _ => ind + 2,
            }
        } else {
            ind + 2
        }
    };
    let delta = target as isize - ind as isize;
    if delta == 0 {
        if outdent && la < lb {
            // 첫 줄은 최상위(0칸)지만 선택에 포함된 아래 하위 항목들은 내어쓰기할 수 있다
            return None;
        }
        return Some((s.to_string(), (a, b)));
    }
    let lead = |l: &str| l.len() - l.trim_start_matches(' ').len();
    // 선택 줄 + 그 아래의 하위 항목·이어지는 줄까지 함께 이동
    let mut end = lb;
    while end + 1 < orig.len() && !orig[end + 1].trim().is_empty() && lead(orig[end + 1]) > ind {
        end += 1;
    }
    let rm_before = merge_blank_range.unwrap_or(la..la);
    let merge_inner_blanks = !outdent
        && la < end
        && orig[la..=end]
            .iter()
            .filter(|l| !l.trim().is_empty())
            .all(|l| list_cols(l).is_some());
    let mut lines: Vec<String> = Vec::with_capacity(orig.len());
    let (mut new_la, mut new_lb, mut new_end) = (0, 0, 0);
    for (i, &raw_l) in orig.iter().enumerate() {
        if rm_before.contains(&i) {
            continue;
        }
        if i > la && i < end && i != lb && merge_inner_blanks && raw_l.trim().is_empty() {
            continue;
        }
        let idx = lines.len();
        if i == la {
            new_la = idx;
        }
        if i == lb {
            new_lb = idx;
        }
        if i == end {
            new_end = idx;
        }
        let mut l = raw_l.to_string();
        if i >= la && i <= end && !l.is_empty() {
            if delta > 0 {
                l.insert_str(0, &" ".repeat(delta as usize));
            } else {
                let n = lead(&l).min((-delta) as usize);
                l.drain(..n);
            }
        }
        lines.push(l);
    }
    let mut run_start = new_la;
    while run_start > 0 {
        if !lines[run_start - 1].trim().is_empty() {
            run_start -= 1;
        } else if run_start >= 2
            && list_cols(&lines[run_start - 2]).is_some()
            && list_cols(&lines[run_start]).is_some()
        {
            run_start -= 2;
        } else {
            break;
        }
    }
    let mut run_end = new_end;
    while run_end + 1 < lines.len() {
        if !lines[run_end + 1].trim().is_empty() {
            run_end += 1;
        } else if run_end + 2 < lines.len() && list_cols(&lines[run_end + 2]).is_some() {
            run_end += 2;
        } else {
            break;
        }
    }
    renumber_lists(&mut lines[run_start..=run_end]);
    // 커서: 같은 줄에서 줄 길이 변화만큼 이동 (변화는 모두 내용 앞에서 일어난다)
    let at = |orig_line: usize, new_line: usize, col: usize| {
        let diff = char_len(&lines[new_line]) as isize - char_len(orig[orig_line]) as isize;
        let col = (col as isize + diff).clamp(0, char_len(&lines[new_line]) as isize) as usize;
        lines[..new_line]
            .iter()
            .map(|l| char_len(l) + 1)
            .sum::<usize>()
            + col
    };
    let sel = (at(la, new_la, ca), at(lb, new_lb, cb));
    Some((lines.join("\n"), sel))
}

const TASK_MARKERS: [&str; 6] = ["- [ ] ", "- [x] ", "- [X] ", "* [ ] ", "* [x] ", "* [X] "];

// 목록 줄 분석: (들여쓰기, 다음 줄에 쓸 기호, 현재 기호 길이(문자), 내용이 비었는지)
pub(crate) fn list_marker(line: &str) -> Option<(String, String, usize, bool)> {
    let indent_len = line.len() - line.trim_start_matches(' ').len();
    let (indent, rest) = line.split_at(indent_len);
    for task in TASK_MARKERS {
        if let Some(body) = rest.strip_prefix(task) {
            let next = format!("{} [ ] ", &task[..1]);
            return Some((indent.into(), next, char_len(task), body.trim().is_empty()));
        }
    }
    for bullet in ["- ", "* "] {
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
    for task in TASK_MARKERS {
        if rest.starts_with(task) {
            return Some(indent + task.len());
        }
    }
    for bullet in ["- ", "* "] {
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
    for task in TASK_MARKERS {
        if rest.starts_with(task) {
            return (indent, Some(BlockKind::Todo), task.len());
        }
    }
    for bullet in ["- ", "* "] {
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
    let mut num = 0usize;
    let multi = lines_vec.len() > 1;
    let out_lines: Vec<String> = lines_vec
        .iter()
        .enumerate()
        .map(|(i, l)| {
            if multi && l.trim().is_empty() {
                if i == 0 {
                    first_delta = 0;
                }
                return (*l).to_string();
            }
            num += 1;
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
                BlockKind::Numbered => format!("{num}. "),
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
    let na = if a != b && a == ls {
        ls
    } else {
        (a as isize + first_delta).max(ls as isize) as usize
    };
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

#[cfg(test)]
pub(crate) fn replace_all(s: &str, matches: &[Sel], with: &str) -> String {
    replace_each(s, matches.iter().map(|&m| (m, with)))
}

// 일치 구간(문자 인덱스, 오름차순)마다 다른 글자로 바꾼다
pub(crate) fn replace_each<'a>(s: &str, items: impl IntoIterator<Item = (Sel, &'a str)>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_b = 0usize;
    let mut prev_c = 0usize;
    for ((a, b), with) in items {
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

#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Debug)]
pub(crate) struct FindOpts {
    pub case: bool,
    pub word: bool,  // 단어 단위
    pub regex: bool, // 정규식
}

fn find_regex(query: &str, o: FindOpts) -> Result<regex::Regex, String> {
    let pat = if o.regex {
        query.to_string()
    } else {
        regex::escape(query)
    };
    let pat = if o.word {
        format!(r"\b(?:{pat})\b")
    } else {
        pat
    };
    regex::RegexBuilder::new(&pat)
        .case_insensitive(!o.case)
        .multi_line(true)
        .build()
        .map_err(|e| match e {
            regex::Error::Syntax(_) => "정규식 오류".to_string(),
            _ => "정규식이 너무 큽니다".to_string(),
        })
}

// 찾기 옵션을 반영한 일치 구간과 바꿀 글자. 정규식이면 $1·${이름} 캡처를 풀어 쓴다.
// 길이 0인 일치(^, a* 등)는 건너뛴다.
pub(crate) fn find_replacements(
    s: &str,
    query: &str,
    o: FindOpts,
    with: Option<&str>,
) -> Result<Vec<(Sel, String)>, String> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    if !o.word && !o.regex {
        let w = with.unwrap_or("");
        return Ok(find_all(s, query, o.case)
            .into_iter()
            .map(|m| (m, w.to_string()))
            .collect());
    }
    let re = find_regex(query, o)?;
    let mut out = Vec::new();
    let (mut last_b, mut ci) = (0, 0);
    for c in re.captures_iter(s) {
        let m = c.get(0).unwrap();
        if m.is_empty() {
            continue;
        }
        let a = ci + char_len(&s[last_b..m.start()]);
        let b = a + char_len(m.as_str());
        let rep = match with {
            Some(w) if o.regex => {
                let mut r = String::new();
                c.expand(w, &mut r);
                r
            }
            Some(w) => w.to_string(),
            None => String::new(),
        };
        out.push(((a, b), rep));
        (last_b, ci) = (m.end(), b);
    }
    Ok(out)
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
// 이 앱에서 줄머리 "+ "는 목록 기호가 아니라 글자 "+" (CommonMark와 다름). 파서에 넘기기 전 "\+"로 바꾼다.
pub(crate) fn plus_line_start(line: &str) -> Option<usize> {
    let ind = line.len() - line.trim_start_matches(' ').len();
    (ind < 4 && line[ind..].starts_with("+ ")).then_some(ind)
}

pub(crate) fn escape_plus_lines(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    let mut fence = false;
    for (i, line) in s.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = !fence;
        }
        match plus_line_start(line).filter(|_| !fence) {
            Some(ind) => {
                out.push_str(&line[..ind]);
                out.push('\\');
                out.push_str(&line[ind..]);
            }
            None => out.push_str(line),
        }
    }
    out
}

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
    let src = escape_plus_lines(&preprocess(md));
    let is_dangerous_url = |url: &str| {
        let cleaned: String = url
            .chars()
            .filter(|c| !c.is_ascii_control() && !c.is_whitespace())
            .map(|c| c.to_ascii_lowercase())
            .collect();
        cleaned.starts_with("javascript:")
            || cleaned.starts_with("vbscript:")
            || cleaned.starts_with("data:text/html")
    };
    // 문서 속 원시 HTML(<script>, onerror= 등)은 실행되지 않게 글자 그대로 보여 주고, 위험한 링크 스킴은 막는다
    let events = Parser::new_ext(&src, opts).map(|ev| match ev {
        Event::Html(t) | Event::InlineHtml(t) => Event::Text(t),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) if is_dangerous_url(&dest_url) => Event::Start(Tag::Link {
            link_type,
            dest_url: "#".into(),
            title,
            id,
        }),
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
    if t.len() <= usize::from(left) + usize::from(right) {
        return None;
    }
    let mid = &t[usize::from(left)..t.len() - usize::from(right)];
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

// 접힌 제목(folds: 제목 줄 원문) 아래, 같거나 높은 수준의 다음 제목 전까지의 슬롯을 뺀다.
// 숨는 슬롯 중 reveal이 참인 것이 있으면 그 슬롯을 가린 제목 원문을 돌려준다 (호출한 쪽이 펼친다).
// ponytail: 원문이 같은 제목은 함께 접힌다. 위치 기반 키는 모든 편집 경로에서 보정해야 해서 쓰지 않는다.
pub(crate) fn hide_folded(
    text: &str,
    slots: &mut Vec<Range<usize>>,
    folds: &std::collections::HashSet<String>,
    reveal: impl Fn(&Range<usize>) -> bool,
) -> Option<String> {
    if folds.is_empty() {
        return None;
    }
    let mut owner: Option<(usize, &str)> = None;
    let mut revealed = None;
    slots.retain(|r| {
        let raw = text.get(r.clone()).unwrap_or("");
        let lv = (!r.is_empty())
            .then(|| heading_level(raw.trim_start()))
            .flatten();
        if let (Some(l), Some((ol, _))) = (lv, owner) {
            if l <= ol {
                owner = None;
            }
        }
        if let Some((_, key)) = owner {
            if revealed.is_none() && reveal(r) {
                revealed = Some(key.to_string());
            }
            return false;
        }
        if let Some(l) = lv {
            if folds.contains(raw.trim()) {
                owner = Some((l, raw.trim()));
            }
        }
        true
    });
    revealed
}

#[cfg(test)]
mod fold_tests {
    use super::*;

    #[test]
    fn hide_folded_hides_until_same_or_higher_heading() {
        let t = "# A\n\none\n\n## B\n\ntwo\n\n# C\n\nthree";
        let blocks = split_blocks(t);
        let mut slots = collect_doc_slots(t, &blocks, None);
        let all = slots.clone();
        let folds: std::collections::HashSet<String> = ["# A".to_string()].into();
        assert_eq!(hide_folded(t, &mut slots, &folds, |_| false), None);
        let texts: Vec<&str> = slots.iter().map(|r| &t[r.clone()]).collect();
        assert_eq!(texts, ["# A", "# C", "three"]);
        // 숨은 슬롯을 드러내야 하면 가린 제목을 알려준다
        let mut s2 = all.clone();
        let two = t.find("two").unwrap();
        assert_eq!(
            hide_folded(t, &mut s2, &folds, |r| r.start == two),
            Some("# A".into())
        );
        // 하위 제목만 접으면 그 아래만 숨는다
        let mut s3 = all;
        let folds: std::collections::HashSet<String> = ["## B".to_string()].into();
        hide_folded(t, &mut s3, &folds, |_| false);
        let texts: Vec<&str> = s3.iter().map(|r| &t[r.clone()]).collect();
        assert_eq!(texts, ["# A", "one", "## B", "# C", "three"]);
    }
}

// ---------- 다중 커서 (Ctrl+클릭 / Alt+드래그 열 편집) ----------

/// 주 커서에서 일어난 편집(old → new, 주 커서 o → n)을 찾아, 주 커서와 추가 커서 모두에 똑같이 적용한다.
/// 모든 위치는 문자 인덱스. 결과: (새 텍스트, 새 추가 커서들, 주 커서 이동량 = 앞쪽 커서에서 늘어난 글자 수)
/// 타이핑·한글 조합(Preedit은 주 커서 앞 조합 글자를 바꿈)·Backspace·Delete·붙여넣기를 같은 식으로 처리한다.
/// 지울 구간이 숨긴 마크다운 기호(hidden_runs)나 줄바꿈에 걸리는 커서는 그 커서만 건너뛴다 (주 커서 포함).
/// ponytail: Vec<char> 복사 — 블록 하나 크기라 충분, 수십만 자 블록이면 바이트 인덱스로 바꿀 것
pub(crate) fn replicate_edit(
    old: &str,
    new: &str,
    o: usize,
    n: usize,
    extras: &[usize],
) -> (String, Vec<usize>, isize) {
    let hidden = hidden_runs(old);
    let oc: Vec<char> = old.chars().collect();
    let nc: Vec<char> = new.chars().collect();
    let (lo, ln) = (oc.len(), nc.len());
    let (o, n) = (o.min(lo), n.min(ln));
    // 커서 뒤는 그대로, 커서 앞도 최대한 그대로라고 보고 바뀐 구간 하나를 찾는다
    let mut s = 0;
    while s < (lo - o).min(ln - n) && oc[lo - 1 - s] == nc[ln - 1 - s] {
        s += 1;
    }
    let mut p = 0;
    while p < o.min(n).min(lo - s).min(ln - s) && oc[p] == nc[p] {
        p += 1;
    }
    let ins = &nc[p..ln - s];
    let (rel_a, rel_b) = (p as isize - o as isize, (lo - s) as isize - o as isize);

    let mut carets: Vec<(usize, bool)> = extras.iter().map(|&c| (c.min(lo), false)).collect();
    carets.push((o, true));
    carets.sort_unstable();
    carets.dedup_by_key(|c| c.0);
    let mut out = String::with_capacity(new.len() + ins.len() * extras.len());
    let (mut prev, mut out_len) = (0usize, 0usize);
    let (mut new_extras, mut shift) = (Vec::with_capacity(extras.len()), 0isize);
    for (c, is_primary) in carets {
        let a = ((c as isize + rel_a).max(0) as usize).clamp(prev, lo);
        let b = ((c as isize + rel_b).max(0) as usize).clamp(a, lo);
        let breaks = a < b
            && (oc[a..b].contains(&'\n') || hidden.iter().any(|&(ha, he, _)| a < he && ha < b));
        if breaks || c < prev {
            let c = c.max(prev);
            out.extend(&oc[prev..c]);
            out_len += c - prev;
            prev = c;
        } else {
            out.extend(&oc[prev..a]);
            out.extend(ins);
            out_len += a - prev + ins.len();
            prev = b;
        }
        if is_primary {
            shift = out_len as isize - (p + ins.len()) as isize;
        } else if new_extras.last() != Some(&out_len) {
            new_extras.push(out_len);
        }
    }
    out.extend(&oc[prev..]);
    let primary_at = (p + ins.len()) as isize + shift;
    new_extras.retain(|&c| c as isize != primary_at);
    (out, new_extras, shift)
}

/// 여러 구간(문자 인덱스, 문서 순서·겹침 없음)의 글자를 지우고 각 구간에 ins[i]를 넣는다 (ins가 하나면 모두에 같은 것).
/// 숨긴 마크다운 기호와 줄바꿈은 지우지 않고 남긴다. 결과: (새 텍스트, 각 구간에 넣은 글 바로 뒤 커서)
pub(crate) fn replace_ranges(
    s: &str,
    ranges: &[(usize, usize)],
    ins: &[&str],
) -> (String, Vec<usize>) {
    let hidden = hidden_runs(s);
    let keep = |i: usize, c: char| c == '\n' || hidden.iter().any(|&(a, e, _)| a <= i && i < e);
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let (mut prev, mut out_len, mut carets) = (0usize, 0usize, Vec::with_capacity(ranges.len()));
    for (k, &(a, b)) in ranges.iter().enumerate() {
        let txt = ins.get(k).or(ins.last()).copied().unwrap_or("");
        let a = a.clamp(prev, chars.len());
        let b = b.clamp(a, chars.len());
        out.extend(&chars[prev..a]);
        out_len += a - prev;
        let mut put = |out: &mut String, out_len: &mut usize| {
            out.push_str(txt);
            *out_len += txt.chars().count();
            carets.push(*out_len);
        };
        let mut done = false;
        for (i, &c) in chars.iter().enumerate().take(b).skip(a) {
            if keep(i, c) {
                out.push(c);
                out_len += 1;
            } else if !done {
                put(&mut out, &mut out_len);
                done = true;
            }
        }
        if !done {
            put(&mut out, &mut out_len);
        }
        prev = b;
    }
    out.extend(&chars[prev..]);
    (out, carets)
}

#[cfg(test)]
mod multi_caret_tests {
    use super::*;

    #[test]
    fn replace_ranges_deletes_visible_text_and_inserts_per_range() {
        // 사각형 선택 지우기: 각 줄 1~3열
        let (t, cs) = replace_ranges("abcd\nefgh\nij", &[(1, 3), (6, 8), (11, 12)], &[""]);
        assert_eq!((t.as_str(), cs), ("ad\neh\ni", vec![1, 4, 7]));
        // 줄마다 다른 글 넣기 (여러 줄 붙여넣기 나눠 넣기)
        let (t, cs) = replace_ranges("ab\ncd", &[(1, 1), (4, 4)], &["X", "YY"]);
        assert_eq!((t.as_str(), cs), ("aXb\ncYYd", vec![2, 7]));
        // 숨긴 기호(목록 기호, **)는 남는다
        let (t, _) = replace_ranges("- ab\n**cd**", &[(0, 4), (5, 11)], &["Z"]);
        assert_eq!(t, "- Z\n**Z**");
    }

    #[test]
    fn replicate_edit_applies_primary_edit_at_every_caret() {
        // 세 줄 맨 앞에 커서: 주 커서는 3번째 줄 앞(8), 추가 커서 0, 4
        let old = "abc\ndef\nghi";
        // 타이핑 "- "
        let (t, ex, sh) = replicate_edit(old, "abc\ndef\n- ghi", 8, 10, &[0, 4]);
        assert_eq!(t, "- abc\n- def\n- ghi");
        assert_eq!((ex, sh), (vec![2, 8], 4));
        // Backspace (각 줄 끝에서): 주 커서 11 → 10
        let (t, ex, sh) = replicate_edit(old, "abc\ndef\ngh", 11, 10, &[3, 7]);
        assert_eq!(t, "ab\nde\ngh");
        assert_eq!((ex, sh), (vec![2, 5], -2));
        // Delete (각 줄 맨 앞에서)
        let (t, ex, _) = replicate_edit(old, "abc\ndef\nhi", 8, 8, &[0, 4]);
        assert_eq!((t.as_str(), ex), ("bc\nef\nhi", vec![0, 3]));
        // 같은 글자를 이어 치는 경우도 커서 기준으로 정확히 (aa| + a)
        let (t, ex, _) = replicate_edit("aa\naa", "aa\naaa", 5, 6, &[2]);
        assert_eq!((t.as_str(), ex), ("aaa\naaa", vec![3]));
        // 한글 조합: Preedit "ㅎ" → "하" (주 커서는 조합 글자 뒤)
        let (t, ex, _) = replicate_edit("xㅎ\nyㅎ", "xㅎ\ny하", 5, 5, &[2]);
        assert_eq!((t.as_str(), ex), ("x하\ny하", vec![2]));
        // 주 커서가 앞에 있으면 이동량 0, 뒤 커서만 밀린다
        let (t, ex, sh) = replicate_edit("ab", "Xab", 0, 1, &[2]);
        assert_eq!((t.as_str(), ex, sh), ("XabX", vec![4], 0));
        // 커서끼리 지우다 겹치면 하나로 합친다
        let (t, ex, _) = replicate_edit("ab", "b", 1, 0, &[2]);
        assert_eq!((t.as_str(), ex), ("", vec![]));
        // 숨긴 기호("- ")·줄바꿈을 지우게 되는 커서는 건너뛴다: 목록 내용 앞 Backspace, 줄 끝 Delete
        let (t, ex, sh) = replicate_edit("- a1\n- b2", "- a1\n- 2", 9, 8, &[2]);
        assert_eq!((t.as_str(), ex, sh), ("- a1\n- 2", vec![2], 0));
        let (t, ex, _) = replicate_edit("ab\ncd", "abcd", 2, 2, &[5]);
        assert_eq!((t.as_str(), ex), ("ab\ncd", vec![5]));
    }
}

#[cfg(test)]
mod plus_tests {
    use super::*;

    #[test]
    fn plus_is_plain_text_not_a_bullet() {
        assert_eq!(
            escape_plus_lines("+ a\n  + b\n```\n+ c\n```"),
            r"\+ a
  \+ b
```
+ c
```"
        );
        assert_eq!(list_marker_bytes("+ a"), None);
        assert_eq!(detect_line_kind("+ a").1, None);
        assert!(normalize_typed_line_marker("+ [] a", 5).is_none());
        let html = export_html("t", "+ 하나\n+ 둘\n\n- 진짜 목록");
        assert!(
            html.contains("+ 하나") && html.matches("<li>").count() == 1,
            "{html}"
        );
    }
}
