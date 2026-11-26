// 네이티브 인쇄: 마크다운 → 페이지별 그리기 명령(배치) → Windows 인쇄 대화상자 + GDI.
// 배치는 측정기(Measure)만 바꾸면 프린터 없이 테스트할 수 있다. 단위는 pt(1/72인치).
// Mermaid·수식은 보기 화면과 같은 렌더러로 그림을 떠서 인쇄한다(Offscreen). 줄 안 수식은 글줄 기준선에 맞춘 그림.
use std::rc::Rc;

use eframe::egui;

use pulldown_cmark::{
    Alignment, BlockQuoteKind, CodeBlockKind, Event, Options, Parser, Tag, TagEnd,
};

use crate::textops::{escape_plus_lines, inline_spans, preprocess, Inline};

const BODY: f32 = 10.5;
const LH: f32 = 1.55; // 줄 높이 배수
const TEXT: u32 = 0x0F172A;
const MUTED: u32 = 0x64748B;
const LINK: u32 = 0x2563EB;
const MARK_BG: u32 = 0xFDE68A;
const CODE_BG: u32 = 0xF1F5F9;
const BORDER: u32 = 0xCBD5E1;
const HEAD_BG: u32 = 0xF8FAFC;
const LIST_W: f32 = 20.0; // 목록 기호 칸
const QUOTE_W: f32 = 14.0;

#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Style {
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub mono: bool,
    pub underline: bool,
    pub strike: bool,
    pub color: u32, // 0xRRGGBB
    pub bg: Option<u32>,
    pub img: Option<InlineImg>, // 줄 안 그림(인라인 수식). 글자 대신 "\u{FFFC}" 한 글자로 자리를 잡는다
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct InlineImg {
    idx: usize, // Layout::inline_imgs
    w: f32,
    asc: f32,
    desc: f32,
}

