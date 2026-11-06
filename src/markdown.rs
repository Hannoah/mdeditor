// 경량 Markdown -> HTML 변환기 (제로 의존성)
// 지원: 제목(#), 굵게(**), 기울임(*), 굵은 기울임(***), 취소선(~~), 인라인 코드(`), 코드블록(```),
//       링크([]()), 이미지(![]()), 목록(-, *, +, 1.; 들여쓰기 중첩, 태스크 체크박스 [ ]/[x]), 인용(>; 재귀),
//       표, 구분선, 단락(연속 줄 병합, 줄 끝 공백 2개 = 줄바꿈), 백슬래시 이스케이프
// 미지원: _밑줄_ 강조, 참조 링크

const MAX_QUOTE_DEPTH: usize = 8; // 인용 재귀 상한 (스택 오버플로 방지)

pub fn to_html(input: &str) -> String {
    render(input, 0)
}

fn render(input: &str, depth: usize) -> String {
    let lines: Vec<&str> = input.lines().collect();
    let mut out = String::new();
    let mut para: Vec<&str> = Vec::new();
    let mut lists: Vec<(usize, bool)> = Vec::new(); // (들여쓰기, 순서있음)
    let mut blank_in_list = false;
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();

        // 빈 줄 -> 단락 종료 (목록은 다음 줄을 보고 결정)
        if trimmed.is_empty() {
            flush_para(&mut out, &mut para);
            if !lists.is_empty() {
                blank_in_list = true;
            }
            i += 1;
            continue;
        }
        let had_blank = std::mem::take(&mut blank_in_list);
        let indent = indent_of(line);
        let item = list_item(trimmed);

        // 목록 항목이 아닌 줄: 항목의 이어지는 문장이거나 목록 종료
        if item.is_none() && !lists.is_empty() {
            if !is_block_start(trimmed) && (!had_blank || indent > 0) {
                out.push('\n');
                out.push_str(&inline(trimmed.trim_end()));
                i += 1;
                continue;
            }
            close_lists(&mut out, &mut lists);
        }

        // 코드블록 ```
        if trimmed.starts_with("```") {
            flush_para(&mut out, &mut para);
            i += 1;
            let mut code = String::new();
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                code.push_str(&escape_html(lines[i]));
                code.push('\n');
                i += 1;
            }
            out.push_str("<pre><code>");
            out.push_str(&code);
            out.push_str("</code></pre>\n");
            i += 1; // 닫는 ``` 건너뛰기
            continue;
        }

        // 표 (GitHub 스타일 파이프 테이블)
        // 현재 줄에 '|'가 있고, 다음 줄이 구분행이며, 열 수가 같을 때만 표로 처리
        if line.contains('|')
            && i + 1 < lines.len()
            && is_table_separator(lines[i + 1])
            && split_table_row(line).len() == split_table_row(lines[i + 1]).len()
        {
            flush_para(&mut out, &mut para);
            let aligns = parse_table_aligns(lines[i + 1]);
            let header = line;
            i += 2; // 헤더행 + 구분행 소비

            out.push_str("<table>\n<thead>\n<tr>");
            for (idx, cell) in split_table_row(header).iter().enumerate() {
                let align = aligns.get(idx).copied().unwrap_or(Align::None);
                out.push_str(&format!("<th{}>{}</th>", align_attr(align), inline(cell.trim())));
            }
            out.push_str("</tr>\n</thead>\n<tbody>\n");

            while i < lines.len() && lines[i].contains('|') && !lines[i].trim().is_empty() {
                out.push_str("<tr>");
                for (idx, cell) in split_table_row(lines[i]).iter().enumerate() {
                    let align = aligns.get(idx).copied().unwrap_or(Align::None);
                    out.push_str(&format!("<td{}>{}</td>", align_attr(align), inline(cell.trim())));
                }
                out.push_str("</tr>\n");
                i += 1;
            }

            out.push_str("</tbody>\n</table>\n");
            continue;
        }

        // 구분선 --- *** ___ (3개 이상)
        if is_hr(trimmed) {
            flush_para(&mut out, &mut para);
            out.push_str("<hr/>\n");
            i += 1;
            continue;
        }

        // 제목 #
        if let Some(level) = heading_level(trimmed) {
            flush_para(&mut out, &mut para);
            let text = trimmed[level..].trim();
            out.push_str(&format!("<h{0}>{1}</h{0}>\n", level, inline(text)));
            i += 1;
            continue;
        }

        // 인용 > : 연속된 '>' 줄을 모아 내용을 재귀 렌더링 (목록/코드/중첩 인용 지원)
        if trimmed.starts_with('>') && depth < MAX_QUOTE_DEPTH {
            flush_para(&mut out, &mut para);
            let mut inner: Vec<&str> = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with('>') {
                let rest = &lines[i].trim_start()[1..];
                inner.push(rest.strip_prefix(' ').unwrap_or(rest));
                i += 1;
            }
            out.push_str("<blockquote>\n");
            out.push_str(&render(&inner.join("\n"), depth + 1));
            out.push_str("</blockquote>\n");
            continue;
        }

        // 목록 항목 (들여쓰기로 중첩)
        if let Some((ordered, start, rest)) = item {
            flush_para(&mut out, &mut para);
            // 더 깊은 목록은 닫는다
            while lists.last().is_some_and(|&(ind, _)| ind > indent) {
                close_top_list(&mut out, &mut lists);
            }
            match lists.last().copied() {
                // 같은 단계 (들여쓰기 차이 2칸 미만은 같은 단계로 간주)
                Some((ind, ord)) if indent < ind + 2 => {
                    if ord == ordered {
                        out.push_str("</li>\n");
                    } else {
                        close_top_list(&mut out, &mut lists);
                        open_list(&mut out, &mut lists, indent, ordered, start);
                    }
                }
                // 더 깊게 들여쓰면 현재 <li> 안에 새 목록, 목록이 없으면 새로 시작
                _ => open_list(&mut out, &mut lists, indent, ordered, start),
            }
            let (item_html, is_task) = parse_task_item(rest.trim_end());
            if is_task {
                out.push_str("<li class=\"task-list-item\">");
            } else {
                out.push_str("<li>");
            }
            out.push_str(&item_html);
            i += 1;
            continue;
        }

        // 일반 단락: 연속된 줄을 하나의 <p> 로 모은다
        para.push(trimmed);
        i += 1;
    }

    // 열린 블록 닫기
    flush_para(&mut out, &mut para);
    close_lists(&mut out, &mut lists);
    out
}

