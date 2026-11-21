// 순수 네이티브 Mermaid 다이어그램 렌더러 (eframe/egui Painter 기반, 브라우저·외부 의존성 없음)
// 지원 다이어그램:
//   - Flowchart / Graph (graph/flowchart TD, TB, BT, LR, RL)
//   - Sequence Diagram (sequenceDiagram)
//   - State Diagram (stateDiagram, stateDiagram-v2)
//   - Class / ER Diagram (classDiagram, erDiagram)
//   - Pie Chart (pie)
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use eframe::egui;
use egui::{pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2};

use crate::theme::{self, Pal};

const PALETTE: [[u8; 3]; 8] = [
    [0x23, 0x83, 0xe2], // Blue
    [0x10, 0xb9, 0x81], // Emerald
    [0x8b, 0x5c, 0xf6], // Violet
    [0xf5, 0x9e, 0x0b], // Amber
    [0xf4, 0x3f, 0x5e], // Rose
    [0x06, 0xb6, 0xd4], // Cyan
    [0x63, 0x66, 0xf1], // Indigo
    [0xec, 0x48, 0x99], // Pink
];

pub fn render(ui: &mut egui::Ui, p: &Pal, code: &str, body_size: f32, avail_width: f32) -> bool {
    let trimmed = strip_frontmatter(code).trim();
    if trimmed.is_empty() {
        return false;
    }
    let first_line = trimmed
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("%%"))
        .unwrap_or("");
    let lower = first_line.to_ascii_lowercase();
    if lower.starts_with("sequencediagram") {
        render_sequence(ui, p, trimmed, body_size, avail_width)
    } else if lower.starts_with("pie") {
        render_pie(ui, p, trimmed, body_size, avail_width)
    } else {
        render_flowchart(ui, p, trimmed, body_size, avail_width)
    }
}

fn strip_frontmatter(s: &str) -> &str {
    let t = s.trim_start();
    if let Some(rest) = t.strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            let after = &rest[end + 4..];
            return after.strip_prefix('\n').unwrap_or(after);
        }
    }
    s
}

// ============================================================================
// 1. Flowchart / Graph / State / Class / ER 다이어그램
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Dir {
    TD,
    BT,
    LR,
    RL,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NodeShape {
    Rect,
    Round,
    Stadium,
    Subroutine,
    Cylinder,
    Circle,
    DoubleCircle,
    Diamond,
    Hexagon,
    Parallelogram,
    Asymmetric,
    StartState,
    EndState,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LineStyle {
    Solid,
    Dotted,
    Thick,
}

#[derive(Clone)]
struct Node {
    label: String,
    shape: NodeShape,
    fill: Option<Color32>,
    stroke: Option<Color32>,
    size: Vec2,
    pos: Pos2, // 중심 좌표
}

#[derive(Clone)]
struct Edge {
    from: usize,
    to: usize,
    label: String,
    style: LineStyle,
    arrow_end: bool,
    arrow_start: bool,
}

struct Subgraph {
    title: String,
    nodes: Vec<usize>,
}

struct Graph {
    dir: Dir,
    nodes: Vec<Node>,
    node_map: HashMap<String, usize>,
    edges: Vec<Edge>,
    subgraphs: Vec<Subgraph>,
    classes: HashMap<String, (Option<Color32>, Option<Color32>)>,
    node_classes: HashMap<String, String>,
}

impl Graph {
    fn new(dir: Dir) -> Self {
        Self {
            dir,
            nodes: Vec::new(),
            node_map: HashMap::new(),
            edges: Vec::new(),
            subgraphs: Vec::new(),
            classes: HashMap::new(),
            node_classes: HashMap::new(),
        }
    }

    fn ensure_node(
        &mut self,
        id: &str,
        label: Option<&str>,
        shape: Option<NodeShape>,
        active_subgraphs: &[usize],
    ) -> usize {
        let idx = if let Some(&i) = self.node_map.get(id) {
            if let Some(l) = label {
                if !l.is_empty() {
                    self.nodes[i].label = l.to_string();
                }
            }
            if let Some(sh) = shape {
                self.nodes[i].shape = sh;
            }
            i
        } else {
            let i = self.nodes.len();
            self.nodes.push(Node {
                label: label.unwrap_or(id).to_string(),
                shape: shape.unwrap_or(NodeShape::Rect),
                fill: None,
                stroke: None,
                size: Vec2::ZERO,
                pos: Pos2::ZERO,
            });
            self.node_map.insert(id.to_string(), i);
            i
        };
        for &sg in active_subgraphs {
            if let Some(sub) = self.subgraphs.get_mut(sg) {
                if !sub.nodes.contains(&idx) {
                    sub.nodes.push(idx);
                }
            }
        }
        idx
    }
}

fn parse_hex_color(s: &str) -> Option<Color32> {
    let hex = s.trim().strip_prefix('#')?;
    match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            Some(Color32::from_rgb(r, g, b))
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(Color32::from_rgb(r, g, b))
        }
        _ => None,
    }
}

fn parse_style_attrs(spec: &str) -> (Option<Color32>, Option<Color32>) {
    let mut fill = None;
    let mut stroke = None;
    for part in spec.split(',') {
        if let Some((k, v)) = part.split_once(':') {
            let k = k.trim();
            let v = v.trim().trim_end_matches(';');
            if k.eq_ignore_ascii_case("fill") {
                fill = parse_hex_color(v);
            } else if k.eq_ignore_ascii_case("stroke") {
                stroke = parse_hex_color(v);
            }
        }
    }
    (fill, stroke)
}

fn clean_label(s: &str) -> String {
    let s = s.trim();
    let s = if (s.starts_with('"') && s.ends_with('"') && s.len() >= 2)
        || (s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2)
    {
        &s[1..s.len() - 1]
    } else {
        s
    };
    s.replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<br>", "\n")
        .replace("\\n", "\n")
}

type ParsedNodeToken = (String, Option<String>, Option<NodeShape>, Option<String>);

// 노드 토큰 파싱: 예) A["시작 (Start)"]:::primary, B{조건?}, C((끝)), [*]
fn parse_node_token(raw: &str, is_target: bool) -> Option<ParsedNodeToken> {
    let mut s = raw.trim().trim_end_matches(';').trim();
    if s.is_empty() {
        return None;
    }
    let mut class_name = None;
    if let Some((before, cls)) = s.rsplit_once(":::") {
        if !cls.contains(['[', '(', '{', '"']) {
            s = before.trim();
            class_name = Some(cls.trim().to_string());
        }
    }
    if s == "[*]" {
        return if is_target {
            Some((
                "__end__".into(),
                Some(String::new()),
                Some(NodeShape::EndState),
                None,
            ))
        } else {
            Some((
                "__start__".into(),
                Some(String::new()),
                Some(NodeShape::StartState),
                None,
            ))
        };
    }
    let open_pos = s.find(['[', '(', '{', '>']);
    let Some(pos) = open_pos else {
        let id = clean_label(s);
        return (!id.is_empty()).then_some((id, None, None, class_name));
    };
    let id = s[..pos].trim();
    if id.is_empty() {
        return None;
    }
    let rest = &s[pos..];
    let patterns: &[(&str, &str, NodeShape)] = &[
        ("(((", "))", NodeShape::DoubleCircle),
        ("([", "])", NodeShape::Stadium),
        ("[[", "]]", NodeShape::Subroutine),
        ("[(", ")]", NodeShape::Cylinder),
        ("((", "))", NodeShape::Circle),
        ("{{", "}}", NodeShape::Hexagon),
        ("[/", "/]", NodeShape::Parallelogram),
        ("[\\", "\\]", NodeShape::Parallelogram),
        ("[/", "\\]", NodeShape::Parallelogram),
        ("[\\", "/]", NodeShape::Parallelogram),
        (">", "]", NodeShape::Asymmetric),
        ("[", "]", NodeShape::Rect),
        ("(", ")", NodeShape::Round),
        ("{", "}", NodeShape::Diamond),
    ];
    for &(open, close, shape) in patterns {
        if let Some(inner) = rest.strip_prefix(open).and_then(|r| r.strip_suffix(close)) {
            return Some((
                id.to_string(),
                Some(clean_label(inner)),
                Some(shape),
                class_name,
            ));
        }
    }
    Some((
        id.to_string(),
        Some(clean_label(rest)),
        Some(NodeShape::Rect),
        class_name,
    ))
}