impl Style {
    fn body() -> Self {
        Self {
            size: BODY,
            bold: false,
            italic: false,
            mono: false,
            underline: false,
            strike: false,
            color: TEXT,
            bg: None,
            img: None,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Op {
    Text {
        x: f32,
        y: f32, // 기준선
        text: String,
        style: Style,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        fill: u32,
    },
    Line {
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        color: u32,
        width: f32,
    },
    Image {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        img: Rc<image::RgbaImage>,
    },
}

pub(crate) trait Measure {
    fn width(&self, text: &str, style: &Style) -> f32;
}

#[derive(Default, Clone)]
struct Piece {
    x: f32,
    w: f32,
    text: String,
    style: Option<Style>,
}

#[derive(Default, Clone)]
struct Line {
    pieces: Vec<Piece>,
    w: f32,
    size: f32, // 줄에서 가장 큰 글자 크기
    asc: f32,  // 줄 안 그림의 기준선 위·아래 (없으면 0)
    desc: f32,
}

impl Line {
    fn h(&self) -> f32 {
        if self.asc + self.desc > 0.0 {
            (self.size * LH).max(
                self.asc.max(self.size * 0.8) + self.desc.max(self.size * 0.3) + self.size * 0.3,
            )
        } else {
            self.size * LH
        }
    }

    fn baseline(&self) -> f32 {
        let h = self.h();
        if self.asc + self.desc > 0.0 {
            let (a, d) = (
                self.asc.max(self.size * 0.8),
                self.desc.max(self.size * 0.3),
            );
            (h - a - d) / 2.0 + a
        } else {
            h / 2.0 + self.size * 0.35
        }
    }
}

fn run_width(m: &dyn Measure, text: &str, st: &Style) -> f32 {
    st.img.map_or_else(|| m.width(text, st), |i| i.w)
}

type Runs = Vec<(String, Style)>;

// 공백 뒤에서 끊을 수 있는 토큰으로 나눈다 (공백은 앞 토큰에 붙는다)
fn tokens(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut prev_space = false;
    for (i, c) in s.char_indices() {
        let space = c == ' ' || c == '\t';
        if prev_space && !space {
            out.push(&s[start..i]);
            start = i;
        }
        prev_space = space;
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

fn break_lines(m: &dyn Measure, runs: &[(String, Style)], width: f32) -> Vec<Line> {
    fn add(m: &dyn Measure, cur: &mut Line, text: &str, st: &Style, w: f32) {
        cur.size = cur.size.max(st.size);
        if let Some(i) = st.img {
            cur.asc = cur.asc.max(i.asc);
            cur.desc = cur.desc.max(i.desc);
        }
        if let Some(last) = cur.pieces.last_mut() {
            if last.style.as_ref() == Some(st) {
                last.text.push_str(text);
                last.w = run_width(m, &last.text, st);
                cur.w = last.x + last.w;
                return;
            }
        }
        cur.pieces.push(Piece {
            x: cur.w,
            w,
            text: text.to_string(),
            style: Some(st.clone()),
        });
        cur.w += w;
    }
    fn push(m: &dyn Measure, lines: &mut Vec<Line>, cur: &mut Line, size: f32) {
        let mut l = std::mem::take(cur);
        if let Some(last) = l.pieces.last_mut() {
            let t = last.text.trim_end().len();
            if t < last.text.len() && last.style.as_ref().is_some_and(|s| s.img.is_none()) {
                last.text.truncate(t);
                last.w = m.width(&last.text, last.style.as_ref().unwrap());
                l.w = last.x + last.w;
            }
        }
        if l.size == 0.0 {
            l.size = size;
        }
        lines.push(l);
    }
    let mut lines = Vec::new();
    let mut cur = Line::default();
    let mut last_size = BODY;
    for (text, st) in runs {
        last_size = st.size;
        for (i, seg) in text.split('\n').enumerate() {
            if i > 0 {
                push(m, &mut lines, &mut cur, st.size);
            }
            for tok in tokens(seg) {
                let tw = run_width(m, tok, st);
                if cur.w > 0.0 && cur.w + tw > width && !tok.trim().is_empty() {
                    push(m, &mut lines, &mut cur, st.size);
                }
                if tw <= width || st.img.is_some() {
                    add(m, &mut cur, tok, st, tw);
                    continue;
                }
                // 한 토큰이 줄보다 길면 글자 단위로 자른다
                for (ci, ch) in tok.char_indices() {
                    let s = &tok[ci..ci + ch.len_utf8()];
                    let cw = m.width(s, st);
                    if cur.w > 0.0 && cur.w + cw > width {
                        push(m, &mut lines, &mut cur, st.size);
                    }
                    add(m, &mut cur, s, st, cw);
                }
            }
        }
    }
    if !cur.pieces.is_empty() || lines.is_empty() {
        push(m, &mut lines, &mut cur, last_size);
    }
    lines
}

enum Marker {
    Text(String),
    Check(bool),
}

struct Table {
    aligns: Vec<Alignment>,
    rows: Vec<(bool, Vec<Runs>)>,
}

pub(crate) struct Layout<'a> {
    m: &'a dyn Measure,
    load: &'a dyn Fn(&str) -> Option<image::RgbaImage>,
    w: f32,
    h: f32,
    pub pages: Vec<Vec<Op>>,
    y: f32,
    x: f32,
    runs: Runs,
    bold: u16,
    italic: u16,
    strike: u16,
    mark: u16,
    link: u16,
    heading: Option<usize>,
    lists: Vec<Option<u64>>,
    marker: Option<Marker>,
    quotes: Vec<(usize, f32, f32)>,
    code: Option<String>,
    table: Option<Table>,
    in_image: u16,
    prefix: Option<(String, Style)>,
    mermaid: bool,
    off: Option<Offscreen>, // 다이어그램·수식 그림용 화면 밖 egui (처음 쓸 때 만든다)
    inline_imgs: Vec<Rc<image::RgbaImage>>,
}

impl<'a> Layout<'a> {
    fn offscreen(&mut self) -> &mut Offscreen {
        self.off.get_or_insert_with(Offscreen::new)
    }

    fn page(&mut self) -> &mut Vec<Op> {
        self.pages.last_mut().unwrap()
    }

    fn ensure(&mut self, need: f32) {
        if self.y + need > self.h && self.y > 0.0 {
            self.pages.push(Vec::new());
            self.y = 0.0;
        }
    }

    fn gap(&mut self, g: f32) {
        if self.y > 0.0 {
            self.y += g;
        }
    }

    fn style(&self) -> Style {
        let mut s = Style::body();
        if let Some(l) = self.heading {
            s.size = [20.0, 16.0, 13.5, 12.0, 11.0, 10.5][l.clamp(1, 6) - 1];
            s.bold = true;
        }
        s.bold |= self.bold > 0;
        s.italic = self.italic > 0;
        s.strike = self.strike > 0;
        if self.mark > 0 {
            s.bg = Some(MARK_BG);
        }
        if self.link > 0 {
            s.color = LINK;
            s.underline = true;
        }
        s
    }

    fn line_ops(&mut self, line: &Line, x: f32) {
        let h = line.h();
        let base = self.y + line.baseline();
        let y = self.y;
        for p in &line.pieces {
            let st = p.style.clone().unwrap();
            if let Some(i) = st.img {
                let img = Rc::clone(&self.inline_imgs[i.idx]);
                self.page().push(Op::Image {
                    x: x + p.x,
                    y: base - i.asc,
                    w: i.w,
                    h: i.asc + i.desc,
                    img,
                });
                continue;
            }
            if let Some(bg) = st.bg {
                let bh = st.size * 1.3;
                self.page().push(Op::Rect {
                    x: x + p.x,
                    y: y + (h - bh) / 2.0,
                    w: p.w,
                    h: bh,
                    fill: bg,
                });
            }
            self.page().push(Op::Text {
                x: x + p.x,
                y: base,
                text: p.text.clone(),
                style: st,
            });
        }
    }

    fn draw_marker(&mut self, line_h: f32, size: f32) {
        let Some(mk) = self.marker.take() else {
            return;
        };
        self.ensure(line_h);
        let x = self.x - LIST_W;
        let y = self.y;
        match mk {
            Marker::Text(label) => {
                let mut st = Style::body();
                st.color = MUTED;
                let base = y + line_h / 2.0 + size * 0.35;
                self.page().push(Op::Text {
                    x,
                    y: base,
                    text: label,
                    style: st,
                });
            }
            Marker::Check(done) => {
                let s = BODY * 0.9;
                let (bx, by) = (x, y + (line_h - s) / 2.0);
                let c = if done { LINK } else { MUTED };
                let page = self.page();
                for (x0, y0, x1, y1) in [
                    (bx, by, bx + s, by),
                    (bx + s, by, bx + s, by + s),
                    (bx + s, by + s, bx, by + s),
                    (bx, by + s, bx, by),
                ] {
                    page.push(Op::Line {
                        x0,
                        y0,
                        x1,
                        y1,
                        color: c,
                        width: 0.8,
                    });
                }
                if done {
                    for (x0, y0, x1, y1) in [
                        (bx + s * 0.2, by + s * 0.5, bx + s * 0.42, by + s * 0.75),
                        (bx + s * 0.42, by + s * 0.75, bx + s * 0.82, by + s * 0.25),
                    ] {
                        page.push(Op::Line {
                            x0,
                            y0,
                            x1,
                            y1,
                            color: c,
                            width: 1.2,
                        });
                    }
                }
            }
        }
    }

    // 모아 둔 인라인 글을 현재 들여쓰기로 흘려 넣는다
    fn flush(&mut self, gap_after: f32) {
        if self.table.is_some() {
            return;
        }
        if self.runs.is_empty() {
            return;
        }
        let mut runs = std::mem::take(&mut self.runs);
        if let Some(p) = self.prefix.take() {
            runs.insert(0, p); // 각주 번호를 첫 문단 앞에 붙인다
        }
        let lines = break_lines(self.m, &runs, self.w - self.x);
        if let Some(first) = lines.first() {
            self.ensure(first.h());
            self.draw_marker(first.h(), first.size);
        }
        let x = self.x;
        for l in &lines {
            self.ensure(l.h());
            self.line_ops(l, x);
            self.y += l.h();
        }
        self.y += gap_after;
    }

    fn code_block(&mut self, code: &str) {
        let mut st = Style::body();
        st.mono = true;
        st.size = BODY * 0.88;
        let pad = 6.0;
        let (x, w) = (self.x, self.w - self.x);
        let runs = vec![(code.trim_end_matches('\n').to_string(), st)];
        let lines = break_lines(self.m, &runs, w - 2.0 * pad);
        self.gap(2.0);
        self.draw_marker(lines[0].h(), lines[0].size);
        let n = lines.len();
        for (i, l) in lines.iter().enumerate() {
            let top = if i == 0 { pad } else { 0.0 };
            let bot = if i + 1 == n { pad } else { 0.0 };
            self.ensure(top + l.h() + bot);
            let y = self.y;
            self.page().push(Op::Rect {
                x,
                y,
                w,
                h: top + l.h() + bot,
                fill: CODE_BG,
            });
            self.y += top;
            self.line_ops(l, x + pad);
            self.y += l.h() + bot;
        }
        self.y += 6.0;
    }

    fn image(&mut self, url: &str) {
        if let Some(img) = (self.load)(url) {
            self.place(img, 0.75, false); // 96dpi 픽셀 → pt
        }
    }

    fn place(&mut self, img: image::RgbaImage, pt_per_px: f32, center: bool) {
        let avail = self.w - self.x;
        let (pw, ph) = (
            img.width() as f32 * pt_per_px,
            img.height() as f32 * pt_per_px,
        );
        let s = (avail / pw).min(self.h / ph).min(1.0);
        let (w, h) = (pw * s, ph * s);
        self.ensure(h);
        let x = if center {
            self.x + (avail - w) / 2.0
        } else {
            self.x
        };
        let y = self.y;
        self.page().push(Op::Image {
            x,
            y,
            w,
            h,
            img: Rc::new(img),
        });
        self.y += h + 4.0;
    }

    fn table(&mut self, t: Table) {
        let ncol = t.rows.iter().map(|r| r.1.len()).max().unwrap_or(0);
        if ncol == 0 {
            return;
        }
        let pad = 5.0;
        let avail = self.w - self.x;
        let mut nat = vec![30.0f32; ncol];
        for (head, cells) in &t.rows {
            for (i, c) in cells.iter().enumerate() {
                let w: f32 = c
                    .iter()
                    .map(|(s, st)| {
                        let mut st = st.clone();
                        st.bold |= *head;
                        run_width(self.m, s, &st)
                    })
                    .sum();
                nat[i] = nat[i].max(w + 2.0 * pad);
            }
        }
        let total: f32 = nat.iter().sum();
        let ws: Vec<f32> = nat.iter().map(|n| avail * n / total).collect();
        self.gap(4.0);
        for (head, cells) in &t.rows {
            let laid: Vec<Vec<Line>> = (0..ncol)
                .map(|i| {
                    let mut runs = cells.get(i).cloned().unwrap_or_default();
                    for r in &mut runs {
                        r.1.bold |= *head;
                    }
                    break_lines(self.m, &runs, ws[i] - 2.0 * pad)
                })
                .collect();
            let row_h = laid
                .iter()
                .map(|ls| ls.iter().map(Line::h).sum::<f32>())
                .fold(0.0, f32::max)
                + 2.0 * pad;
            self.ensure(row_h);
            let y0 = self.y;
            let mut x = self.x;
            if *head {
                self.page().push(Op::Rect {
                    x,
                    y: y0,
                    w: avail,
                    h: row_h,
                    fill: HEAD_BG,
                });
            }
            for (i, ls) in laid.iter().enumerate() {
                self.y = y0 + pad;
                for l in ls {
                    let off = match t.aligns.get(i) {
                        Some(Alignment::Center) => (ws[i] - 2.0 * pad - l.w) / 2.0,
                        Some(Alignment::Right) => ws[i] - 2.0 * pad - l.w,
                        _ => 0.0,
                    };
                    self.line_ops(l, x + pad + off.max(0.0));
                    self.y += l.h();
                }
                x += ws[i];
            }
            // 테두리
            let (l, r, b) = (self.x, self.x + avail, y0 + row_h);
            let mut segs = vec![(l, y0, r, y0), (l, b, r, b)];
            let mut cx = l;
            segs.push((cx, y0, cx, b));
            for w in &ws {
                cx += w;
                segs.push((cx, y0, cx, b));
            }
            for (x0, y0, x1, y1) in segs {
                self.page().push(Op::Line {
                    x0,
                    y0,
                    x1,
                    y1,
                    color: BORDER,
                    width: 0.6,
                });
            }
            self.y = b;
        }
        self.y += 8.0;
    }

    fn rule(&mut self, x0: f32, color: u32, width: f32) {
        let (y, x1) = (self.y, self.w);
        self.page().push(Op::Line {
            x0,
            y0: y,
            x1,
            y1: y,
            color,
            width,
        });
    }

    fn event(&mut self, ev: Event<'_>) {
        match ev {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) => {
                if self.in_image > 0 {
                } else if let Some(c) = &mut self.code {
                    c.push_str(&t);
                } else {
                    self.runs.push((t.into_string(), self.style()));
                }
            }
            Event::InlineMath(t) => {
                let size = self.style().size;
                match self.offscreen().math(&t, size, false) {
                    Some((img, asc)) => {
                        let (w, h) = (
                            img.width() as f32 / DIAGRAM_PX,
                            img.height() as f32 / DIAGRAM_PX,
                        );
                        let idx = self.inline_imgs.len();
                        self.inline_imgs.push(Rc::new(img));
                        let mut st = self.style();
                        st.img = Some(InlineImg {
                            idx,
                            w,
                            asc,
                            desc: h - asc,
                        });
                        self.runs.push(("\u{FFFC}".into(), st));
                    }
                    None => self.runs.push((t.into_string(), self.style())),
                }
            }
            Event::Code(t) => {
                let mut st = self.style();
                st.mono = true;
                st.size *= 0.9;
                st.bg.get_or_insert(CODE_BG);
                self.runs.push((t.into_string(), st));
            }
            Event::DisplayMath(t) => {
                self.flush(0.0);
                match self.offscreen().math(&t, BODY * 1.15, true) {
                    Some((img, _)) => {
                        self.gap(4.0);
                        self.place(img, 1.0 / DIAGRAM_PX, true);
                        self.y += 4.0;
                    }
                    None => self.code_block(t.trim()),
                }
            }
            Event::InlineHtml(t) if self.inline_tag(&t) => {}
            Event::Html(t) | Event::InlineHtml(t) => {
                if let Some(c) = &mut self.code {
                    c.push_str(&t);
                } else {
                    self.runs.push((t.into_string(), self.style()));
                }
            }
            Event::FootnoteReference(name) => {
                let mut st = self.style();
                st.size *= 0.8;
                st.color = LINK;
                self.runs.push((format!("[{name}]"), st));
            }
            Event::SoftBreak | Event::HardBreak => self.runs.push(("\n".into(), self.style())),
            Event::Rule => {
                self.flush(0.0);
                self.ensure(14.0);
                self.y += 7.0;
                self.rule(self.x, BORDER, 0.8);
                self.y += 7.0;
            }
            Event::TaskListMarker(done) => self.marker = Some(Marker::Check(done)),
        }
    }

    // inline_tags가 넣은 서식 태그
    fn inline_tag(&mut self, t: &str) -> bool {
        let (close, name) = match t.strip_prefix("</") {
            Some(n) => (true, n),
            None => (false, t.strip_prefix('<').unwrap_or("")),
        };
        let c = match name {
            "b>" => &mut self.bold,
            "i>" => &mut self.italic,
            "s>" => &mut self.strike,
            "mark>" => &mut self.mark,
            _ => return false,
        };
        *c = if close { c.saturating_sub(1) } else { *c + 1 };
        if name == "mark>" {
            self.bold = if close {
                self.bold.saturating_sub(1)
            } else {
                self.bold + 1
            };
        }
        true
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Emphasis => self.italic += 1,
            Tag::Strong => self.bold += 1,
            Tag::Strikethrough => self.strike += 1,
            Tag::Link { .. } => self.link += 1,
            Tag::Image { dest_url, .. } => {
                if self.in_image == 0 {
                    let gap = if self.runs.is_empty() { 0.0 } else { 2.0 };
                    self.flush(gap);
                    self.image(&dest_url);
                }
                self.in_image += 1;
            }
            Tag::Paragraph => self.flush(0.0),
            Tag::Heading { level, .. } => {
                self.flush(0.0);
                self.gap(8.0);
                self.heading = Some(level as usize);
            }
            Tag::CodeBlock(kind) => {
                self.flush(0.0);
                self.code = Some(String::new());
                self.mermaid = matches!(&kind, CodeBlockKind::Fenced(l) if l.trim().eq_ignore_ascii_case("mermaid"));
            }
            Tag::List(first) => {
                self.flush(0.0);
                self.lists.push(first);
                self.x += LIST_W;
            }
            Tag::Item => {
                self.flush(0.0);
                let depth = self.lists.len().saturating_sub(1);
                self.marker = Some(Marker::Text(match self.lists.last_mut() {
                    Some(Some(n)) => {
                        let s = crate::ui::num_label(*n, depth);
                        *n += 1;
                        s
                    }
                    _ => ["•", "◦", "▪"][depth % 3].to_string(),
                }));
            }
            Tag::BlockQuote(kind) => {
                self.flush(0.0);
                self.gap(2.0);
                self.quotes.push((self.pages.len() - 1, self.y, self.x));
                self.x += QUOTE_W;
                if let Some(k) = kind {
                    let (label, color) = match k {
                        BlockQuoteKind::Note => ("Note", 0x0284C7),
                        BlockQuoteKind::Tip => ("Tip", 0x059669),
                        BlockQuoteKind::Important => ("Important", 0x7C3AED),
                        BlockQuoteKind::Warning => ("Warning", 0xD97706),
                        BlockQuoteKind::Caution => ("Caution", 0xDC2626),
                    };
                    let mut st = Style::body();
                    st.bold = true;
                    st.color = color;
                    self.runs.push((label.into(), st));
                    self.flush(2.0);
                }
            }
            Tag::Table(aligns) => {
                self.flush(0.0);
                self.table = Some(Table {
                    aligns,
                    rows: Vec::new(),
                });
            }
            Tag::TableHead | Tag::TableRow => {
                let head = matches!(tag, Tag::TableHead);
                if let Some(t) = &mut self.table {
                    t.rows.push((head, Vec::new()));
                }
            }
            Tag::TableCell => self.runs.clear(),
            Tag::FootnoteDefinition(name) => {
                self.flush(0.0);
                let mut st = Style::body();
                st.color = LINK;
                self.prefix = Some((format!("[{name}] "), st));
            }
            _ => self.flush(0.0),
        }
    }

    fn end(&mut self, tag: TagEnd) {
        let in_list = !self.lists.is_empty();
        match tag {
            TagEnd::Emphasis => self.italic = self.italic.saturating_sub(1),
            TagEnd::Strong => self.bold = self.bold.saturating_sub(1),
            TagEnd::Strikethrough => self.strike = self.strike.saturating_sub(1),
            TagEnd::Link => self.link = self.link.saturating_sub(1),
            TagEnd::Image => self.in_image = self.in_image.saturating_sub(1),
            TagEnd::Paragraph => self.flush(if in_list { 2.0 } else { 7.0 }),
            TagEnd::Heading(level) => {
                self.flush(3.0);
                self.heading = None;
                if (level as usize) <= 2 {
                    self.rule(self.x, BORDER, 0.6);
                    self.y += 6.0;
                }
            }
            TagEnd::CodeBlock => {
                if let Some(c) = self.code.take() {
                    let avail = self.w - self.x;
                    match std::mem::take(&mut self.mermaid)
                        .then(|| self.offscreen().mermaid(&c, avail))
                        .flatten()
                    {
                        Some(img) => {
                            self.gap(4.0);
                            self.place(img, 1.0 / DIAGRAM_PX, false);
                            self.y += 4.0;
                        }
                        None => self.code_block(&c), // 그릴 수 없는 다이어그램은 코드 그대로
                    }
                }
            }
            TagEnd::List(_) => {
                self.flush(0.0);
                self.lists.pop();
                self.x -= LIST_W;
                if self.lists.is_empty() {
                    self.gap(5.0);
                }
            }
            TagEnd::Item => {
                self.flush(2.0);
                if self.marker.is_some() {
                    let h = BODY * LH;
                    self.draw_marker(h, BODY);
                    self.y += h;
                }
            }
            TagEnd::BlockQuote(_) => {
                self.flush(0.0);
                self.x -= QUOTE_W;
                if let Some((p0, y0, x)) = self.quotes.pop() {
                    let (p1, y1) = (self.pages.len() - 1, self.y);
                    for p in p0..=p1 {
                        let a = if p == p0 { y0 } else { 0.0 };
                        let b = if p == p1 { y1 } else { self.h };
                        self.pages[p].push(Op::Line {
                            x0: x + 3.0,
                            y0: a,
                            x1: x + 3.0,
                            y1: b,
                            color: 0x94A3B8,
                            width: 2.5,
                        });
                    }
                }
                self.y += 6.0;
            }
            TagEnd::TableCell => {
                let cell = std::mem::take(&mut self.runs);
                if let Some(row) = self.table.as_mut().and_then(|t| t.rows.last_mut()) {
                    row.1.push(cell);
                }
            }
            TagEnd::Table => {
                if let Some(t) = self.table.take() {
                    self.table(t);
                }
            }
            TagEnd::TableHead | TagEnd::TableRow => {}
            _ => self.flush(4.0),
        }
    }
}

// Mermaid 그림 해상도: egui 1pt(=인쇄 1pt)당 픽셀 (≈216dpi)
const DIAGRAM_PX: f32 = 3.0;

// 화면 밖 egui: 보기 화면과 같은 렌더러(밝은 테마, 종이색 바탕)로 그린 도형을 tessellate한 뒤
// 삼각형 메시를 소프트웨어로 래스터화해 흰 바탕 이미지로 만든다. 인쇄 한 번 동안 재사용한다.
struct Offscreen {
    ctx: egui::Context,
    pal: crate::theme::Pal,
    textures: std::collections::HashMap<egui::TextureId, egui::ColorImage>,
}

impl Offscreen {
    fn new() -> Self {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut pal = crate::theme::pal(false);
        (pal.bg, pal.surface) = (egui::Color32::WHITE, egui::Color32::WHITE); // 파이 가운데 구멍 등
        Self {
            ctx,
            pal,
            textures: std::collections::HashMap::new(),
        }
    }

    fn mermaid(&mut self, code: &str, width_pt: f32) -> Option<image::RgbaImage> {
        let pal = self.pal;
        self.render(width_pt, |ui| {
            crate::mermaid::render(ui, &pal, code, 11.0, width_pt).then(|| {
                let r = ui.min_rect();
                (r, r.bottom())
            })
        })
        .map(|(img, _)| img)
    }

    // 수식 그림과 기준선 위 높이(pt)
    fn math(&mut self, src: &str, size: f32, display: bool) -> Option<(image::RgbaImage, f32)> {
        let pal = self.pal;
        self.render(4000.0, |ui| {
            crate::math::render(ui, &pal, src, size, display)
        })
    }

    // draw가 돌려준 영역만 잘라 그림으로. 두 번째 값은 영역 위에서 기준선까지(pt)
    fn render(
        &mut self,
        width_pt: f32,
        mut draw: impl FnMut(&mut egui::Ui) -> Option<(egui::Rect, f32)>,
    ) -> Option<(image::RgbaImage, f32)> {
        use egui::epaint::Primitive;
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width_pt, 20_000.0),
            )),
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(DIAGRAM_PX);
        let mut got = None;
        let mut frame = |ui: &mut egui::Ui| got = draw(ui);
        // 글꼴은 첫 프레임이 시작될 때 적용되고, 처음 보는 위젯은 크기 재기 프레임이 있어 두 번 그린다
        let mut out = self.ctx.run_ui(input.clone(), &mut frame);
        for (id, ds) in out.textures_delta.set.drain() {
            ds.into_iter()
                .for_each(|d| apply_delta(&mut self.textures, id, d));
        }
        out.textures_delta.clear(); // 처리 안 한 변경이 남은 채 버리면 egui가 패닉한다
        let mut out = self.ctx.run_ui(input, &mut frame);
        for (id, ds) in out.textures_delta.set.drain() {
            ds.into_iter()
                .for_each(|d| apply_delta(&mut self.textures, id, d));
        }
        out.textures_delta.clear();
        let (used, base) = got?;
        if !used.is_positive() {
            return None;
        }
        let ppp = out.pixels_per_point;
        let (w, h) = (
            (used.width() * ppp).ceil() as u32,
            (used.height() * ppp).ceil() as u32,
        );
        let mut img = image::RgbaImage::from_pixel(w, h, image::Rgba([255, 255, 255, 255]));
        for prim in self.ctx.tessellate(std::mem::take(&mut out.shapes), ppp) {
            let Primitive::Mesh(mesh) = prim.primitive else {
                continue;
            };
            let Some(tex) = self.textures.get(&mesh.texture_id) else {
                continue;
            };
            let clip = prim.clip_rect.intersect(used);
            let to_px = |p: egui::Pos2| (p - used.min) * ppp;
            let (cx0, cy0) = (to_px(clip.min).x.max(0.0), to_px(clip.min).y.max(0.0));
            let (cx1, cy1) = (
                to_px(clip.max).x.min(w as f32),
                to_px(clip.max).y.min(h as f32),
            );
            for t in mesh.indices.as_chunks::<3>().0 {
                let v = [0, 1, 2].map(|k| &mesh.vertices[t[k] as usize]);
                let p = v.map(|v| to_px(v.pos));
                fill_triangle(&mut img, tex, p, v, (cx0, cy0, cx1, cy1));
            }
        }
        Some((img, (base - used.top()).max(0.0)))
    }
}