fn flush_para(out: &mut String, para: &mut Vec<&str>) {
    if para.is_empty() {
        return;
    }
    out.push_str("<p>");
    let n = para.len();
    for (k, l) in para.iter().enumerate() {
        out.push_str(&inline(l.trim_end()));
        if k + 1 < n {
            // 줄 끝 공백 2개 이상 = 강제 줄바꿈, 아니면 공백처럼 이어붙임
            out.push_str(if l.ends_with("  ") { "<br/>\n" } else { "\n" });
        }
    }
    out.push_str("</p>\n");
    para.clear();
}

fn open_list(out: &mut String, lists: &mut Vec<(usize, bool)>, indent: usize, ordered: bool, start: u32) {
    match (ordered, start) {
        (false, _) => out.push_str("<ul>\n"),
        (true, 1) => out.push_str("<ol>\n"),
        (true, n) => out.push_str(&format!("<ol start=\"{n}\">\n")),
    }
    lists.push((indent, ordered));
}

fn close_top_list(out: &mut String, lists: &mut Vec<(usize, bool)>) {
    if let Some((_, ordered)) = lists.pop() {
        out.push_str(if ordered { "</li>\n</ol>\n" } else { "</li>\n</ul>\n" });
    }
}

fn close_lists(out: &mut String, lists: &mut Vec<(usize, bool)>) {
    while !lists.is_empty() {
        close_top_list(out, lists);
    }
}

// 탭은 4칸으로 계산
fn indent_of(line: &str) -> usize {
    line.chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum()
}

fn is_hr(t: &str) -> bool {
    let t = t.trim_end();
    let first = t.chars().next();
    t.chars().count() >= 3 && matches!(first, Some('-' | '*' | '_')) && t.chars().all(|c| Some(c) == first)
}

// 단락/목록 항목을 끊고 새 블록을 시작하는 줄인가
fn is_block_start(t: &str) -> bool {
    t.starts_with("```") || t.starts_with('>') || is_hr(t) || heading_level(t).is_some() || list_item(t).is_some()
}

fn heading_level(line: &str) -> Option<usize> {
    let count = line.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&count) && line[count..].starts_with(' ') {
        Some(count)
    } else {
        None
    }
}

