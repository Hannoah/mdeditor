// 경량 Markdown -> HTML 변환기 (제로 의존성)
// 지원: 제목(#, 슬러그 id), 굵게(**), 기울임(*), 굵은 기울임(***), 취소선(~~), 윗첨자(^), 인라인 코드(`),
//       코드블록(``` 및 ```mermaid 다이어그램), 링크([]()), 이미지(![]() 로컬/원격),
//       목록(-, *, +, 1.; 들여쓰기 중첩, 태스크 체크박스 [ ]/[x]), 인용(>; 재귀, 콜아웃),
//       각주([^1] 및 [^1]: 본문), 표, 구분선, 단락(연속 줄 병합, 줄 끝 공백 2개 = 줄바꿈), 백슬래시 이스케이프
// 미지원: _밑줄_ 강조, 참조 링크

use std::collections::HashMap;

const MAX_QUOTE_DEPTH: usize = 8; // 인용 재귀 상한 (스택 오버플로 방지)

pub fn to_html(input: &str) -> String {
    let raw_lines: Vec<&str> = input.lines().collect();
    let (lines, footnotes) = extract_footnotes(&raw_lines);
    let mut slug_counts: HashMap<String, usize> = HashMap::new();
    let mut html = render_lines(&lines, 0, &mut slug_counts);

    if !footnotes.is_empty() {
        html.push_str("\n<hr class=\"footnotes-sep\"/>\n<section class=\"footnotes\">\n<ol>\n");
        for (id, text) in &footnotes {
            let id_escaped = escape_html(id);
            let rendered_text = inline(text);
            html.push_str(&format!(
                "<li id=\"fn-{0}\">{1} <a href=\"#fnref-{0}\" class=\"footnote-backref\">↩</a></li>\n",
                id_escaped, rendered_text
            ));
        }
        html.push_str("</ol>\n</section>\n");
    }

    html
}

fn extract_footnotes<'a>(lines: &[&'a str]) -> (Vec<&'a str>, Vec<(String, String)>) {
    let mut cleaned = Vec::with_capacity(lines.len());
    let mut footnotes = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.starts_with("[^") {
            if let Some(colon) = trimmed.find("]:") {
                let id = trimmed[2..colon].trim().to_string();
                let mut def = trimmed[colon + 2..].trim().to_string();
                i += 1;
                while i < lines.len() && (lines[i].starts_with("    ") || lines[i].starts_with('\t')) {
                    def.push(' ');
                    def.push_str(lines[i].trim());
                    i += 1;
                }
                footnotes.push((id, def));
                continue;
            }
        }
        cleaned.push(lines[i]);
        i += 1;
    }
    (cleaned, footnotes)
}

fn render(input: &str, depth: usize, slug_counts: &mut HashMap<String, usize>) -> String {
    let lines: Vec<&str> = input.lines().collect();
    render_lines(&lines, depth, slug_counts)
}