fn apply_delta(
    textures: &mut std::collections::HashMap<egui::TextureId, egui::ColorImage>,
    id: egui::TextureId,
    d: egui::epaint::ImageDelta,
) {
    let egui::epaint::ImageData::Color(src) = d.image;
    match d.pos {
        None => {
            textures.insert(id, (*src).clone());
        }
        Some([x0, y0]) => {
            if let Some(dst) = textures.get_mut(&id) {
                for y in 0..src.size[1] {
                    for x in 0..src.size[0] {
                        dst.pixels[(y0 + y) * dst.size[0] + x0 + x] =
                            src.pixels[y * src.size[0] + x];
                    }
                }
            }
        }
    }
}

// 픽셀 중심이 삼각형 안이면 (꼭짓점 색 × 텍스처)를 premultiplied "over"로 합성
fn fill_triangle(
    img: &mut image::RgbaImage,
    tex: &egui::ColorImage,
    p: [egui::Vec2; 3],
    v: [&egui::epaint::Vertex; 3],
    (cx0, cy0, cx1, cy1): (f32, f32, f32, f32),
) {
    let area = (p[1].x - p[0].x) * (p[2].y - p[0].y) - (p[2].x - p[0].x) * (p[1].y - p[0].y);
    if area.abs() < 1e-6 {
        return;
    }
    let x0 = p
        .iter()
        .map(|q| q.x)
        .fold(f32::MAX, f32::min)
        .max(cx0)
        .floor() as i32;
    let x1 = p
        .iter()
        .map(|q| q.x)
        .fold(f32::MIN, f32::max)
        .min(cx1)
        .ceil() as i32;
    let y0 = p
        .iter()
        .map(|q| q.y)
        .fold(f32::MAX, f32::min)
        .max(cy0)
        .floor() as i32;
    let y1 = p
        .iter()
        .map(|q| q.y)
        .fold(f32::MIN, f32::max)
        .min(cy1)
        .ceil() as i32;
    let edge = |a: egui::Vec2, b: egui::Vec2, x: f32, y: f32| {
        (b.x - a.x) * (y - a.y) - (b.y - a.y) * (x - a.x)
    };
    let [tw, th] = tex.size;
    for py in y0.max(0)..y1.min(img.height() as i32) {
        for px in x0.max(0)..x1.min(img.width() as i32) {
            let (x, y) = (px as f32 + 0.5, py as f32 + 0.5);
            let w0 = edge(p[1], p[2], x, y) / area;
            let w1 = edge(p[2], p[0], x, y) / area;
            let w2 = 1.0 - w0 - w1;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }
            let uv = v[0].uv.to_vec2() * w0 + v[1].uv.to_vec2() * w1 + v[2].uv.to_vec2() * w2;
            let tx = ((uv.x * tw as f32) as usize).min(tw - 1);
            let ty = ((uv.y * th as f32) as usize).min(th - 1);
            let t = tex.pixels[ty * tw + tx].to_array();
            let c = [0, 1, 2, 3].map(|k| {
                let vc = f32::from(v[0].color.to_array()[k]) * w0
                    + f32::from(v[1].color.to_array()[k]) * w1
                    + f32::from(v[2].color.to_array()[k]) * w2;
                vc * f32::from(t[k]) / 255.0
            });
            let a = c[3] / 255.0;
            let d = img.get_pixel_mut(px as u32, py as u32);
            for k in 0..3 {
                d[k] = (c[k] + f32::from(d[k]) * (1.0 - a))
                    .round()
                    .clamp(0.0, 255.0) as u8;
            }
        }
    }
}