// 목록 항목: (순서있음, 시작 번호, 내용)
fn list_item(line: &str) -> Option<(bool, u32, &str)> {
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(marker) {
            return Some((false, 0, rest));
        }
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    if (1..=9).contains(&digits) {
        if let Some(rest) = line[digits..].strip_prefix(". ") {
            return Some((true, line[..digits].parse().unwrap_or(1), rest));
        }
    }
    None
}

// 태스크 체크박스: [ ] 또는 [x] / [X]
fn parse_task_item(rest: &str) -> (String, bool) {
    if let Some(after) = rest.strip_prefix("[ ] ") {
        (format!("<input type=\"checkbox\" disabled> {}", inline(after.trim_end())), true)
    } else if let Some(after) = rest.strip_prefix("[x] ").or_else(|| rest.strip_prefix("[X] ")) {
        (format!("<input type=\"checkbox\" checked disabled> {}", inline(after.trim_end())), true)
    } else {
        (inline(rest), false)
    }
}

// ---------- 인라인 ----------
// 처리 순서: 코드 스팬 -> 백슬래시 이스케이프 -> HTML 이스케이프 -> 링크/이미지 -> 강조.
// 코드/이스케이프/링크 태그는 \u{1}N\u{1} 자리표시자로 빼 두었다가 마지막에 복원한다.
// 그래서 코드 안의 '*' 나 URL 안의 '*' 가 강조로 오인되지 않는다.
fn inline(text: &str) -> String {
    let text = text.replace('\u{1}', "");
    let mut stash: Vec<String> = Vec::new();

    // 1) 코드 스팬: 짝이 맞는 `...` 만 처리, 짝 없는 ` 는 그대로 둔다
    let mut s = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(a) = rest.find('`') {
        match rest[a + 1..].find('`') {
            Some(len) => {
                s.push_str(&rest[..a]);
                let code = format!("<code>{}</code>", escape_html(&rest[a + 1..a + 1 + len]));
                s.push_str(&hold(&mut stash, code));
                rest = &rest[a + len + 2..];
            }
            None => break,
        }
    }
    s.push_str(rest);

    // 2) 백슬래시 이스케이프: \* \[ \\ 등
    let mut esc = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, chars.peek().copied()) {
            ('\\', Some(n)) if "\\*_[]()#+-.!|>~".contains(n) => {
                esc.push_str(&hold(&mut stash, escape_html(&n.to_string())));
                chars.next();
            }
            _ => esc.push(c),
        }
    }

    // 3) 나머지 텍스트 HTML 이스케이프 후 링크/이미지, 강조
    let s = replace_links(&escape_html(&esc), &mut stash);
    let s = replace_delim(&s, "***", "<strong><em>", "</em></strong>");
    let s = replace_delim(&s, "**", "<strong>", "</strong>");
    let s = replace_delim(&s, "*", "<em>", "</em>");
    let s = replace_delim(&s, "~~", "<del>", "</del>");

    restore(&s, &stash)
}

fn hold(stash: &mut Vec<String>, html: String) -> String {
    stash.push(html);
    format!("\u{1}{}\u{1}", stash.len() - 1)
}

fn restore(s: &str, stash: &[String]) -> String {
    s.split('\u{1}')
        .enumerate()
        .map(|(n, seg)| {
            if n % 2 == 1 {
                seg.parse::<usize>().ok().and_then(|k| stash.get(k)).map_or("", |h| h.as_str())
            } else {
                seg
            }
        })
        .collect()
}