struct ParsedConn {
    left_end: usize,
    right_start: usize,
    label: String,
    style: LineStyle,
    arrow_start: bool,
    arrow_end: bool,
}

// 한 줄에서 첫 번째 연결 연산자(-->, -.->, ==>, ---, -- text -->, -->|text| 등)를 찾는다
fn find_first_connector(s: &str) -> Option<ParsedConn> {
    let bytes = s.as_bytes();
    let mut in_quote = false;
    let mut quote_ch = b'"';
    let mut bracket_depth: i32 = 0;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if in_quote {
            if b == quote_ch {
                in_quote = false;
            }
            i += 1;
            continue;
        }
        if b == b'"' || b == b'\'' {
            in_quote = true;
            quote_ch = b;
            i += 1;
            continue;
        }
        if matches!(b, b'[' | b'(' | b'{') && !(i > 0 && bytes[i - 1] == b'-') {
            bracket_depth += 1;
            i += 1;
            continue;
        }
        if matches!(b, b']' | b')' | b'}') && bracket_depth > 0 {
            bracket_depth -= 1;
            i += 1;
            continue;
        }
        if bracket_depth > 0 {
            i += 1;
            continue;
        }

        let tail = &s[i..];
        // 1. 인라인 라벨 문법: -- label -->, -. label .->, == label ==>
        for &(open, close, style, a_start, a_end) in &[
            ("-- ", " -->", LineStyle::Solid, false, true),
            ("-- ", " ---", LineStyle::Solid, false, false),
            ("-. ", " .->", LineStyle::Dotted, false, true),
            ("-. ", " -.-", LineStyle::Dotted, false, false),
            ("== ", " ==>", LineStyle::Thick, false, true),
            ("== ", " ===", LineStyle::Thick, false, false),
        ] {
            if let Some(after_open) = tail.strip_prefix(open) {
                if let Some(close_idx) = after_open.find(close) {
                    let mid_label = after_open[..close_idx].trim();
                    if !mid_label.is_empty() && !mid_label.contains("-->") {
                        let right_start = i + open.len() + close_idx + close.len();
                        return Some(ParsedConn {
                            left_end: i,
                            right_start,
                            label: clean_label(mid_label),
                            style,
                            arrow_start: a_start,
                            arrow_end: a_end,
                        });
                    }
                }
            }
        }

        // 2. 일반 화살표 연산자 (+ 뒤따르는 |라벨| 지원)
        let ops: &[(&str, LineStyle, bool, bool)] = &[
            ("<-->", LineStyle::Solid, true, true),
            ("<-.->", LineStyle::Dotted, true, true),
            ("<==>", LineStyle::Thick, true, true),
            ("-.->", LineStyle::Dotted, false, true),
            ("-.-", LineStyle::Dotted, false, false),
            ("===>", LineStyle::Thick, false, true),
            ("==>", LineStyle::Thick, false, true),
            ("===", LineStyle::Thick, false, false),
            ("--->", LineStyle::Solid, false, true),
            ("-->", LineStyle::Solid, false, true),
            ("---", LineStyle::Solid, false, false),
            ("..|>", LineStyle::Dotted, false, true),
            ("<|..", LineStyle::Dotted, true, false),
            ("..>", LineStyle::Dotted, false, true),
            ("<|--", LineStyle::Solid, true, false),
            ("--|>", LineStyle::Solid, false, true),
            ("*--", LineStyle::Solid, true, false),
            ("--*", LineStyle::Solid, false, true),
            ("o--", LineStyle::Solid, true, false),
            ("--o", LineStyle::Solid, false, true),
            ("||--o{", LineStyle::Solid, false, true),
            ("}o--||", LineStyle::Solid, true, false),
            ("||--|{", LineStyle::Solid, false, true),
            ("||--||", LineStyle::Solid, false, false),
            ("->", LineStyle::Solid, false, true),
        ];
        for &(op, style, a_start, a_end) in ops {
            if let Some(after_op) = tail.strip_prefix(op) {
                let trimmed_after = after_op.trim_start();
                let ws_len = after_op.len() - trimmed_after.len();
                if let Some(after_pipe) = trimmed_after.strip_prefix('|') {
                    if let Some(end_pipe) = after_pipe.find('|') {
                        let lbl = clean_label(&after_pipe[..end_pipe]);
                        let right_start = i + op.len() + ws_len + 1 + end_pipe + 1;
                        return Some(ParsedConn {
                            left_end: i,
                            right_start,
                            label: lbl,
                            style,
                            arrow_start: a_start,
                            arrow_end: a_end,
                        });
                    }
                }
                return Some(ParsedConn {
                    left_end: i,
                    right_start: i + op.len(),
                    label: String::new(),
                    style,
                    arrow_start: a_start,
                    arrow_end: a_end,
                });
            }
        }
        i += 1;
    }
    None
}