// 굵게·형광·기울임·취소선을 편집 화면의 인라인 파서(inline_spans)로 찾아 HTML 태그로 바꾼다.
// CommonMark 규칙으로는 "**굵게!**입니다"처럼 문장부호 뒤 조사가 붙으면 서식이 풀리기 때문.
fn inline_tags(md: &str) -> String {
    let mut out = String::with_capacity(md.len() + 16);
    let mut fence = false;
    for (i, line) in md.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = !fence;
        }
        if fence || t.starts_with("```") || t.starts_with("~~~") {
            out.push_str(line);
            continue;
        }
        let mut at = 0;
        for (os, bs, be, ce, kind) in inline_spans(line) {
            let tag = match kind {
                Inline::Bold => "b",
                Inline::Mark => "mark",
                Inline::Italic => "i",
                Inline::Strike => "s",
                _ => continue,
            };
            out.push_str(&line[at..os]);
            out.push_str(&format!("<{tag}>"));
            out.push_str(&line[bs..be]);
            out.push_str(&format!("</{tag}>"));
            at = ce;
        }
        out.push_str(&line[at..]);
    }
    out
}

// 마크다운을 w×h(pt) 페이지들로 배치한다. 쪽 번호는 h 아래 18pt 기준선에 둔다.
pub(crate) fn layout(
    md: &str,
    m: &dyn Measure,
    load: &dyn Fn(&str) -> Option<image::RgbaImage>,
    w: f32,
    h: f32,
) -> Vec<Vec<Op>> {
    let clean = escape_plus_lines(&preprocess(&inline_tags(md)));
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_MATH
        | Options::ENABLE_GFM;
    let mut l = Layout {
        m,
        load,
        w,
        h,
        pages: vec![Vec::new()],
        y: 0.0,
        x: 0.0,
        runs: Vec::new(),
        bold: 0,
        italic: 0,
        strike: 0,
        mark: 0,
        link: 0,
        heading: None,
        lists: Vec::new(),
        marker: None,
        quotes: Vec::new(),
        code: None,
        table: None,
        in_image: 0,
        prefix: None,
        mermaid: false,
        off: None,
        inline_imgs: Vec::new(),
    };
    for ev in Parser::new_ext(&clean, opts) {
        l.event(ev);
    }
    l.flush(0.0);
    let mut pages = l.pages;
    while pages.len() > 1 && pages.last().is_some_and(Vec::is_empty) {
        pages.pop();
    }
    let n = pages.len();
    for (i, page) in pages.iter_mut().enumerate() {
        let mut st = Style::body();
        st.size = 8.5;
        st.color = MUTED;
        let label = format!("{} / {n}", i + 1);
        let lw = m.width(&label, &st);
        page.push(Op::Text {
            x: (w - lw) / 2.0,
            y: h + 18.0,
            text: label,
            style: st,
        });
    }
    pages
}