// 여는 구분자 뒤와 닫는 구분자 앞은 공백이 아니어야 하고, 짝이 없으면 문자 그대로 둔다
// (짝 없는 태그가 이후 문서 전체로 번지는 것을 막는다)
fn replace_delim(s: &str, delim: &str, open: &str, close: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(a) = rest.find(delim) {
        let after = &rest[a + delim.len()..];
        let opens = after.chars().next().is_some_and(|c| !c.is_whitespace());
        match opens.then(|| find_close(after, delim)).flatten() {
            Some(b) => {
                out.push_str(&rest[..a]);
                out.push_str(open);
                out.push_str(&after[..b]);
                out.push_str(close);
                rest = &after[b + delim.len()..];
            }
            None => {
                out.push_str(&rest[..a + delim.len()]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn find_close(s: &str, delim: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(p) = s[from..].find(delim) {
        let b = from + p;
        if b > 0 && !s[..b].ends_with(char::is_whitespace) {
            return Some(b);
        }
        from = b + delim.len();
    }
    None
}

// 안전한 링크 스킴만 허용 (javascript:, data: 등 차단)
fn safe_link(url: &str) -> bool {
    let u = url.trim().to_ascii_lowercase();
    u.starts_with("http://") || u.starts_with("https://") || u.starts_with("mailto:") || u.starts_with('#')
}

fn safe_image(url: &str) -> bool {
    let u = url.trim().to_ascii_lowercase();
    u.starts_with("http://") || u.starts_with("https://")
}

// [text](url) -> <a>text</a>, ![alt](url) -> <img>. 입력은 이미 HTML 이스케이프된 상태.
// <a ...> / </a> 는 자리표시자로 빼서 URL 이 강조 처리에 노출되지 않게 한다.
fn replace_links(s: &str, stash: &mut Vec<String>) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let img = chars[i] == '!' && chars.get(i + 1) == Some(&'[');
        let open = if img { i + 1 } else { i };
        if chars[open] == '[' {
            if let Some(close_br) = find_char(&chars, open + 1, ']') {
                if chars.get(close_br + 1) == Some(&'(') {
                    if let Some(close_par) = find_close_paren(&chars, close_br + 2) {
                        let text: String = chars[open + 1..close_br].iter().collect();
                        let url: String = chars[close_br + 2..close_par].iter().collect();
                        if img && safe_image(&url) {
                            let tag = format!("<img src=\"{}\" alt=\"{}\">", url.trim(), text);
                            out.push_str(&hold(stash, tag));
                            i = close_par + 1;
                            continue;
                        } else if !img && safe_link(&url) {
                            let a = format!("<a href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">", url.trim());
                            out.push_str(&hold(stash, a));
                            out.push_str(&text);
                            out.push_str(&hold(stash, "</a>".to_string()));
                            i = close_par + 1;
                            continue;
                        }
                    }
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn find_char(chars: &[char], start: usize, target: char) -> Option<usize> {
    chars.iter().skip(start).position(|&c| c == target).map(|p| p + start)
}

// 괄호 짝을 세어 닫는 ')' 위치를 찾는다 (URL 안의 (..) 허용)
fn find_close_paren(chars: &[char], start: usize) -> Option<usize> {
    let mut depth = 0;
    for (k, &c) in chars.iter().enumerate().skip(start) {
        match c {
            '(' => depth += 1,
            ')' if depth == 0 => return Some(k),
            ')' => depth -= 1,
            _ => {}
        }
    }
    None
}

// ---------- 표(테이블) 처리 ----------
#[derive(Clone, Copy, PartialEq)]
enum Align {
    None,
    Left,
    Center,
    Right,
}

fn align_attr(a: Align) -> &'static str {
    match a {
        Align::None => "",
        Align::Left => " style=\"text-align:left\"",
        Align::Center => " style=\"text-align:center\"",
        Align::Right => " style=\"text-align:right\"",
    }
}

// 구분행 판별: 셀이 모두 '-'와 ':'와 공백으로만 이뤄지고 '-'를 최소 1개 포함
fn is_table_separator(line: &str) -> bool {
    let t = line.trim();
    if !t.contains('|') && !t.contains('-') {
        return false;
    }
    let cells = split_table_row(line);
    if cells.is_empty() {
        return false;
    }
    cells.iter().all(|cell| {
        let c = cell.trim();
        !c.is_empty() && c.contains('-') && c.chars().all(|ch| ch == '-' || ch == ':' || ch == ' ')
    })
}

// 구분행에서 각 열의 정렬 파싱
fn parse_table_aligns(line: &str) -> Vec<Align> {
    split_table_row(line)
        .iter()
        .map(|cell| {
            let c = cell.trim();
            match (c.starts_with(':'), c.ends_with(':')) {
                (true, true) => Align::Center,
                (true, false) => Align::Left,
                (false, true) => Align::Right,
                (false, false) => Align::None,
            }
        })
        .collect()
}

// 한 행을 '|' 기준으로 분리 (앞뒤 테두리 파이프 제거, \| 이스케이프 및 코드 스팬 안의 | 보호 지원)
fn split_table_row(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);

    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut chars = t.chars().peekable();
    let mut in_code = false;
    while let Some(c) = chars.next() {
        if c == '`' {
            in_code = !in_code;
            cur.push(c);
        } else if c == '\\' {
            if chars.peek() == Some(&'|') {
                cur.push('|');
                chars.next();
                continue;
            }
            cur.push('\\');
        } else if c == '|' && !in_code {
            cells.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    cells.push(cur);
    cells
}

fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_renders() {
        let md = "| 이름 | 나이 |\n| :--- | :---: |\n| 홍길동 | 30 |\n";
        let html = to_html(md);
        assert!(html.contains("<table>"), "table 태그 없음: {html}");
        assert!(html.contains("<thead>"), "thead 없음");
        assert!(html.contains("<th style=\"text-align:left\">이름</th>"), "좌측정렬 th 없음: {html}");
        assert!(html.contains("<th style=\"text-align:center\">나이</th>"), "가운데정렬 th 없음: {html}");
        assert!(html.contains("<td style=\"text-align:left\">홍길동</td>"), "td 없음: {html}");
        // 표가 hr 로 오인되지 않아야 함
        assert!(!html.contains("<hr/>"), "구분행이 hr 로 잘못 처리됨");
    }

    #[test]
    fn table_with_code_span_pipe() {
        let md = "| 명령어 | 설명 |\n| --- | --- |\n| `a | b` | 파이프 테스트 |\n";
        let html = to_html(md);
        assert!(html.contains("<table>"), "table 태그 없음: {html}");
        assert!(html.contains("<code>a | b</code>"), "코드 내 파이프 보존 실패: {html}");
        assert!(html.contains("<td>파이프 테스트</td>"), "열 분리 실패: {html}");
    }

    #[test]
    fn pipe_line_before_hr_is_not_a_table() {
        let html = to_html("a | b\n---\n");
        assert!(!html.contains("<table>"), "{html}");
        assert!(html.contains("<hr/>"), "{html}");
    }

    #[test]
    fn unmatched_delimiters_stay_literal() {
        // 짝 없는 * ` 가 열린 태그로 남아 이후 문서를 오염시키면 안 된다
        for md in ["2 * 3 * 4", "a * b", "**bold", "`code", "*x"] {
            let html = to_html(md);
            assert_eq!(html.matches("<em>").count(), html.matches("</em>").count(), "{md}: {html}");
            assert_eq!(html.matches("<strong>").count(), html.matches("</strong>").count(), "{md}: {html}");
            assert_eq!(html.matches("<code>").count(), html.matches("</code>").count(), "{md}: {html}");
        }
        assert!(to_html("2 * 3 * 4").contains("2 * 3 * 4"));
    }

    #[test]
    fn emphasis_works() {
        assert!(to_html("*a* **b** ***c***").contains("<em>a</em> <strong>b</strong> <strong><em>c</em></strong>"));
        assert!(to_html("**`x`**").contains("<strong><code>x</code></strong>"));
    }

    #[test]
    fn code_span_is_literal() {
        let html = to_html("`*.md` 와 `a*b*c` 와 `[x](y)`");
        assert!(html.contains("<code>*.md</code>"), "{html}");
        assert!(html.contains("<code>a*b*c</code>"), "{html}");
        assert!(html.contains("<code>[x](y)</code>"), "{html}");
        assert!(!html.contains("<em>") && !html.contains("<a "), "{html}");
    }

    #[test]
    fn dangerous_links_are_blocked() {
        for md in ["[x](javascript:alert(1))", "[x](JaVaScRiPt:alert(1))", "[x](data:text/html,hi)", "[x]( vbscript:x)"] {
            assert!(!to_html(md).contains("<a "), "{md}");
        }
        let ok = to_html("[x](https://a.com/p?q=1&r=2) [y](#h) [z](mailto:a@b.c)");
        assert_eq!(ok.matches("<a href=").count(), 3, "{ok}");
        assert!(ok.contains("&amp;r=2"), "{ok}");
        assert!(ok.contains("rel=\"noopener noreferrer\""), "{ok}");
    }

    #[test]
    fn link_url_is_not_touched_by_emphasis_and_allows_parens() {
        let html = to_html("[a](https://x.com/a*b*c_(d)) *e*");
        assert!(html.contains("href=\"https://x.com/a*b*c_(d)\""), "{html}");
        assert!(html.contains("<em>e</em>"), "{html}");
    }

    #[test]
    fn html_is_escaped_and_images_restricted() {
        assert!(to_html("<script>alert(1)</script>").contains("&lt;script&gt;"));
        assert!(to_html("![a](https://x/y.png)").contains("<img src=\"https://x/y.png\" alt=\"a\">"));
        assert!(!to_html("![a](javascript:x)").contains("<img"));
        assert!(!to_html("![a](\"onerror=x)").contains("<img"));
    }

    #[test]
    fn backslash_escapes() {
        let html = to_html("\\*not em\\* and \\[x\\](https://a.com)");
        assert!(!html.contains("<em>") && !html.contains("<a "), "{html}");
        assert!(html.contains("*not em*"), "{html}");
    }

    #[test]
    fn nested_lists() {
        let html = to_html("- a\n  - b\n    - c\n- d\n");
        assert_eq!(html.matches("<ul>").count(), 3, "{html}");
        assert_eq!(html.matches("<ul>").count(), html.matches("</ul>").count(), "{html}");
        assert_eq!(html.matches("<li>").count(), html.matches("</li>").count(), "{html}");
        assert!(html.contains("<li>a<ul>"), "{html}");
        assert!(html.contains("</ul>\n</li>\n<li>d"), "{html}");
    }

    #[test]
    fn ordered_list_keeps_numbering_across_blank_lines() {
        let html = to_html("1. a\n\n2. b\n\n3. c\n");
        assert_eq!(html.matches("<ol").count(), 1, "{html}");
        assert_eq!(html.matches("<li>").count(), 3, "{html}");
        assert!(to_html("3. x\n4. y").contains("<ol start=\"3\">"));
    }

    #[test]
    fn list_type_switch_and_continuation() {
        let html = to_html("- a\n1. b\n");
        assert!(html.contains("<ul>") && html.contains("<ol>"), "{html}");
        let html = to_html("- a\n  more\n- b\n");
        assert!(html.contains("<li>a\nmore</li>"), "{html}");
    }

    #[test]
    fn paragraph_lines_merge() {
        let html = to_html("one\ntwo\n\nthree  \nfour\n");
        assert_eq!(html.matches("<p>").count(), 2, "{html}");
        assert!(html.contains("one\ntwo"), "{html}");
        assert!(html.contains("three<br/>\nfour"), "{html}");
    }

    #[test]
    fn blockquote_merges_and_nests() {
        let html = to_html("> a\n> b\n>\n> - x\n> > deep\n");
        assert_eq!(html.matches("<blockquote>").count(), 2, "{html}");
        assert_eq!(html.matches("<blockquote>").count(), html.matches("</blockquote>").count(), "{html}");
        assert!(html.contains("<p>a\nb</p>"), "{html}");
        assert!(html.contains("<li>x"), "{html}");
    }

    #[test]
    fn deep_quote_does_not_overflow_stack() {
        let md = ">".repeat(100_000);
        let _ = to_html(&md);
    }

    #[test]
    fn hr_and_headings() {
        assert!(to_html("-----").contains("<hr/>"));
        assert!(to_html("***").contains("<hr/>"));
        assert!(to_html("## T").contains("<h2>T</h2>"));
        assert!(!to_html("#tag").contains("<h1>"));
    }

    #[test]
    fn code_block_is_escaped_and_unclosed_fence_ok() {
        assert!(to_html("```\n<b>*x*</b>\n```").contains("<pre><code>&lt;b&gt;*x*&lt;/b&gt;\n</code></pre>"));
        let _ = to_html("```\nnever closed");
    }

    #[test]
    fn strikethrough_works() {
        assert!(to_html("~~취소선~~").contains("<del>취소선</del>"));
        assert!(to_html("~~a **b** c~~").contains("<del>a <strong>b</strong> c</del>"));
        assert_eq!(to_html("~not del~"), "<p>~not del~</p>\n");
        assert_eq!(to_html("~~unmatched"), "<p>~~unmatched</p>\n");
    }

    #[test]
    fn task_list_works() {
        let md = "- [ ] 할 일 1\n- [x] 완료된 일 2\n* [X] 완료된 일 3\n- 일반 항목\n";
        let html = to_html(md);
        assert!(html.contains("<li class=\"task-list-item\"><input type=\"checkbox\" disabled> 할 일 1</li>"), "{html}");
        assert!(html.contains("<li class=\"task-list-item\"><input type=\"checkbox\" checked disabled> 완료된 일 2</li>"), "{html}");
        assert!(html.contains("<li class=\"task-list-item\"><input type=\"checkbox\" checked disabled> 완료된 일 3</li>"), "{html}");
        assert!(html.contains("<li>일반 항목</li>"), "{html}");
    }
}