fn parse_graph(code: &str) -> Graph {
    let mut dir = Dir::TD;
    let mut lines = code.lines().peekable();
    while let Some(line) = lines.peek() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("%%") {
            lines.next();
            continue;
        }
        let lower = t.to_ascii_lowercase();
        if lower.starts_with("graph") || lower.starts_with("flowchart") {
            let parts: Vec<&str> = t.split_whitespace().collect();
            if let Some(d) = parts.get(1) {
                dir = match d.trim_end_matches(';').to_ascii_uppercase().as_str() {
                    "LR" => Dir::LR,
                    "RL" => Dir::RL,
                    "BT" => Dir::BT,
                    _ => Dir::TD,
                };
            }
            lines.next();
        } else if lower.starts_with("statediagram")
            || lower.starts_with("classdiagram")
            || lower.starts_with("erdiagram")
            || lower.starts_with("mindmap")
        {
            if lower.starts_with("erdiagram") {
                dir = Dir::LR;
            }
            lines.next();
        }
        break;
    }

    let mut g = Graph::new(dir);
    let mut active_subgraphs: Vec<usize> = Vec::new();
    let mut current_class_block: Option<(String, Vec<String>)> = None;

    for raw_line in lines {
        let t = raw_line.trim().trim_end_matches(';').trim();
        if t.is_empty() || t.starts_with("%%") {
            continue;
        }
        // Class / ER 멀티라인 블록 내부 처리
        if let Some((cls_id, mut members)) = current_class_block.take() {
            if t == "}" {
                let label = if members.is_empty() {
                    cls_id.clone()
                } else {
                    format!("{cls_id}\n────────\n{}", members.join("\n"))
                };
                g.ensure_node(
                    &cls_id,
                    Some(&label),
                    Some(NodeShape::Rect),
                    &active_subgraphs,
                );
            } else {
                members.push(t.to_string());
                current_class_block = Some((cls_id, members));
            }
            continue;
        }

        let lower = t.to_ascii_lowercase();
        if lower == "end" {
            active_subgraphs.pop();
            continue;
        }
        if lower.starts_with("direction ") {
            if let Some(d) = t.split_whitespace().nth(1) {
                if active_subgraphs.is_empty() {
                    g.dir = match d.to_ascii_uppercase().as_str() {
                        "LR" => Dir::LR,
                        "RL" => Dir::RL,
                        "BT" => Dir::BT,
                        _ => Dir::TD,
                    };
                }
            }
            continue;
        }
        if lower.starts_with("subgraph ") {
            let rest = t[9..].trim();
            let title = if let Some(open) = rest.find('[') {
                rest[open + 1..].trim_end_matches(']').trim()
            } else {
                rest
            };
            let sg_idx = g.subgraphs.len();
            g.subgraphs.push(Subgraph {
                title: clean_label(title),
                nodes: Vec::new(),
            });
            active_subgraphs.push(sg_idx);
            continue;
        }
        if lower.starts_with("classdef ") {
            let parts: Vec<&str> = t[9..].splitn(2, char::is_whitespace).collect();
            if parts.len() == 2 {
                g.classes
                    .insert(parts[0].trim().to_string(), parse_style_attrs(parts[1]));
            }
            continue;
        }
        if lower.starts_with("class ") && !t.contains('{') && find_first_connector(t).is_none() {
            let parts: Vec<&str> = t[6..].split_whitespace().collect();
            if parts.len() >= 2 {
                let cls = parts[1].trim();
                for n_id in parts[0].split(',') {
                    g.node_classes
                        .insert(n_id.trim().to_string(), cls.to_string());
                }
            }
            continue;
        }
        if lower.starts_with("style ") {
            let parts: Vec<&str> = t[6..].splitn(2, char::is_whitespace).collect();
            if parts.len() == 2 {
                let node_id = parts[0].trim();
                let (fill, stroke) = parse_style_attrs(parts[1]);
                let idx = g.ensure_node(node_id, None, None, &active_subgraphs);
                g.nodes[idx].fill = fill.or(g.nodes[idx].fill);
                g.nodes[idx].stroke = stroke.or(g.nodes[idx].stroke);
            }
            continue;
        }
        if lower.starts_with("state ") && !t.contains("-->") {
            let rest = t[6..].trim();
            if let Some((desc, id)) = rest.rsplit_once(" as ") {
                let id = id.trim().trim_end_matches('{').trim();
                g.ensure_node(
                    id,
                    Some(&clean_label(desc)),
                    Some(NodeShape::Round),
                    &active_subgraphs,
                );
            }
            continue;
        }
        if let Some(rest) = t.strip_prefix("class ") {
            if let Some(cls_name) = rest.strip_suffix('{') {
                current_class_block = Some((cls_name.trim().to_string(), Vec::new()));
                continue;
            }
        }
        if let Some(cls_name) = t.strip_suffix('{') {
            if !cls_name.contains(['[', '(', '-']) {
                current_class_block = Some((cls_name.trim().to_string(), Vec::new()));
                continue;
            }
        }

        // 체인 연결선 파싱: A --> B --> C : label
        if find_first_connector(t).is_some() {
            let mut rem = t;
            let mut prev_nodes: Vec<usize> = Vec::new();
            let mut pending_conn: Option<(String, LineStyle, bool, bool)> = None;

            while let Some(conn) = find_first_connector(rem) {
                let left_part = rem[..conn.left_end].trim();
                let mut cur_nodes = Vec::new();
                for item in left_part.split('&') {
                    if let Some((id, lbl, shp, cls)) =
                        parse_node_token(item, prev_nodes.is_empty() && false)
                    {
                        let idx = g.ensure_node(&id, lbl.as_deref(), shp, &active_subgraphs);
                        if let Some(c) = cls {
                            g.node_classes.insert(id, c);
                        }
                        cur_nodes.push(idx);
                    }
                }
                if let Some((lbl, style, a_start, a_end)) = pending_conn.take() {
                    for &u in &prev_nodes {
                        for &v in &cur_nodes {
                            g.edges.push(Edge {
                                from: u,
                                to: v,
                                label: lbl.clone(),
                                style,
                                arrow_start: a_start,
                                arrow_end: a_end,
                            });
                        }
                    }
                }
                if !cur_nodes.is_empty() {
                    prev_nodes = cur_nodes;
                }
                pending_conn = Some((conn.label, conn.style, conn.arrow_start, conn.arrow_end));
                rem = &rem[conn.right_start..];
            }

            // 마지막 오른쪽 피연산자 (+ ": 라벨" 지원)
            let (right_part, colon_label) = match rem.split_once(':') {
                Some((r, l)) if !r.contains('"') => (r.trim(), Some(clean_label(l))),
                _ => (rem.trim(), None),
            };
            let mut last_nodes = Vec::new();
            for item in right_part.split('&') {
                if let Some((id, lbl, shp, cls)) = parse_node_token(item, true) {
                    let idx = g.ensure_node(&id, lbl.as_deref(), shp, &active_subgraphs);
                    if let Some(c) = cls {
                        g.node_classes.insert(id, c);
                    }
                    last_nodes.push(idx);
                }
            }
            if let Some((mut lbl, style, a_start, a_end)) = pending_conn {
                if lbl.is_empty() {
                    if let Some(cl) = colon_label {
                        lbl = cl;
                    }
                }
                for &u in &prev_nodes {
                    for &v in &last_nodes {
                        g.edges.push(Edge {
                            from: u,
                            to: v,
                            label: lbl.clone(),
                            style,
                            arrow_start: a_start,
                            arrow_end: a_end,
                        });
                    }
                }
            }
            continue;
        }

        // Class 멤버 한 줄 문법: ClassName : +method()
        if let Some((cls, member)) = t.split_once(':') {
            let cls = cls.trim();
            let member = member.trim();
            if !cls.is_empty() && !cls.contains(' ') && !member.is_empty() {
                let idx = g.ensure_node(cls, None, Some(NodeShape::Rect), &active_subgraphs);
                if !g.nodes[idx].label.contains("────────") {
                    g.nodes[idx].label = format!("{}\n────────\n{member}", g.nodes[idx].label);
                } else {
                    g.nodes[idx].label.push('\n');
                    g.nodes[idx].label.push_str(member);
                }
                continue;
            }
        }

        // 단독 노드 선언: A["단독 노드"]
        if let Some((id, lbl, shp, cls)) = parse_node_token(t, false) {
            let _ = g.ensure_node(&id, lbl.as_deref(), shp, &active_subgraphs);
            if let Some(c) = cls {
                g.node_classes.insert(id, c);
            }
        }
    }

    for (node_id, cls_name) in &g.node_classes {
        if let (Some(&idx), Some(&(fill, stroke))) =
            (g.node_map.get(node_id), g.classes.get(cls_name))
        {
            g.nodes[idx].fill = g.nodes[idx].fill.or(fill);
            g.nodes[idx].stroke = g.nodes[idx].stroke.or(stroke);
        }
    }
    g
}

fn code_hash(code: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    code.hash(&mut h);
    h.finish()
}

struct FlowchartCache {
    hash: u64,
    code: String,
    body_bits: u32,
    width_bits: u32,
    dark: bool,
    graph: Arc<Graph>,
    total_w: f32,
    total_h: f32,
}