// ---------- Windows GDI ----------

#[cfg(windows)]
mod gdi {
    use super::{Measure, Op, Style};
    use std::cell::RefCell;
    use std::collections::HashMap;
    use windows_sys::Win32::Foundation::{GlobalFree, POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::*;
    use windows_sys::Win32::Storage::Xps::{EndDoc, EndPage, StartDocW, StartPage, DOCINFOW};
    use windows_sys::Win32::UI::Controls::Dialogs::{
        CommDlgExtendedError, PrintDlgW, PD_NOPAGENUMS, PD_NOSELECTION, PD_RETURNDC,
        PD_USEDEVMODECOPIESANDCOLLATE, PRINTDLGW,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn rgb(c: u32) -> u32 {
        ((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF)
    }

    type FontKey = (u32, bool, bool, bool, bool, bool);

    pub(super) struct Gdi {
        hdc: HDC,
        scale: f32, // 장치 픽셀 / pt
        fonts: RefCell<HashMap<FontKey, HFONT>>,
    }

    impl Gdi {
        fn font(&self, st: &Style) -> HFONT {
            let key = (
                (st.size * 100.0) as u32,
                st.bold,
                st.italic,
                st.mono,
                st.underline,
                st.strike,
            );
            *self.fonts.borrow_mut().entry(key).or_insert_with(|| {
                let face = wide(if st.mono { "Consolas" } else { "Malgun Gothic" });
                unsafe {
                    CreateFontW(
                        -((st.size * self.scale).round() as i32),
                        0,
                        0,
                        0,
                        if st.bold { 700 } else { 400 },
                        u32::from(st.italic),
                        u32::from(st.underline),
                        u32::from(st.strike),
                        u32::from(DEFAULT_CHARSET),
                        u32::from(OUT_DEFAULT_PRECIS),
                        u32::from(CLIP_DEFAULT_PRECIS),
                        u32::from(CLEARTYPE_QUALITY),
                        u32::from(DEFAULT_PITCH | FF_DONTCARE),
                        face.as_ptr(),
                    )
                }
            })
        }
    }

    impl Measure for Gdi {
        fn width(&self, text: &str, st: &Style) -> f32 {
            let w: Vec<u16> = text.encode_utf16().collect();
            let mut sz = windows_sys::Win32::Foundation::SIZE { cx: 0, cy: 0 };
            unsafe {
                SelectObject(self.hdc, self.font(st));
                GetTextExtentPoint32W(self.hdc, w.as_ptr(), w.len() as i32, &mut sz);
            }
            sz.cx as f32 / self.scale
        }
    }

    impl Drop for Gdi {
        fn drop(&mut self) {
            for f in self.fonts.borrow().values() {
                unsafe { DeleteObject(*f) };
            }
        }
    }

    // 인쇄 대화상자 → 문서를 그 프린터로 보낸다. 취소하면 Ok(false).
    pub(super) fn print_dialog(
        title: &str,
        md: &str,
        load: &dyn Fn(&str) -> Option<image::RgbaImage>,
    ) -> Result<bool, String> {
        unsafe {
            let mut pd: PRINTDLGW = std::mem::zeroed();
            pd.lStructSize = std::mem::size_of::<PRINTDLGW>() as u32;
            pd.Flags = PD_RETURNDC | PD_NOSELECTION | PD_NOPAGENUMS | PD_USEDEVMODECOPIESANDCOLLATE;
            let ok = PrintDlgW(&mut pd) != 0;
            for g in [pd.hDevMode, pd.hDevNames] {
                if !g.is_null() {
                    GlobalFree(g);
                }
            }
            if !ok {
                let err = CommDlgExtendedError();
                return if err == 0 {
                    Ok(false)
                } else {
                    Err(format!("인쇄 대화상자 오류 {err:#x}"))
                };
            }
            let r = print_to_dc(pd.hDC, title, md, load, None);
            DeleteDC(pd.hDC);
            r.map(|()| true)
        }
    }

    // 프린터 이름으로 바로 인쇄 (output이 있으면 그 파일로 — "Microsoft Print to PDF" 검증용)
    #[cfg(test)]
    pub(super) fn print_to_printer(
        printer: &str,
        output: &str,
        title: &str,
        md: &str,
        load: &dyn Fn(&str) -> Option<image::RgbaImage>,
    ) -> Result<(), String> {
        unsafe {
            let name = wide(printer);
            let hdc = CreateDCW(
                std::ptr::null(),
                name.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
            );
            if hdc.is_null() {
                return Err(format!("프린터 없음: {printer}"));
            }
            let r = print_to_dc(hdc, title, md, load, Some(output));
            DeleteDC(hdc);
            r
        }
    }

    unsafe fn print_to_dc(
        hdc: HDC,
        title: &str,
        md: &str,
        load: &dyn Fn(&str) -> Option<image::RgbaImage>,
        output: Option<&str>,
    ) -> Result<(), String> {
        let dpi = GetDeviceCaps(hdc, LOGPIXELSY as _) as f32;
        let scale = dpi / 72.0;
        let (pw, ph) = (
            GetDeviceCaps(hdc, HORZRES as _) as f32 / scale,
            GetDeviceCaps(hdc, VERTRES as _) as f32 / scale,
        );
        let (ml, mt, mb) = (40.0, 36.0, 48.0);
        let g = Gdi {
            hdc,
            scale,
            fonts: RefCell::new(HashMap::new()),
        };
        let pages = super::layout(md, &g, load, pw - 2.0 * ml, ph - mt - mb);
        let doc = wide(title);
        let out = output.map(wide);
        let mut di: DOCINFOW = std::mem::zeroed();
        di.cbSize = std::mem::size_of::<DOCINFOW>() as i32;
        di.lpszDocName = doc.as_ptr();
        if let Some(o) = &out {
            di.lpszOutput = o.as_ptr();
        }
        if StartDocW(hdc, &di) <= 0 {
            return Err("인쇄 작업을 시작하지 못했습니다".into());
        }
        let px = |v: f32| (v * scale).round() as i32;
        SetBkMode(hdc, TRANSPARENT as _);
        for page in &pages {
            StartPage(hdc);
            SetBkMode(hdc, TRANSPARENT as _);
            SetTextAlign(hdc, TA_BASELINE | TA_LEFT);
            for op in page {
                match op {
                    Op::Text { x, y, text, style } => {
                        let w: Vec<u16> = text.encode_utf16().collect();
                        SelectObject(hdc, g.font(style));
                        SetTextColor(hdc, rgb(style.color));
                        TextOutW(hdc, px(ml + x), px(mt + y), w.as_ptr(), w.len() as i32);
                    }
                    Op::Rect { x, y, w, h, fill } => {
                        let b = CreateSolidBrush(rgb(*fill));
                        let r = RECT {
                            left: px(ml + x),
                            top: px(mt + y),
                            right: px(ml + x + w),
                            bottom: px(mt + y + h),
                        };
                        FillRect(hdc, &r, b);
                        DeleteObject(b);
                    }
                    Op::Line {
                        x0,
                        y0,
                        x1,
                        y1,
                        color,
                        width,
                    } => {
                        let pen = CreatePen(PS_SOLID, px(*width).max(1), rgb(*color));
                        let old = SelectObject(hdc, pen);
                        let mut prev = POINT { x: 0, y: 0 };
                        MoveToEx(hdc, px(ml + x0), px(mt + y0), &mut prev);
                        LineTo(hdc, px(ml + x1), px(mt + y1));
                        SelectObject(hdc, old);
                        DeleteObject(pen);
                    }
                    Op::Image { x, y, w, h, img } => {
                        // 프린터는 알파를 모르므로 흰 바탕에 합성한 BGRA로 보낸다
                        let mut bgra = Vec::with_capacity(img.len());
                        for p in img.pixels() {
                            let a = u32::from(p[3]);
                            let mix = |c: u8| ((u32::from(c) * a + 255 * (255 - a)) / 255) as u8;
                            bgra.extend_from_slice(&[mix(p[2]), mix(p[1]), mix(p[0]), 0]);
                        }
                        let mut bi: BITMAPINFO = std::mem::zeroed();
                        bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
                        bi.bmiHeader.biWidth = img.width() as i32;
                        bi.bmiHeader.biHeight = -(img.height() as i32);
                        bi.bmiHeader.biPlanes = 1;
                        bi.bmiHeader.biBitCount = 32;
                        bi.bmiHeader.biCompression = BI_RGB;
                        SetStretchBltMode(hdc, HALFTONE);
                        SetBrushOrgEx(hdc, 0, 0, std::ptr::null_mut());
                        StretchDIBits(
                            hdc,
                            px(ml + x),
                            px(mt + y),
                            px(*w),
                            px(*h),
                            0,
                            0,
                            img.width() as i32,
                            img.height() as i32,
                            bgra.as_ptr().cast(),
                            &bi,
                            DIB_RGB_COLORS,
                            SRCCOPY,
                        );
                    }
                }
            }
            EndPage(hdc);
        }
        EndDoc(hdc);
        Ok(())
    }
}

// 현재 문서를 Windows 인쇄 대화상자로 인쇄한다. 이미지 경로는 문서 폴더 기준.
#[cfg(windows)]
pub(crate) fn print_markdown(
    title: &str,
    md: &str,
    doc_dir: &std::path::Path,
) -> Result<bool, String> {
    let load = image_loader(doc_dir);
    gdi::print_dialog(title, md, &load)
}

fn image_loader(doc_dir: &std::path::Path) -> impl Fn(&str) -> Option<image::RgbaImage> + '_ {
    move |url: &str| {
        if url.contains("://") {
            return None;
        }
        let p = doc_dir.join(crate::pct_decode(url));
        image::open(p).ok().map(|i| i.to_rgba8())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 가짜 측정기: 한글 등 넓은 글자는 1em, 나머지는 0.5em
    struct Fake;
    impl Measure for Fake {
        fn width(&self, t: &str, st: &Style) -> f32 {
            t.chars()
                .map(|c| if c.is_ascii() { 0.5 } else { 1.0 })
                .sum::<f32>()
                * st.size
        }
    }

    fn texts(pages: &[Vec<Op>]) -> Vec<(usize, String, Style)> {
        let mut out = Vec::new();
        for (i, p) in pages.iter().enumerate() {
            for op in p {
                if let Op::Text { text, style, .. } = op {
                    out.push((i, text.clone(), style.clone()));
                }
            }
        }
        out
    }

    #[test]
    fn layout_wraps_paginates_and_styles() {
        let no_img = |_: &str| None;
        let long = "가나다라 ".repeat(40);
        let md = format!(
            "# 제목\n\n{long}\n\n==형광!==입니다 **굵게**\n\n1. 하나\n   1. 둘\n      1. 셋\n\n- [x] 끝\n\n| a | b |\n|---|--:|\n| 1 | 2 |\n\n```\ncode\n```\n"
        );
        let pages = layout(&md, &Fake, &no_img, 300.0, 200.0);
        let t = texts(&pages);
        let find = |s: &str| {
            t.iter()
                .find(|x| x.1.contains(s))
                .unwrap_or_else(|| panic!("{s}: {t:?}"))
        };
        assert!(find("제목").2.bold && find("제목").2.size > BODY);
        // 긴 문단은 여러 줄로 나뉘고 어느 줄도 폭을 넘지 않는다
        let lines = t.iter().filter(|x| x.1.contains("가나다라")).count();
        assert!(lines >= 4, "{lines}");
        for p in &pages {
            for op in p {
                if let Op::Text { x, text, style, .. } = op {
                    assert!(x + Fake.width(text, style) <= 300.0 + 0.01, "{text}");
                }
            }
        }
        // 형광펜(문장부호+조사)은 배경, 굵게는 배경 없음
        assert_eq!(find("형광!").2.bg, Some(MARK_BG));
        assert_eq!(find("굵게").2.bg, None);
        assert!(find("굵게").2.bold && !t.iter().any(|x| x.1.contains("**")));
        // 번호 계층
        for m in ["1.", "1)", "(1)"] {
            assert!(t.iter().any(|x| x.1 == m), "{m}");
        }
        assert!(find("code").2.mono);
        assert!(t.iter().any(|x| x.1 == "2"));
        // 쪽 나눔과 쪽 번호
        let n = pages.len();
        assert!(n >= 2, "{n}");
        assert!(t.iter().any(|x| x.1 == format!("{n} / {n}")));
    }

    // 보기 화면의 줄 안 수식: 자리 잡은 NBSP 위치에 수식이 그려져 글자 사이에 잉크가 있다
    #[test]
    fn view_inline_math_is_typeset() {
        let mut off = Offscreen::new();
        let (img, _) = off
            .render(600.0, |ui| {
                let r = crate::ui::test_paint_inline(ui, "앞 $\\frac{a}{b}$ 뒤", 16.0, 560.0);
                Some((r, r.bottom()))
            })
            .unwrap();
        if let Ok(p) = std::env::var("MDEDITOR_VIEW_PNG") {
            img.save(&p).unwrap();
            // 눈으로 확인용: 여러 수식이 섞여 줄이 바뀌는 문단
            let src = r"본문 속 수식 $E = mc^2$ 와 $\alpha + \beta \le \frac{1}{2}$, 그리고 $\sqrt{x^2+y^2}$ 가 줄 안에 들어가고 $\sum_{i=1}^n i$ 도 됩니다.";
            let (long, _) = off
                .render(320.0, |ui| {
                    let r = crate::ui::test_paint_inline(ui, src, 16.0, 300.0);
                    Some((r, r.bottom()))
                })
                .unwrap();
            long.save(p.replace(".png", "-long.png")).unwrap();
        }
        // 분수라 줄 높이가 보통 글줄(16×1.6≈26pt)보다 높다
        assert!(img.height() as f32 / DIAGRAM_PX > 30.0, "{}", img.height());
        // "앞 " 뒤 수식 자리 가운데 세로줄에 잉크(분수선·글자)가 있다
        let x = (img.width() as f32 * 0.07) as u32..(img.width() as f32 * 0.12) as u32;
        assert!(x
            .into_iter()
            .any(|x| (0..img.height()).any(|y| img.get_pixel(x, y)[0] < 128)));
    }

    #[test]
    fn math_prints_as_pictures_inline_and_block() {
        let no_img = |_: &str| None;
        let md = "앞 $x^2$ 뒤\n\n$$\n\\frac{a}{b}\n$$\n";
        let pages = layout(md, &Fake, &no_img, 500.0, 700.0);
        let imgs: Vec<(f32, f32, f32)> = pages[0]
            .iter()
            .filter_map(|op| match op {
                Op::Image { x, y, h, .. } => Some((*x, *y, *h)),
                _ => None,
            })
            .collect();
        assert_eq!(imgs.len(), 2, "{imgs:?}");
        let t = texts(&pages);
        assert!(!t
            .iter()
            .any(|x| x.1.contains("x^2") || x.1.contains("frac")));
        // 줄 안 수식은 "앞" 뒤에 같은 줄로, 블록 수식은 가운데로
        let (ax, _) = pages[0]
            .iter()
            .find_map(|op| match op {
                Op::Text { x, y, text, .. } if text.contains('앞') => Some((*x, *y)),
                _ => None,
            })
            .unwrap();
        assert!(imgs[0].0 > ax && imgs[0].1 < 30.0, "{imgs:?}");
        assert!(imgs[1].0 > 100.0, "display math centered: {imgs:?}");
    }

    #[test]
    fn mermaid_prints_as_picture() {
        let no_img = |_: &str| None;
        let md = "```mermaid\nflowchart LR\n  A[시작] --> B[끝]\n```\n";
        let pages = layout(md, &Fake, &no_img, 500.0, 700.0);
        let img = pages[0]
            .iter()
            .find_map(|op| match op {
                Op::Image { img, w, .. } => Some((img.clone(), *w)),
                _ => None,
            })
            .expect("mermaid → image");
        // 흰 바탕 위에 무언가 그려졌고, 코드 글자는 인쇄하지 않는다
        assert!(img.0.pixels().any(|p| p[0] < 200), "blank diagram");
        assert!(img.1 <= 500.0);
        assert!(!texts(&pages).iter().any(|t| t.1.contains("flowchart")));
    }

    // 실제 GDI로 "Microsoft Print to PDF"에 인쇄해 PDF를 만든다 (눈으로 확인용)
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn print_sample_to_pdf() {
        let out = std::env::var("MDEDITOR_PRINT_PDF").unwrap_or_else(|_| {
            std::env::temp_dir()
                .join("mdeditor-print-test.pdf")
                .to_string_lossy()
                .into_owned()
        });
        let _ = std::fs::remove_file(&out);
        let md = std::fs::read_to_string(std::env::var("MDEDITOR_PRINT_MD").unwrap()).unwrap();
        let dir = std::path::PathBuf::from(std::env::var("MDEDITOR_PRINT_MD").unwrap());
        let load = image_loader(dir.parent().unwrap());
        gdi::print_to_printer("Microsoft Print to PDF", &out, "test", &md, &load).unwrap();
        for _ in 0..50 {
            if std::fs::metadata(&out).is_ok_and(|m| m.len() > 0) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(std::fs::metadata(&out).unwrap().len() > 0);
    }
}