fn render_lines(lines: &[&str], depth: usize, slug_counts: &mut HashMap<String, usize>) -> String {
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

        // 코드블록 ```lang
        if trimmed.starts_with("```") {
            flush_para(&mut out, &mut para);
            let lang = trimmed.strip_prefix("```").unwrap_or("").trim();
            i += 1;
            let mut code = String::new();
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                code.push_str(&escape_html(lines[i]));
                code.push('\n');
                i += 1;
            }

            if lang.eq_ignore_ascii_case("mermaid") {
                out.push_str("<div class=\"code-block-wrapper mermaid-wrapper\">\n<div class=\"code-block-header\"><span class=\"code-lang\">Mermaid diagram</span><button class=\"copy-code-btn\" type=\"button\" onclick=\"copyCode(this)\">복사</button></div>\n<div class=\"mermaid\">\n");
                out.push_str(&code);
                out.push_str("</div>\n<pre style=\"display:none\"><code class=\"language-mermaid\">");
                out.push_str(&code);
                out.push_str("</code></pre>\n</div>\n");
            } else {
                let lang_label = if lang.is_empty() { "Text".to_string() } else { capitalize_first(lang) };
                let code_class = if lang.is_empty() { String::new() } else { format!(" class=\"language-{}\"", escape_html(lang)) };
                out.push_str(&format!(
                    "<div class=\"code-block-wrapper\">\n<div class=\"code-block-header\"><span class=\"code-lang\">{}</span><button class=\"copy-code-btn\" type=\"button\" onclick=\"copyCode(this)\">복사</button></div>\n<pre><code{}>",
                    escape_html(&lang_label),
                    code_class
                ));
                out.push_str(&code);
                out.push_str("</code></pre>\n</div>\n");
            }
            if i < lines.len() {
                i += 1; // 닫는 ``` 건너뛰기
            }
            continue;
        }

        // 블록 수식 $$ ... $$ (노션/LaTeX 스타일)
        if let Some(rest) = trimmed.strip_prefix("$$") {
            flush_para(&mut out, &mut para);
            if rest.ends_with("$$") && rest.len() >= 2 {
                let formula = rest[..rest.len() - 2].trim();
                out.push_str(&format!("<div class=\"math-block\">$${}$$</div>\n", escape_html(formula)));
                i += 1;
                continue;
            }
            i += 1;
            let mut math_lines = Vec::new();
            if !rest.trim().is_empty() {
                math_lines.push(rest);
            }
            while i < lines.len() && !lines[i].trim_start().starts_with("$$") {
                math_lines.push(lines[i]);
                i += 1;
            }
            if i < lines.len() {
                i += 1; // 닫는 $$ 건너뛰기
            }
            let formula = math_lines.join("\n");
            out.push_str(&format!("<div class=\"math-block\">$${}$$</div>\n", escape_html(formula.trim())));
            continue;
        }

        // 토글 / 접기 블록 :::toggle 제목 ... :::
        if (trimmed.starts_with(":::toggle") || trimmed.starts_with(":::details")) && depth < MAX_QUOTE_DEPTH {
            flush_para(&mut out, &mut para);
            let prefix_len = if trimmed.starts_with(":::toggle") { 9 } else { 10 };
            let title = trimmed[prefix_len..].trim();
            let summary = if title.is_empty() { "상세 정보" } else { title };
            i += 1;
            let mut block_lines: Vec<&str> = Vec::new();
            while i < lines.len() && lines[i].trim_start() != ":::" {
                block_lines.push(lines[i]);
                i += 1;
            }
            if i < lines.len() {
                i += 1; // 닫는 ::: 소비
            }
            let inner_html = render(&block_lines.join("\n"), depth + 1, slug_counts);
            out.push_str("<details class=\"notion-toggle\">\n");
            out.push_str(&format!("<summary>{}</summary>\n", inline(summary)));
            out.push_str("<div class=\"toggle-content\">\n");
            out.push_str(&inner_html);
            out.push_str("</div>\n</details>\n");
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

            let col_count = aligns.len();
            while i < lines.len() && lines[i].contains('|') && !lines[i].trim().is_empty() {
                let row_trim = lines[i].trim();
                // 테이블 안이 아닌 새 블록이 시작되면 테이블 종료
                if is_block_start(row_trim) && !row_trim.starts_with('|') {
                    break;
                }
                if is_hr(row_trim) {
                    break;
                }
                let cells = split_table_row(lines[i]);
                out.push_str("<tr>");
                for idx in 0..col_count {
                    let align = aligns.get(idx).copied().unwrap_or(Align::None);
                    let cell_content = cells.get(idx).map(|s| s.as_str()).unwrap_or("");
                    out.push_str(&format!("<td{}>{}</td>", align_attr(align), inline(cell_content.trim())));
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
            let slug = unique_slug(&slugify(text), slug_counts);
            out.push_str(&format!("<h{0} id=\"{2}\">{1}</h{0}>\n", level, inline(text), escape_html(&slug)));
            i += 1;
            continue;
        }

        // 인용 > 또는 콜아웃 (노션/GitHub Alert 스타일)
        if trimmed.starts_with('>') && depth < MAX_QUOTE_DEPTH {
            flush_para(&mut out, &mut para);
            let mut inner: Vec<&str> = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with('>') {
                let rest = &lines[i].trim_start()[1..];
                inner.push(rest.strip_prefix(' ').unwrap_or(rest));
                i += 1;
            }
            if let Some(first) = inner.first() {
                if let Some((kind, title)) = parse_callout_header(first) {
                    let body_lines = &inner[1..];
                    let body_rendered = render(&body_lines.join("\n"), depth + 1, slug_counts);
                    let (icon, default_title, class_name) = callout_meta(&kind);
                    let display_title = if title.is_empty() { default_title } else { title.as_str() };
                    out.push_str(&format!(
                        "<div class=\"callout {}\">\n<div class=\"callout-header\"><span class=\"callout-icon\">{}</span><span class=\"callout-title\">{}</span></div>\n<div class=\"callout-body\">\n{}</div>\n</div>\n",
                        class_name,
                        icon,
                        inline(display_title),
                        body_rendered
                    ));
                    continue;
                } else if let Some((emoji, rest_text)) = parse_emoji_callout(first) {
                    let mut all_lines = vec![rest_text];
                    all_lines.extend_from_slice(&inner[1..]);
                    let body_rendered = render(&all_lines.join("\n"), depth + 1, slug_counts);
                    out.push_str(&format!(
                        "<div class=\"callout callout-custom\">\n<div class=\"callout-header\"><span class=\"callout-icon\">{}</span></div>\n<div class=\"callout-body\">\n{}</div>\n</div>\n",
                        emoji,
                        body_rendered
                    ));
                    continue;
                }
            }
            out.push_str("<blockquote>\n");
            out.push_str(&render(&inner.join("\n"), depth + 1, slug_counts));
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
    t.starts_with("```")
        || t.starts_with("$$")
        || t.starts_with(":::toggle")
        || t.starts_with(":::details")
        || t.starts_with('>')
        || is_hr(t)
        || heading_level(t).is_some()
        || list_item(t).is_some()
}

fn heading_level(line: &str) -> Option<usize> {
    let count = line.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&count) && line[count..].starts_with(' ') {
        Some(count)
    } else {
        None
    }
}

// 코드 언어 라벨용: 첫 글자만 대문자로 (문장형 표시)
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn slugify(text: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = true;
    for c in text.chars() {
        if c.is_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if (c == ' ' || c == '-' || c == '_') && !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "section".to_string()
    } else {
        slug
    }
}

// 중복 제목에는 -1, -2 접미사를 붙여 id 충돌 방지 (문서 전체에서 고유 보장)
fn unique_slug(base: &str, counts: &mut HashMap<String, usize>) -> String {
    let n = counts.entry(base.to_string()).or_insert(0);
    if *n == 0 {
        *n = 1;
        base.to_string()
    } else {
        let slug = format!("{base}-{n}");
        *n += 1;
        slug
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

// 콜아웃(GitHub Alert & Notion) 헤더 파싱: [!NOTE], [!TIP] 등
fn parse_callout_header(first_line: &str) -> Option<(String, String)> {
    let t = first_line.trim();
    if t.starts_with("[!") && t.contains(']') {
        let end = t.find(']')?;
        let kind = t[2..end].trim().to_ascii_lowercase();
        if matches!(
            kind.as_str(),
            "note" | "tip" | "important" | "warning" | "caution" | "info" | "todo" | "success" | "danger" | "quote"
        ) {
            let custom_title = t[end + 1..].trim();
            return Some((kind, custom_title.to_string()));
        }
    }
    None
}

// 이모지로 시작하는 노션 감성 콜아웃: > 💡 팁 내용
fn parse_emoji_callout(first_line: &str) -> Option<(&'static str, &str)> {
    let t = first_line.trim();
    const CALLOUT_EMOJIS: &[&str] = &[
        "💡", "📌", "⚠️", "🚨", "ℹ️", "📝", "✨", "🔥", "🎯", "❓", "✅", "❌", "💬", "🔍", "⚡",
    ];
    for &em in CALLOUT_EMOJIS {
        if let Some(rest) = t.strip_prefix(em) {
            if rest.starts_with(' ') || rest.is_empty() {
                return Some((em, rest.trim_start()));
            }
        }
    }
    None
}

fn callout_meta(kind: &str) -> (&'static str, &'static str, &'static str) {
    match kind {
        "note" | "info" => (
            r#"<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="10"/><path d="M12 16v-4"/><path d="M12 8h.01"/></svg>"#,
            "Note",
            "callout-note",
        ),
        "tip" => (
            r#"<svg viewBox="0 0 24 24"><path d="M15 14c.2-1 .7-1.7 1.5-2.5 1-.9 1.5-2.2 1.5-3.5A6 6 0 0 0 6 8c0 1 .2 2.2 1.5 3.5.7.7 1.3 1.5 1.5 2.5"/><path d="M9 18h6"/><path d="M10 22h4"/></svg>"#,
            "Tip",
            "callout-tip",
        ),
        "important" => (
            r#"<svg viewBox="0 0 24 24"><path d="m19 21-7-4-7 4V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v16z"/></svg>"#,
            "Important",
            "callout-important",
        ),
        "warning" => (
            r#"<svg viewBox="0 0 24 24"><path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>"#,
            "Warning",
            "callout-warning",
        ),
        "caution" | "danger" => (
            r#"<svg viewBox="0 0 24 24"><polygon points="7.86 2 16.14 2 22 7.86 22 16.14 16.14 22 7.86 22 2 16.14 2 7.86 7.86 2"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>"#,
            "Caution",
            "callout-caution",
        ),
        "todo" => (
            r#"<svg viewBox="0 0 24 24"><path d="m9 11 3 3L22 4"/><path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11"/></svg>"#,
            "To-Do",
            "callout-todo",
        ),
        "success" => (
            r#"<svg viewBox="0 0 24 24"><path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/></svg>"#,
            "Success",
            "callout-success",
        ),
        _ => (
            r#"<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="10"/><path d="M12 16v-4"/><path d="M12 8h.01"/></svg>"#,
            "Note",
            "callout-note",
        ),
    }
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
// 처리 순서: 코드 스팬 -> 인라인 수식 -> 각주 참조 -> 백슬래시 이스케이프 -> HTML 이스케이프 -> 링크/이미지 -> 강조/형광펜/키캡/윗첨자
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

    // 2) 인라인 수식: $formula$
    let mut s_math = String::with_capacity(s.len());
    let mut rest_math = s.as_str();
    while let Some(a) = rest_math.find('$') {
        let after = &rest_math[a + 1..];
        if after.starts_with(|c: char| c.is_whitespace() || c == '$') {
            s_math.push_str(&rest_math[..=a]);
            rest_math = after;
            continue;
        }
        let mut close_idx = None;
        let mut search_from = 0;
        while let Some(pos) = after[search_from..].find('$') {
            let actual = search_from + pos;
            if actual > 0 && !after[..actual].ends_with(char::is_whitespace) {
                close_idx = Some(actual);
                break;
            }
            search_from = actual + 1;
        }
        if let Some(len) = close_idx {
            let formula = &after[..len];
            if !formula.contains('\n') {
                s_math.push_str(&rest_math[..a]);
                let tag = format!("<span class=\"math-inline\">${}$</span>", escape_html(formula));
                s_math.push_str(&hold(&mut stash, tag));
                rest_math = &after[len + 1..];
                continue;
            }
        }
        s_math.push_str(&rest_math[..=a]);
        rest_math = after;
    }
    s_math.push_str(rest_math);
    let s = s_math;

    // 3) 각주 참조: [^id]
    let mut s_fn = String::with_capacity(s.len());
    let mut rest_fn = s.as_str();
    while let Some(a) = rest_fn.find("[^") {
        if let Some(b) = rest_fn[a + 2..].find(']') {
            let id = &rest_fn[a + 2..a + 2 + b];
            if !id.is_empty() && !id.contains('\n') && !id.contains(' ') {
                s_fn.push_str(&rest_fn[..a]);
                let id_esc = escape_html(id);
                let tag = format!(
                    "<sup class=\"footnote-ref\"><a href=\"#fn-{0}\" id=\"fnref-{0}\">[{0}]</a></sup>",
                    id_esc
                );
                s_fn.push_str(&hold(&mut stash, tag));
                rest_fn = &rest_fn[a + 2 + b + 1..];
                continue;
            }
        }
        s_fn.push_str(&rest_fn[..a + 2]);
        rest_fn = &rest_fn[a + 2..];
    }
    s_fn.push_str(rest_fn);
    let s = s_fn;

    // 4) 백슬래시 이스케이프: \* \[ \\ 등
    let mut esc = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, chars.peek().copied()) {
            ('\\', Some(n)) if "\\*_[]()#+-.!|>~=$^".contains(n) => {
                esc.push_str(&hold(&mut stash, escape_html(&n.to_string())));
                chars.next();
            }
            _ => esc.push(c),
        }
    }

    // 5) 나머지 텍스트 HTML 이스케이프 후 링크/이미지, 서식
    let s = replace_links(&escape_html(&esc), &mut stash);
    let s = replace_delim(&s, "***", "<strong><em>", "</em></strong>");
    let s = replace_delim(&s, "**", "<strong>", "</strong>");
    let s = replace_delim(&s, "*", "<em>", "</em>");
    let s = replace_delim(&s, "~~", "<del>", "</del>");
    let s = replace_delim(&s, "==", "<mark>", "</mark>");
    let s = replace_delim(&s, "^", "<sup>", "</sup>");
    let s = replace_kbd(&s);

    restore(&s, &stash)
}

// 키보드 키캡 서식: [[Ctrl]] 또는 <kbd>Ctrl</kbd>
fn replace_kbd(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find("[[") {
        if let Some(end) = rest[start + 2..].find("]]") {
            let key = &rest[start + 2..start + 2 + end];
            if !key.is_empty() && !key.contains('\n') {
                out.push_str(&rest[..start]);
                out.push_str("<kbd>");
                out.push_str(key);
                out.push_str("</kbd>");
                rest = &rest[start + 2 + end + 2..];
                continue;
            }
        }
        out.push_str(&rest[..start + 2]);
        rest = &rest[start + 2..];
    }
    out.push_str(rest);
    out.replace("&lt;kbd&gt;", "<kbd>").replace("&lt;/kbd&gt;", "</kbd>")
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

fn is_relative_image_path(url: &str) -> bool {
    let u = url.trim();
    if u.is_empty() || u.contains(':') || u.contains('\\') || u.starts_with("//") {
        return false;
    }
    if u.contains("..") {
        return false;
    }
    let clean = u.split('?').next().unwrap_or("").split('#').next().unwrap_or("");
    let ext = clean.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "bmp" | "ico" | "avif"
    )
}

fn safe_image(url: &str) -> bool {
    let u = url.trim().to_ascii_lowercase();
    if u.starts_with("http://") || u.starts_with("https://") {
        return true;
    }
    // data: URL은 래스터 이미지만 허용 (svg+xml 등은 스크립트 표면이 있어 차단)
    if let Some(mime) = u.strip_prefix("data:image/") {
        let mime = mime.split(|c: char| c == ';' || c == ',' || c.is_whitespace()).next().unwrap_or("");
        return matches!(mime, "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "avif");
    }
    is_relative_image_path(url)
}

pub fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

// [text](url) -> <a>text</a>, ![alt](url) -> <img>. 입력은 이미 HTML 이스케이프된 상태.
// 상대 이미지 경로는 로컬 서버의 /api/raw?file=... 엔드포인트로 자동 매핑
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
                            let src = if is_relative_image_path(&url) {
                                let clean = url.trim().trim_start_matches("./");
                                format!("/api/raw?file={}", url_encode(clean))
                            } else {
                                url.trim().to_string()
                            };
                            let tag = format!("<img src=\"{}\" alt=\"{}\">", src, text);
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
    fn table_stops_on_heading_or_quote_with_pipe() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |\n# Heading | with pipe\n";
        let html = to_html(md);
        assert!(html.contains("<table>"), "{html}");
        assert!(html.contains("<h1 id=\"heading-with-pipe\">Heading | with pipe</h1>"), "{html}");
        assert_eq!(html.matches("<tr>").count(), 2, "헤더 1행 + 본문 1행이어야 함: {html}");
    }

    #[test]
    fn table_pads_missing_columns() {
        let md = "| A | B | C |\n|---|---|---|\n| 1 |\n";
        let html = to_html(md);
        assert!(html.contains("<tr><td>1</td><td></td><td></td></tr>"), "{html}");
    }

    #[test]
    fn unmatched_delimiters_stay_literal() {
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
    fn local_relative_images_mapped_to_api_raw() {
        let md = "![다이어그램](diagram.png) 및 ![아키텍처](images/arch.svg)";
        let html = to_html(md);
        assert!(html.contains("<img src=\"/api/raw?file=diagram.png\" alt=\"다이어그램\">"), "{html}");
        assert!(html.contains("<img src=\"/api/raw?file=images/arch.svg\" alt=\"아키텍처\">"), "{html}");
        // Path traversal 시도 차단
        assert!(!to_html("![탈출](../secret.png)").contains("<img src=\"/api/raw"));
    }

    #[test]
    fn mermaid_diagram_rendering() {
        let md = "```mermaid\ngraph TD\n  A --> B\n```";
        let html = to_html(md);
        assert!(html.contains("class=\"code-block-wrapper mermaid-wrapper\""), "{html}");
        assert!(html.contains("<div class=\"mermaid\">\ngraph TD\n  A --&gt; B\n</div>"), "{html}");
    }

    #[test]
    fn footnotes_work() {
        let md = "문장입니다[^1] 그리고 다음[^note].\n\n[^1]: 첫 번째 각주\n[^note]: 두 번째 상세 설명";
        let html = to_html(md);
        assert!(html.contains("<sup class=\"footnote-ref\"><a href=\"#fn-1\" id=\"fnref-1\">[1]</a></sup>"), "{html}");
        assert!(html.contains("<li id=\"fn-1\">첫 번째 각주 <a href=\"#fnref-1\" class=\"footnote-backref\">↩</a></li>"), "{html}");
        assert!(html.contains("<li id=\"fn-note\">두 번째 상세 설명 <a href=\"#fnref-note\" class=\"footnote-backref\">↩</a></li>"), "{html}");
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
        assert!(to_html("## T").contains("<h2 id=\"t\">T</h2>"));
        assert!(!to_html("#tag").contains("<h1>"));
    }

    #[test]
    fn duplicate_headings_get_unique_ids() {
        let html = to_html("# 같은 제목\n\n# 같은 제목\n\n## 같은 제목\n");
        assert!(html.contains("<h1 id=\"같은-제목\">같은 제목</h1>"), "{html}");
        assert!(html.contains("<h1 id=\"같은-제목-1\">같은 제목</h1>"), "{html}");
        assert!(html.contains("<h2 id=\"같은-제목-2\">같은 제목</h2>"), "{html}");
    }

    #[test]
    fn data_svg_images_are_blocked_but_raster_data_urls_stay() {
        assert!(!to_html("![a](data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=)").contains("<img"), "svg data URL 차단 실패");
        assert!(!to_html("![a](DATA:IMAGE/SVG+XML;hi)").contains("<img"), "대소문자 우회 차단 실패");
        assert!(to_html("![a](data:image/png;base64,iVBORw0KGgo=)").contains("<img src=\"data:image/png;base64,iVBORw0KGgo=\""), "png data URL 허용 실패");
    }

    #[test]
    fn code_block_is_escaped_and_unclosed_fence_ok() {
        assert!(to_html("```\n<b>*x*</b>\n```").contains("<pre><code>&lt;b&gt;*x*&lt;/b&gt;\n</code></pre>"));
        let _ = to_html("```\nnever closed");
    }

    #[test]
    fn strikethrough_and_superscript() {
        assert!(to_html("~~취소선~~").contains("<del>취소선</del>"));
        assert!(to_html("~~a **b** c~~").contains("<del>a <strong>b</strong> c</del>"));
        assert_eq!(to_html("~not del~"), "<p>~not del~</p>\n");
        assert_eq!(to_html("~~unmatched"), "<p>~~unmatched</p>\n");
        assert!(to_html("x^2^ + y^2^").contains("x<sup>2</sup> + y<sup>2</sup>"));
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

    #[test]
    fn callouts_render() {
        let md = "> [!NOTE]\n> 이것은 중요 메모입니다.\n\n> [!TIP] 꿀팁\n> 유용한 정보\n\n> 💡 이모지 콜아웃";
        let html = to_html(md);
        assert!(html.contains("class=\"callout callout-note\""), "{html}");
        assert!(html.contains("class=\"callout-title\">Note<"), "{html}");
        assert!(html.contains("<svg viewBox=\"0 0 24 24\">"), "{html}");
        assert!(html.contains("class=\"callout callout-tip\""), "{html}");
        assert!(html.contains("class=\"callout-title\">꿀팁<"), "{html}");
        assert!(html.contains("class=\"callout callout-custom\""), "{html}");
        assert!(html.contains("💡"), "{html}");
    }

    #[test]
    fn toggle_and_math_render() {
        let md = ":::toggle 상세 정보 펼치기\n내용입니다.\n:::\n\n$$E = mc^2$$\n\n인라인 $a^2 + b^2 = c^2$ 수식";
        let html = to_html(md);
        assert!(html.contains("<details class=\"notion-toggle\">"), "{html}");
        assert!(html.contains("<summary>상세 정보 펼치기</summary>"), "{html}");
        assert!(html.contains("<div class=\"math-block\">$$E = mc^2$$</div>"), "{html}");
        assert!(html.contains("<span class=\"math-inline\">$a^2 + b^2 = c^2$</span>"), "{html}");
    }

    #[test]
    fn highlight_and_kbd_render() {
        let md = "여기는 ==형광펜== 텍스트이고 [[Ctrl]] + [[C]] 키입니다.";
        let html = to_html(md);
        assert!(html.contains("<mark>형광펜</mark>"), "{html}");
        assert!(html.contains("<kbd>Ctrl</kbd>"), "{html}");
        assert!(html.contains("<kbd>C</kbd>"), "{html}");
    }

    #[test]
    fn edge_cases_and_unclosed_blocks() {
        let unclosed_math = "$$\n\\alpha + \\beta";
        let html_math = to_html(unclosed_math);
        assert!(html_math.contains("<div class=\"math-block\">"), "{html_math}");

        let unclosed_toggle = ":::toggle 닫히지 않은 토글\n내용";
        let html_toggle = to_html(unclosed_toggle);
        assert!(html_toggle.contains("<details class=\"notion-toggle\">"), "{html_toggle}");
        assert!(html_toggle.contains("</details>"), "{html_toggle}");

        let currency = "가격은 $100 이고 다른 것은 $200 입니다.";
        let html_curr = to_html(currency);
        assert!(!html_curr.contains("class=\"math-inline\""), "{html_curr}");

        let esc_dollar = "\\$50";
        assert!(to_html(esc_dollar).contains("$50"));

        assert_eq!(to_html("==시작만"), "<p>==시작만</p>\n");
        assert_eq!(to_html("===="), "<p>====</p>\n");

        let empty_callout = "> [!NOTE]";
        let html_callout = to_html(empty_callout);
        assert!(html_callout.contains("class=\"callout callout-note\""), "{html_callout}");

        let mut deep_toggle = String::new();
        for _ in 0..100 {
            deep_toggle.push_str(":::toggle 깊은 토글\n");
        }
        let _ = to_html(&deep_toggle);
    }

    #[test]
    fn code_language_labels_use_sentence_case() {
        let rust = to_html("```rust\nlet x = 1;\n```");
        assert!(rust.contains("<span class=\"code-lang\">Rust</span>"), "{rust}");
        let mermaid = to_html("```mermaid\ngraph TD\n  A --> B\n```");
        assert!(mermaid.contains("<span class=\"code-lang\">Mermaid diagram</span>"), "{mermaid}");
        let plain = to_html("```\ntext\n```");
        assert!(plain.contains("<span class=\"code-lang\">Text</span>"), "{plain}");
    }
}