fn layout_flowchart_graph(
    ui: &egui::Ui,
    p: &Pal,
    code: &str,
    font_id: &FontId,
    avail_width: f32,
) -> Option<(Graph, f32, f32)> {
    let mut g = parse_graph(code);
    if g.nodes.is_empty() {
        return None;
    }

    // 1. 각 노드의 텍스트 크기 측정 (폰트 락 1회 일괄 측정)
    ui.ctx().fonts_mut(|f| {
        for node in &mut g.nodes {
            if matches!(node.shape, NodeShape::StartState | NodeShape::EndState) {
                node.size = vec2(22.0, 22.0);
                continue;
            }
            let text_size = f
                .layout_no_wrap(node.label.clone(), font_id.clone(), p.text)
                .size();
            let (pad_x, pad_y) = match node.shape {
                NodeShape::Diamond => (text_size.x * 0.45 + 28.0, text_size.y * 0.45 + 20.0),
                NodeShape::Circle | NodeShape::DoubleCircle => {
                    let d = text_size.x.max(text_size.y) + 26.0;
                    node.size = vec2(d.max(44.0), d.max(44.0));
                    continue;
                }
                NodeShape::Hexagon | NodeShape::Parallelogram | NodeShape::Asymmetric => {
                    (34.0, 18.0)
                }
                NodeShape::Cylinder => (28.0, 24.0),
                _ => (28.0, 16.0),
            };
            node.size = vec2(
                (text_size.x + pad_x).max(64.0),
                (text_size.y + pad_y).max(36.0),
            );
        }
    });

    // 2. 사이클을 제외한 DAG 위상 계층(Rank) 할당
    let n = g.nodes.len();
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut in_deg: Vec<usize> = vec![0; n];
    // DFS로 역방향 간선(back-edge) 탐지
    let mut raw_adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for e in &g.edges {
        if e.from != e.to {
            raw_adj[e.from].push(e.to);
        }
    }
    let mut state = vec![0u8; n]; // 0 미방문, 1 방문중, 2 완료
    let mut back_edges = Vec::new();
    fn dfs_cycle(
        u: usize,
        raw_adj: &[Vec<usize>],
        state: &mut [u8],
        back: &mut Vec<(usize, usize)>,
    ) {
        state[u] = 1;
        for &v in &raw_adj[u] {
            if state[v] == 1 {
                back.push((u, v));
            } else if state[v] == 0 {
                dfs_cycle(v, raw_adj, state, back);
            }
        }
        state[u] = 2;
    }
    for i in 0..n {
        if state[i] == 0 {
            dfs_cycle(i, &raw_adj, &mut state, &mut back_edges);
        }
    }
    for e in &g.edges {
        if e.from == e.to {
            continue;
        }
        let (u, v) = if back_edges.contains(&(e.from, e.to)) {
            (e.to, e.from)
        } else {
            (e.from, e.to)
        };
        if !adj[u].contains(&v) {
            adj[u].push(v);
            in_deg[v] += 1;
        }
    }

    let mut rank = vec![0usize; n];
    let mut queue: Vec<usize> = (0..n).filter(|&i| in_deg[i] == 0).collect();
    if queue.is_empty() {
        queue.push(0);
    }
    let mut head = 0;
    let mut visited = vec![false; n];
    while head < queue.len() {
        let u = queue[head];
        head += 1;
        visited[u] = true;
        for &v in &adj[u] {
            rank[v] = rank[v].max(rank[u] + 1);
            in_deg[v] = in_deg[v].saturating_sub(1);
            if in_deg[v] == 0 {
                queue.push(v);
            }
        }
    }
    for i in 0..n {
        if !visited[i] {
            for &v in &adj[i] {
                rank[v] = rank[v].max(rank[i] + 1);
            }
        }
    }

    let max_rank = rank.iter().copied().max().unwrap_or(0);
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); max_rank + 1];
    for (i, &r) in rank.iter().enumerate() {
        layers[r].push(i);
    }
    if matches!(g.dir, Dir::BT | Dir::RL) {
        layers.reverse();
    }

    // 3. Barycenter 교차 최소화 정렬 (4회 반복)
    for pass in 0..4 {
        if pass % 2 == 0 {
            for l in 1..layers.len() {
                let prev = layers[l - 1].clone();
                layers[l].sort_by(|&a, &b| {
                    let ba = barycenter(a, &prev, &g.edges);
                    let bb = barycenter(b, &prev, &g.edges);
                    ba.partial_cmp(&bb).unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        } else {
            for l in (0..layers.len().saturating_sub(1)).rev() {
                let next = layers[l + 1].clone();
                layers[l].sort_by(|&a, &b| {
                    let ba = barycenter(a, &next, &g.edges);
                    let bb = barycenter(b, &next, &g.edges);
                    ba.partial_cmp(&bb).unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }
    }

    // 4. 계층별 좌표 배치
    let has_labels = g.edges.iter().any(|e| !e.label.is_empty());
    let is_horizontal = matches!(g.dir, Dir::LR | Dir::RL);
    let pad = 24.0;
    let sg_extra = if g.subgraphs.is_empty() { 0.0 } else { 22.0 };

    let (total_w, total_h) = if !is_horizontal {
        let layer_gap = if has_labels { 64.0 } else { 48.0 } + sg_extra * 0.5;
        let node_gap = 28.0 + sg_extra * 0.4;
        let mut max_w: f32 = 160.0;
        let mut layer_widths = Vec::with_capacity(layers.len());
        let mut layer_heights = Vec::with_capacity(layers.len());
        for layer in &layers {
            let w: f32 = layer.iter().map(|&i| g.nodes[i].size.x).sum::<f32>()
                + node_gap * (layer.len().saturating_sub(1) as f32);
            let h: f32 = layer.iter().map(|&i| g.nodes[i].size.y).fold(0.0, f32::max);
            max_w = max_w.max(w);
            layer_widths.push(w);
            layer_heights.push(h);
        }
        let total_w = (max_w + pad * 2.0 + sg_extra * 2.0).max(avail_width.min(360.0));
        let mut y = pad + sg_extra;
        for (l_idx, layer) in layers.iter().enumerate() {
            let lw = layer_widths[l_idx];
            let lh = layer_heights[l_idx];
            let mut x = (total_w - lw) * 0.5;
            for &u in layer {
                let sz = g.nodes[u].size;
                g.nodes[u].pos = pos2(x + sz.x * 0.5, y + lh * 0.5);
                x += sz.x + node_gap;
            }
            y += lh + layer_gap;
        }
        let total_h = (y - layer_gap + pad + sg_extra).max(80.0);
        (total_w, total_h)
    } else {
        let layer_gap = if has_labels { 84.0 } else { 60.0 } + sg_extra * 0.5;
        let node_gap = 24.0 + sg_extra * 0.4;
        let mut max_h: f32 = 80.0;
        let mut layer_widths = Vec::with_capacity(layers.len());
        let mut layer_heights = Vec::with_capacity(layers.len());
        for layer in &layers {
            let w: f32 = layer.iter().map(|&i| g.nodes[i].size.x).fold(0.0, f32::max);
            let h: f32 = layer.iter().map(|&i| g.nodes[i].size.y).sum::<f32>()
                + node_gap * (layer.len().saturating_sub(1) as f32);
            max_h = max_h.max(h);
            layer_widths.push(w);
            layer_heights.push(h);
        }
        let total_h = (max_h + pad * 2.0 + sg_extra * 2.0).max(90.0);
        let mut x = pad + sg_extra;
        for (l_idx, layer) in layers.iter().enumerate() {
            let lw = layer_widths[l_idx];
            let lh = layer_heights[l_idx];
            let mut y = (total_h - lh) * 0.5;
            for &u in layer {
                let sz = g.nodes[u].size;
                g.nodes[u].pos = pos2(x + lw * 0.5, y + sz.y * 0.5);
                y += sz.y + node_gap;
            }
            x += lw + layer_gap;
        }
        let total_w = (x - layer_gap + pad + sg_extra).max(200.0);
        (total_w, total_h)
    };
    Some((g, total_w, total_h))
}

fn render_flowchart(
    ui: &mut egui::Ui,
    p: &Pal,
    code: &str,
    body_size: f32,
    avail_width: f32,
) -> bool {
    const MAX_CACHE: usize = 16;
    static CACHE: Mutex<Vec<FlowchartCache>> = Mutex::new(Vec::new());
    let font_size = (body_size * 0.84).clamp(12.0, 14.5);
    let font_id = FontId::proportional(font_size);
    let body_bits = body_size.to_bits();
    let width_bits = avail_width.to_bits();
    let dark = p.bg.r() < 128;
    let hash = code_hash(code);
    let cached = {
        let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        guard
            .iter()
            .position(|c| {
                c.hash == hash
                    && c.body_bits == body_bits
                    && c.width_bits == width_bits
                    && c.dark == dark
                    && c.code == code
            })
            .map(|pos| {
                let hit = guard.remove(pos);
                let out = (Arc::clone(&hit.graph), hit.total_w, hit.total_h);
                guard.push(hit);
                out
            })
    };
    let (g, total_w, total_h) = match cached {
        Some(hit) => hit,
        None => {
            let Some((graph, total_w, total_h)) =
                layout_flowchart_graph(ui, p, code, &font_id, avail_width)
            else {
                return false;
            };
            let graph = Arc::new(graph);
            let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
            if guard.len() >= MAX_CACHE {
                guard.remove(0);
            }
            guard.push(FlowchartCache {
                hash,
                code: code.to_string(),
                body_bits,
                width_bits,
                dark,
                graph: Arc::clone(&graph),
                total_w,
                total_h,
            });
            (graph, total_w, total_h)
        }
    };
    let is_horizontal = matches!(g.dir, Dir::LR | Dir::RL);

    // 5. 캔버스 영역 할당 및 페인팅 (가로로 넓으면 중앙 정렬 또는 가로 스크롤)
    egui::ScrollArea::horizontal()
        .id_salt(("mermaid-scroll", hash))
        .show(ui, |ui| {
            let alloc_w = total_w.max(ui.available_width());
            let (canvas_rect, _) = ui.allocate_exact_size(vec2(alloc_w, total_h), Sense::hover());
            let offset = vec2(
                canvas_rect.left() + ((alloc_w - total_w) * 0.5).max(0.0),
                canvas_rect.top(),
            );
            let painter = ui.painter_at(canvas_rect);

            // (a) 서브그래프 배경 상자 렌더링
            for (sg_i, sg) in g.subgraphs.iter().enumerate() {
                if sg.nodes.is_empty() {
                    continue;
                }
                let mut min_p = pos2(f32::INFINITY, f32::INFINITY);
                let mut max_p = pos2(f32::NEG_INFINITY, f32::NEG_INFINITY);
                for &ni in &sg.nodes {
                    let nd = &g.nodes[ni];
                    let r = Rect::from_center_size(nd.pos + offset, nd.size);
                    min_p.x = min_p.x.min(r.left());
                    min_p.y = min_p.y.min(r.top());
                    max_p.x = max_p.x.max(r.right());
                    max_p.y = max_p.y.max(r.bottom());
                }
                let sg_rect =
                    Rect::from_min_max(min_p - vec2(16.0, 28.0), max_p + vec2(16.0, 14.0));
                let [cr, cg, cb] = PALETTE[sg_i % PALETTE.len()];
                let tint = Color32::from_rgb(cr, cg, cb);
                painter.rect(
                    sg_rect,
                    8.0,
                    tint.gamma_multiply(0.06),
                    Stroke::new(1.2, tint.gamma_multiply(0.35)),
                    egui::StrokeKind::Inside,
                );
                if !sg.title.is_empty() {
                    painter.text(
                        pos2(sg_rect.left() + 10.0, sg_rect.top() + 12.0),
                        Align2::LEFT_CENTER,
                        &sg.title,
                        FontId::new(11.5, theme::bold()),
                        tint,
                    );
                }
            }

            // (b) 간선(Edges) 및 화살표·라벨 렌더링
            for (e_idx, e) in g.edges.iter().enumerate() {
                let stroke_w = match e.style {
                    LineStyle::Thick => 2.6,
                    _ => 1.7,
                };
                let edge_color = p.muted;
                let stroke = Stroke::new(stroke_w, edge_color);

                if e.from == e.to {
                    // 자기 참조 루프 (Self-Loop)
                    let nd = &g.nodes[e.from];
                    let center = nd.pos + offset;
                    let right = center.x + nd.size.x * 0.5;
                    let p0 = pos2(right, center.y - 6.0);
                    let p1 = pos2(right + 34.0, center.y - 22.0);
                    let p2 = pos2(right + 34.0, center.y + 22.0);
                    let p3 = pos2(right, center.y + 6.0);
                    draw_cubic(
                        &painter,
                        [p0, p1, p2, p3],
                        stroke,
                        e.style == LineStyle::Dotted,
                    );
                    if e.arrow_end {
                        draw_arrow(&painter, p3, (p3 - p2).normalized(), edge_color);
                    }
                    if !e.label.is_empty() {
                        draw_edge_label(&painter, p, pos2(right + 36.0, center.y), &e.label);
                    }
                    continue;
                }

                let nu = &g.nodes[e.from];
                let nv = &g.nodes[e.to];
                let cu = nu.pos + offset;
                let cv = nv.pos + offset;

                // 양방향으로 별도 간선이 있거나 중복 간선이면 곡선 제어점을 옆으로 벌린다
                let has_reverse = g
                    .edges
                    .iter()
                    .enumerate()
                    .any(|(j, o)| j != e_idx && o.from == e.to && o.to == e.from);
                let curve_shift = if has_reverse { 18.0 } else { 0.0 };
                let dir_vec = (cv - cu).normalized();
                let perp = vec2(-dir_vec.y, dir_vec.x) * curve_shift;

                let start_pt = border_point(cu, nu.size, nu.shape, (cv + perp) - cu);
                let end_pt = border_point(cv, nv.size, nv.shape, (cu + perp) - cv);

                let dist = start_pt.distance(end_pt);
                let (c1, c2) = if curve_shift.abs() > 0.1 {
                    let mid = start_pt.lerp(end_pt, 0.5) + perp;
                    (start_pt.lerp(mid, 0.65), end_pt.lerp(mid, 0.65))
                } else if !is_horizontal && (start_pt.x - end_pt.x).abs() > 4.0 {
                    let dy = ((end_pt.y - start_pt.y) * 0.45).clamp(-60.0, 60.0);
                    (
                        pos2(start_pt.x, start_pt.y + dy),
                        pos2(end_pt.x, end_pt.y - dy),
                    )
                } else if is_horizontal && (start_pt.y - end_pt.y).abs() > 4.0 {
                    let dx = ((end_pt.x - start_pt.x) * 0.45).clamp(-60.0, 60.0);
                    (
                        pos2(start_pt.x + dx, start_pt.y),
                        pos2(end_pt.x - dx, end_pt.y),
                    )
                } else {
                    let d = (end_pt - start_pt) * 0.33;
                    (start_pt + d, end_pt - d)
                };

                let tip_back = if e.arrow_end { 4.0 } else { 0.0 };
                let start_back = if e.arrow_start { 4.0 } else { 0.0 };
                let end_tangent = (end_pt - c2).normalized();
                let start_tangent = (start_pt - c1).normalized();
                let line_start = if dist > 14.0 {
                    start_pt - start_tangent * start_back
                } else {
                    start_pt
                };
                let line_end = if dist > 14.0 {
                    end_pt - end_tangent * tip_back
                } else {
                    end_pt
                };

                draw_cubic(
                    &painter,
                    [line_start, c1, c2, line_end],
                    stroke,
                    e.style == LineStyle::Dotted,
                );
                if e.arrow_end {
                    draw_arrow(&painter, end_pt, end_tangent, edge_color);
                }
                if e.arrow_start {
                    draw_arrow(&painter, start_pt, start_tangent, edge_color);
                }
                if !e.label.is_empty() {
                    let mid = cubic_eval([start_pt, c1, c2, end_pt], 0.5);
                    draw_edge_label(&painter, p, mid, &e.label);
                }
            }

            // (c) 노드(Nodes) 렌더링
            for (idx, nd) in g.nodes.iter().enumerate() {
                let center = nd.pos + offset;
                let rect = Rect::from_center_size(center, nd.size);
                let [cr, cg, cb] = PALETTE[idx % PALETTE.len()];
                let accent = Color32::from_rgb(cr, cg, cb);
                let fill = nd.fill.unwrap_or_else(|| accent.gamma_multiply(0.13));
                let border_c = nd.stroke.unwrap_or_else(|| accent.gamma_multiply(0.75));
                let stroke = Stroke::new(1.5, border_c);

                match nd.shape {
                    NodeShape::StartState => {
                        painter.circle_filled(center, 9.0, p.accent);
                        continue;
                    }
                    NodeShape::EndState => {
                        painter.circle_stroke(center, 10.0, Stroke::new(1.8, p.accent));
                        painter.circle_filled(center, 5.5, p.accent);
                        continue;
                    }
                    NodeShape::Rect => {
                        painter.rect_filled(rect, 7.0, p.elevated);
                        painter.rect(rect, 7.0, fill, stroke, egui::StrokeKind::Inside);
                    }
                    NodeShape::Round => {
                        painter.rect_filled(rect, 13.0, p.elevated);
                        painter.rect(rect, 13.0, fill, stroke, egui::StrokeKind::Inside);
                    }
                    NodeShape::Stadium => {
                        let r = rect.height() * 0.5;
                        painter.rect_filled(rect, r, p.elevated);
                        painter.rect(rect, r, fill, stroke, egui::StrokeKind::Inside);
                    }
                    NodeShape::Subroutine => {
                        painter.rect_filled(rect, 5.0, p.elevated);
                        painter.rect(rect, 5.0, fill, stroke, egui::StrokeKind::Inside);
                        painter.vline(rect.left() + 7.0, rect.y_range(), stroke);
                        painter.vline(rect.right() - 7.0, rect.y_range(), stroke);
                    }
                    NodeShape::Cylinder => {
                        painter.rect_filled(rect, 8.0, p.elevated);
                        painter.rect(rect, 8.0, fill, stroke, egui::StrokeKind::Inside);
                        painter.hline(
                            rect.left() + 4.0..=rect.right() - 4.0,
                            rect.top() + 7.0,
                            Stroke::new(1.0, border_c.gamma_multiply(0.6)),
                        );
                    }
                    NodeShape::Circle => {
                        let rad = rect.width() * 0.5;
                        painter.circle_filled(center, rad, p.elevated);
                        painter.circle(center, rad, fill, stroke);
                    }
                    NodeShape::DoubleCircle => {
                        let rad = rect.width() * 0.5;
                        painter.circle_filled(center, rad, p.elevated);
                        painter.circle(center, rad, fill, stroke);
                        painter.circle_stroke(center, (rad - 4.0).max(4.0), stroke);
                    }
                    NodeShape::Diamond => {
                        let pts = vec![
                            pos2(center.x, rect.top()),
                            pos2(rect.right(), center.y),
                            pos2(center.x, rect.bottom()),
                            pos2(rect.left(), center.y),
                        ];
                        painter.add(Shape::convex_polygon(pts.clone(), p.elevated, Stroke::NONE));
                        painter.add(Shape::convex_polygon(pts, fill, stroke));
                    }
                    NodeShape::Hexagon => {
                        let inset = 12.0_f32.min(rect.width() * 0.22);
                        let pts = vec![
                            pos2(rect.left() + inset, rect.top()),
                            pos2(rect.right() - inset, rect.top()),
                            pos2(rect.right(), center.y),
                            pos2(rect.right() - inset, rect.bottom()),
                            pos2(rect.left() + inset, rect.bottom()),
                            pos2(rect.left(), center.y),
                        ];
                        painter.add(Shape::convex_polygon(pts.clone(), p.elevated, Stroke::NONE));
                        painter.add(Shape::convex_polygon(pts, fill, stroke));
                    }
                    NodeShape::Parallelogram => {
                        let sk = 10.0_f32.min(rect.width() * 0.18);
                        let pts = vec![
                            pos2(rect.left() + sk, rect.top()),
                            pos2(rect.right(), rect.top()),
                            pos2(rect.right() - sk, rect.bottom()),
                            pos2(rect.left(), rect.bottom()),
                        ];
                        painter.add(Shape::convex_polygon(pts.clone(), p.elevated, Stroke::NONE));
                        painter.add(Shape::convex_polygon(pts, fill, stroke));
                    }
                    NodeShape::Asymmetric => {
                        let cut = 12.0_f32.min(rect.width() * 0.22);
                        let pts = vec![
                            pos2(rect.left(), rect.top()),
                            pos2(rect.right() - cut, rect.top()),
                            pos2(rect.right(), center.y),
                            pos2(rect.right() - cut, rect.bottom()),
                            pos2(rect.left(), rect.bottom()),
                        ];
                        painter.add(Shape::convex_polygon(pts.clone(), p.elevated, Stroke::NONE));
                        painter.add(Shape::convex_polygon(pts, fill, stroke));
                    }
                }

                let galley = ui
                    .ctx()
                    .fonts_mut(|f| f.layout_no_wrap(nd.label.clone(), font_id.clone(), p.text));
                painter.galley(center - galley.size() * 0.5, galley, p.text);
            }
        });
    true
}

fn barycenter(node: usize, neighbor_layer: &[usize], edges: &[Edge]) -> f32 {
    let mut sum = 0.0;
    let mut count = 0usize;
    for (pos, &other) in neighbor_layer.iter().enumerate() {
        let connected = edges
            .iter()
            .any(|e| (e.from == node && e.to == other) || (e.to == node && e.from == other));
        if connected {
            sum += pos as f32;
            count += 1;
        }
    }
    if count == 0 {
        f32::MAX
    } else {
        sum / (count as f32)
    }
}

fn border_point(center: Pos2, size: Vec2, shape: NodeShape, dir: Vec2) -> Pos2 {
    if dir.length_sq() < 1e-4 {
        return center;
    }
    let d = dir.normalized();
    let hw = size.x * 0.5;
    let hh = size.y * 0.5;
    match shape {
        NodeShape::StartState | NodeShape::EndState => center + d * 10.0,
        NodeShape::Circle | NodeShape::DoubleCircle => center + d * hw,
        NodeShape::Diamond => {
            // |x|/hw + |y|/hh = 1
            let denom = (d.x.abs() / hw.max(1.0)) + (d.y.abs() / hh.max(1.0));
            if denom > 1e-5 {
                center + d / denom
            } else {
                center
            }
        }
        _ => {
            let tx = if d.x.abs() > 1e-5 {
                hw / d.x.abs()
            } else {
                f32::INFINITY
            };
            let ty = if d.y.abs() > 1e-5 {
                hh / d.y.abs()
            } else {
                f32::INFINITY
            };
            center + d * tx.min(ty)
        }
    }
}

fn cubic_eval(pts: [Pos2; 4], t: f32) -> Pos2 {
    let u = 1.0 - t;
    let p = pts[0].to_vec2() * (u * u * u)
        + pts[1].to_vec2() * (3.0 * u * u * t)
        + pts[2].to_vec2() * (3.0 * u * t * t)
        + pts[3].to_vec2() * (t * t * t);
    p.to_pos2()
}

fn draw_cubic(painter: &egui::Painter, pts: [Pos2; 4], stroke: Stroke, dotted: bool) {
    if !dotted {
        painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
            pts,
            false,
            Color32::TRANSPARENT,
            stroke,
        ));
    } else {
        let steps = 22usize;
        for i in (0..steps).step_by(2) {
            let t0 = i as f32 / steps as f32;
            let t1 = (i + 1) as f32 / steps as f32;
            painter.line_segment([cubic_eval(pts, t0), cubic_eval(pts, t1)], stroke);
        }
    }
}

fn draw_arrow(painter: &egui::Painter, tip: Pos2, dir: Vec2, color: Color32) {
    let d = if dir.length_sq() > 1e-4 {
        dir.normalized()
    } else {
        vec2(0.0, 1.0)
    };
    let perp = vec2(-d.y, d.x);
    let len = 9.0;
    let half_w = 4.4;
    let base = tip - d * len;
    let pts = vec![tip, base + perp * half_w, base - perp * half_w];
    painter.add(Shape::convex_polygon(pts, color, Stroke::NONE));
}

fn draw_edge_label(painter: &egui::Painter, p: &Pal, center: Pos2, text: &str) {
    let font = FontId::proportional(11.5);
    let galley = painter.layout_no_wrap(text.to_string(), font, p.text);
    let rect = Rect::from_center_size(center, galley.size() + vec2(12.0, 6.0));
    painter.rect(
        rect,
        5.0,
        p.surface,
        Stroke::new(1.0, p.border_hover),
        egui::StrokeKind::Inside,
    );
    painter.galley(center - galley.size() * 0.5, galley, p.text);
}

// ============================================================================
// 2. Sequence Diagram (sequenceDiagram)
// ============================================================================

struct Participant {
    label: String,
}

enum SeqEvent {
    Msg {
        from: usize,
        to: usize,
        text: String,
        dashed: bool,
    },
    Note {
        p1: usize,
        p2: usize,
        text: String,
    },
}

struct ParsedSequence {
    parts: Vec<Participant>,
    events: Vec<SeqEvent>,
}

struct SequenceCache {
    hash: u64,
    code: String,
    parsed: Arc<ParsedSequence>,
}

fn parse_sequence(code: &str) -> ParsedSequence {
    let mut parts: Vec<Participant> = Vec::new();
    let mut part_map: HashMap<String, usize> = HashMap::new();
    let mut events: Vec<SeqEvent> = Vec::new();

    let ensure_p = |id: &str,
                    label: Option<&str>,
                    parts: &mut Vec<Participant>,
                    map: &mut HashMap<String, usize>|
     -> usize {
        if let Some(&i) = map.get(id) {
            if let Some(l) = label {
                parts[i].label = l.to_string();
            }
            i
        } else {
            let i = parts.len();
            parts.push(Participant {
                label: label.unwrap_or(id).to_string(),
            });
            map.insert(id.to_string(), i);
            i
        }
    };

    for line in code.lines().skip(1) {
        let t = line.trim();
        if t.is_empty() || t.starts_with("%%") {
            continue;
        }
        let lower = t.to_ascii_lowercase();
        if lower.starts_with("participant ") || lower.starts_with("actor ") {
            let rest = t.split_once(char::is_whitespace).map_or("", |x| x.1).trim();
            if let Some((id, alias)) = rest.split_once(" as ") {
                ensure_p(
                    id.trim(),
                    Some(&clean_label(alias)),
                    &mut parts,
                    &mut part_map,
                );
            } else {
                let id = clean_label(rest);
                ensure_p(&id, None, &mut parts, &mut part_map);
            }
            continue;
        }
        if lower.starts_with("note ") {
            if let Some((head, body)) = t[5..].split_once(':') {
                let head_l = head.to_ascii_lowercase();
                let target_str = head_l
                    .strip_prefix("over ")
                    .or_else(|| head_l.strip_prefix("right of "))
                    .or_else(|| head_l.strip_prefix("left of "))
                    .unwrap_or(head.trim());
                let offset_idx = head.len() - target_str.len();
                let raw_targets = &head[offset_idx..];
                let mut ids = raw_targets.split(',').map(str::trim);
                if let Some(first_id) = ids.next().filter(|s| !s.is_empty()) {
                    let p1 = ensure_p(first_id, None, &mut parts, &mut part_map);
                    let p2 = ids
                        .next()
                        .map(|s| ensure_p(s, None, &mut parts, &mut part_map))
                        .unwrap_or(p1);
                    events.push(SeqEvent::Note {
                        p1: p1.min(p2),
                        p2: p1.max(p2),
                        text: clean_label(body),
                    });
                }
            }
            continue;
        }

        // 메시지 화살표: ->>, -->>, ->, -->, -x, --x, -)
        for &(op, dashed) in &[
            ("-->>", true),
            ("->>", false),
            ("-->", true),
            ("->", false),
            ("--x", true),
            ("-x", false),
            ("--)", true),
            ("-)", false),
        ] {
            if let Some((left, right)) = t.split_once(op) {
                let from_id = left.trim();
                let (to_id, msg_text) = match right.split_once(':') {
                    Some((to_s, m)) => (to_s.trim().trim_start_matches(['+', '-']), clean_label(m)),
                    None => (right.trim(), String::new()),
                };
                if !from_id.is_empty() && !to_id.is_empty() {
                    let u = ensure_p(from_id, None, &mut parts, &mut part_map);
                    let v = ensure_p(to_id, None, &mut parts, &mut part_map);
                    events.push(SeqEvent::Msg {
                        from: u,
                        to: v,
                        text: msg_text,
                        dashed,
                    });
                    break;
                }
            }
        }
    }

    ParsedSequence { parts, events }
}

fn render_sequence(
    ui: &mut egui::Ui,
    p: &Pal,
    code: &str,
    body_size: f32,
    avail_width: f32,
) -> bool {
    const MAX_CACHE: usize = 16;
    static SEQ_CACHE: Mutex<Vec<SequenceCache>> = Mutex::new(Vec::new());
    let hash = code_hash(code);
    let parsed = {
        let mut guard = SEQ_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(pos) = guard.iter().position(|c| c.hash == hash && c.code == code) {
            let hit = guard.remove(pos);
            let out = Arc::clone(&hit.parsed);
            guard.push(hit);
            out
        } else {
            drop(guard);
            let parsed = Arc::new(parse_sequence(code));
            let mut guard = SEQ_CACHE.lock().unwrap_or_else(|e| e.into_inner());
            if guard.len() >= MAX_CACHE {
                guard.remove(0);
            }
            guard.push(SequenceCache {
                hash,
                code: code.to_string(),
                parsed: Arc::clone(&parsed),
            });
            parsed
        }
    };
    let parts = &parsed.parts;
    let events = &parsed.events;

    if parts.is_empty() {
        return false;
    }

    let col_gap = 150.0_f32;
    let pad_x = 40.0;
    let header_h = 36.0;
    let row_h = 46.0;
    let total_w = (pad_x * 2.0 + (parts.len().saturating_sub(1) as f32) * col_gap + 120.0)
        .max(avail_width.min(360.0));
    let total_h = header_h * 2.0 + 36.0 + (events.len() as f32) * row_h + 24.0;

    egui::ScrollArea::horizontal()
        .id_salt(("mermaid-seq", hash))
        .show(ui, |ui| {
            let alloc_w = total_w.max(ui.available_width());
            let (canvas_rect, _) = ui.allocate_exact_size(vec2(alloc_w, total_h), Sense::hover());
            let start_x = canvas_rect.left()
                + ((alloc_w - ((parts.len().saturating_sub(1) as f32) * col_gap)) * 0.5).max(60.0);
            let top_y = canvas_rect.top() + 14.0;
            let bot_y = canvas_rect.bottom() - 14.0 - header_h;
            let painter = ui.painter_at(canvas_rect);

            let xs: Vec<f32> = (0..parts.len())
                .map(|i| start_x + (i as f32) * col_gap)
                .collect();

            // 생명선(Lifelines) 및 상·하단 참가자 박스
            for (i, part) in parts.iter().enumerate() {
                let cx = xs[i];
                let [cr, cg, cb] = PALETTE[i % PALETTE.len()];
                let accent = Color32::from_rgb(cr, cg, cb);
                draw_cubic(
                    &painter,
                    [
                        pos2(cx, top_y + header_h),
                        pos2(cx, (top_y + bot_y) * 0.5),
                        pos2(cx, (top_y + bot_y) * 0.5),
                        pos2(cx, bot_y),
                    ],
                    Stroke::new(1.2, p.border_hover),
                    true,
                );
                for by in [top_y, bot_y] {
                    let r = Rect::from_center_size(
                        pos2(cx, by + header_h * 0.5),
                        vec2(112.0, header_h),
                    );
                    painter.rect_filled(r, 7.0, p.elevated);
                    painter.rect(
                        r,
                        7.0,
                        accent.gamma_multiply(0.14),
                        Stroke::new(1.5, accent.gamma_multiply(0.75)),
                        egui::StrokeKind::Inside,
                    );
                    painter.text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        &part.label,
                        FontId::new(body_size * 0.82, theme::bold()),
                        p.text,
                    );
                }
            }

            // 메시지 및 노트 렌더링
            let mut y = top_y + header_h + 28.0;
            for ev in events {
                match ev {
                    SeqEvent::Msg {
                        from,
                        to,
                        text,
                        dashed,
                    } => {
                        let x1 = xs[*from];
                        let x2 = xs[*to];
                        let stroke = Stroke::new(1.7, p.text);
                        if from == to {
                            let p0 = pos2(x1, y - 6.0);
                            let p1 = pos2(x1 + 38.0, y - 6.0);
                            let p2 = pos2(x1 + 38.0, y + 12.0);
                            let p3 = pos2(x1, y + 12.0);
                            painter.line_segment([p0, p1], stroke);
                            painter.line_segment([p1, p2], stroke);
                            painter.line_segment([p2, p3], stroke);
                            draw_arrow(&painter, p3, vec2(-1.0, 0.0), p.text);
                            if !text.is_empty() {
                                painter.text(
                                    pos2(x1 + 44.0, y + 3.0),
                                    Align2::LEFT_CENTER,
                                    text,
                                    FontId::proportional(12.0),
                                    p.text,
                                );
                            }
                        } else {
                            let dir = if x2 > x1 { 1.0 } else { -1.0 };
                            let p_start = pos2(x1, y);
                            let p_end = pos2(x2 - dir * 4.0, y);
                            draw_cubic(
                                &painter,
                                [
                                    p_start,
                                    p_start.lerp(p_end, 0.33),
                                    p_start.lerp(p_end, 0.66),
                                    p_end,
                                ],
                                stroke,
                                *dashed,
                            );
                            draw_arrow(&painter, pos2(x2, y), vec2(dir, 0.0), p.text);
                            if !text.is_empty() {
                                painter.text(
                                    pos2((x1 + x2) * 0.5, y - 10.0),
                                    Align2::CENTER_CENTER,
                                    text,
                                    FontId::proportional(12.0),
                                    p.text,
                                );
                            }
                        }
                    }
                    SeqEvent::Note { p1, p2, text } => {
                        let x_mid = (xs[*p1] + xs[*p2]) * 0.5;
                        let w = ((xs[*p2] - xs[*p1]).abs() + 96.0).max(120.0);
                        let r = Rect::from_center_size(pos2(x_mid, y), vec2(w, 28.0));
                        let amber = Color32::from_rgb(0xf5, 0x9e, 0x0b);
                        painter.rect_filled(r, 5.0, p.elevated);
                        painter.rect(
                            r,
                            5.0,
                            amber.gamma_multiply(0.16),
                            Stroke::new(1.0, amber.gamma_multiply(0.65)),
                            egui::StrokeKind::Inside,
                        );
                        painter.text(
                            r.center(),
                            Align2::CENTER_CENTER,
                            text,
                            FontId::proportional(11.5),
                            p.text,
                        );
                    }
                }
                y += row_h;
            }
        });
    true
}

// ============================================================================
// 3. Pie Chart (pie)
// ============================================================================

struct ParsedPie {
    title: String,
    slices: Vec<(String, f32)>,
    total: f32,
}

struct PieCache {
    hash: u64,
    code: String,
    parsed: Arc<ParsedPie>,
}

fn parse_pie(code: &str) -> ParsedPie {
    let mut title = String::new();
    let mut slices: Vec<(String, f32)> = Vec::new();

    for line in code.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("%%") {
            continue;
        }
        let lower = t.to_ascii_lowercase();
        if lower.starts_with("pie") {
            let orig_rest = t[3..].trim();
            let orig_lower = orig_rest.to_ascii_lowercase();
            if let Some(idx) = orig_lower.find("title ") {
                let raw_t = orig_rest.get(idx + 6..).unwrap_or("").trim();
                title = clean_label(raw_t);
            }
            continue;
        }
        if lower.starts_with("title ") {
            title = clean_label(&t[6..]);
            continue;
        }
        if let Some((lbl, val_str)) = t.rsplit_once(':') {
            if let Ok(v) = val_str.trim().parse::<f32>() {
                if v > 0.0 {
                    slices.push((clean_label(lbl), v));
                }
            }
        }
    }

    let total: f32 = slices.iter().map(|(_, v)| *v).sum();
    ParsedPie {
        title,
        slices,
        total,
    }
}

fn render_pie(ui: &mut egui::Ui, p: &Pal, code: &str, body_size: f32, _avail_width: f32) -> bool {
    const MAX_CACHE: usize = 16;
    static PIE_CACHE: Mutex<Vec<PieCache>> = Mutex::new(Vec::new());
    let hash = code_hash(code);
    let parsed = {
        let mut guard = PIE_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(pos) = guard.iter().position(|c| c.hash == hash && c.code == code) {
            let hit = guard.remove(pos);
            let out = Arc::clone(&hit.parsed);
            guard.push(hit);
            out
        } else {
            drop(guard);
            let parsed = Arc::new(parse_pie(code));
            let mut guard = PIE_CACHE.lock().unwrap_or_else(|e| e.into_inner());
            if guard.len() >= MAX_CACHE {
                guard.remove(0);
            }
            guard.push(PieCache {
                hash,
                code: code.to_string(),
                parsed: Arc::clone(&parsed),
            });
            parsed
        }
    };
    let title = &parsed.title;
    let slices = &parsed.slices;
    let total = parsed.total;

    if slices.is_empty() || total <= 0.0 {
        return false;
    }

    let radius = 76.0_f32;
    let top_title_h = if title.is_empty() { 0.0 } else { 28.0 };
    let legend_h = (slices.len() as f32) * 22.0;
    let body_h = (radius * 2.0).max(legend_h);
    let total_h = top_title_h + body_h + 28.0;
    let legend_font = FontId::proportional(body_size * 0.8);
    let max_legend_w = ui.ctx().fonts_mut(|f| {
        slices
            .iter()
            .map(|(lbl, val)| {
                let pct = (*val / total) * 100.0;
                f.layout_no_wrap(format!("{lbl} ({pct:.1}%)"), legend_font.clone(), p.text)
                    .size()
                    .x
            })
            .fold(80.0_f32, f32::max)
    });
    let content_w = (radius * 2.0 + 32.0 + 18.0 + max_legend_w + 32.0).max(320.0);

    egui::ScrollArea::horizontal()
        .id_salt(("mermaid-pie", hash))
        .show(ui, |ui| {
            let alloc_w = content_w.max(ui.available_width());
            let (rect, _) = ui.allocate_exact_size(vec2(alloc_w, total_h), Sense::hover());
            let painter = ui.painter_at(rect);

            if !title.is_empty() {
                painter.text(
                    pos2(rect.center().x, rect.top() + 14.0),
                    Align2::CENTER_CENTER,
                    title,
                    FontId::new(body_size * 0.9, theme::bold()),
                    p.text,
                );
            }

            let start_x = rect.left() + ((alloc_w - content_w) * 0.5).max(16.0);
            let pie_center = pos2(
                start_x + radius,
                rect.top() + top_title_h + body_h * 0.5 + 12.0,
            );
            let mut angle = -std::f32::consts::FRAC_PI_2;
            for (i, (_, val)) in slices.iter().enumerate() {
                let frac = *val / total;
                let sweep = frac * std::f32::consts::TAU;
                let steps = ((sweep * 20.0).ceil() as usize).max(6);
                let [cr, cg, cb] = PALETTE[i % PALETTE.len()];
                let color = Color32::from_rgb(cr, cg, cb);
                for s in 0..steps {
                    let a0 = angle + sweep * (s as f32 / steps as f32);
                    let a1 = angle + sweep * ((s + 1) as f32 / steps as f32);
                    let p1 = pie_center + vec2(a0.cos(), a0.sin()) * radius;
                    let p2 = pie_center + vec2(a1.cos(), a1.sin()) * radius;
                    painter.add(Shape::convex_polygon(
                        vec![pie_center, p1, p2],
                        color,
                        Stroke::new(0.5, color),
                    ));
                }
                angle += sweep;
            }
            painter.circle_filled(pie_center, radius * 0.42, p.surface);

            // 오른쪽 범례(Legend)
            let legend_x = pie_center.x + radius + 32.0;
            let mut legend_y = pie_center.y - legend_h * 0.5 + 10.0;
            for (i, (lbl, val)) in slices.iter().enumerate() {
                let [cr, cg, cb] = PALETTE[i % PALETTE.len()];
                let color = Color32::from_rgb(cr, cg, cb);
                let pct = (*val / total) * 100.0;
                let dot_r = Rect::from_center_size(pos2(legend_x, legend_y), vec2(11.0, 11.0));
                painter.rect_filled(dot_r, 3.0, color);
                painter.text(
                    pos2(legend_x + 12.0, legend_y),
                    Align2::LEFT_CENTER,
                    format!("{lbl} ({pct:.1}%)"),
                    legend_font.clone(),
                    p.text,
                );
                legend_y += 22.0;
            }
        });
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flowchart_nodes_shapes_edges_and_subgraphs() {
        let src = r#"
graph LR
  subgraph SG["인증 흐름"]
    A["사용자 (User)"] -->|로그인 요청| B{토큰 유효?}
  end
  B -- 예 --> C([대시보드])
  B -. 아니오 .-> D((오류))
  style C fill:#10b981,stroke:#059669
"#;
        let g = parse_graph(src);
        assert_eq!(g.dir, Dir::LR);
        assert_eq!(g.nodes.len(), 4);
        assert_eq!(g.edges.len(), 3);
        assert_eq!(g.subgraphs.len(), 1);
        assert_eq!(g.subgraphs[0].title, "인증 흐름");
        assert_eq!(g.nodes[0].label, "사용자 (User)");
        assert!(matches!(g.nodes[1].shape, NodeShape::Diamond));
        assert!(matches!(g.nodes[2].shape, NodeShape::Stadium));
        assert!(matches!(g.nodes[3].shape, NodeShape::Circle));
        assert_eq!(g.edges[0].label, "로그인 요청");
        assert_eq!(g.edges[1].label, "예");
        assert_eq!(g.edges[2].label, "아니오");
        assert!(matches!(g.edges[2].style, LineStyle::Dotted));
        assert!(g.nodes[2].fill.is_some());

        // 유니코드 대소문자 길이 변환 문자(İ → i\u{307})가 포함되어도 패닉 없이 파싱
        let seq = parse_sequence(
            "sequenceDiagram\n  participant A\n  Note over İA, İB: İstanbul\n  A->>B: İ",
        );
        assert_eq!(seq.events.len(), 2);
        let pie = parse_pie("pie   title İstanbul İ\n  \"İ\" : 40\n  \"B\" : 60");
        assert_eq!(pie.title, "İstanbul İ");
        assert_eq!(pie.slices.len(), 2);
    }
}
