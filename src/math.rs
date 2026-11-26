// 순수 네이티브 LaTeX 수식 조판 (egui Painter 기반, 외부 의존성 없음).
// 지원: 분수·첨자·루트·그리스 문자·연산자/관계 기호·큰 연산자(∑∫lim)·\left\right·\big·
//       행렬(matrix/pmatrix/bmatrix/vmatrix)·cases·aligned·악센트·\text·\mathbf/\mathbb/\mathcal·간격.
// 글꼴은 theme::math() (Windows: Cambria Math). 변수는 유니코드 수학 이탤릭 글자로 그린다.
// ponytail: TeX 규칙 중 자주 쓰는 것만 근사한다(글자 높이는 em 비율 근사). 모르는 명령은 이름을 글자로 보여 준다.
use std::sync::Arc;

use eframe::egui::{self, pos2, vec2, Color32, FontId, Galley, Pos2, Rect, Stroke};
use egui::epaint::text::FontsView;

use crate::theme::{self, Pal};

#[derive(Clone, Copy, PartialEq, Debug)]
enum Class {
    Ord,
    Op,
    Bin,
    Rel,
    Open,
    Close,
    Punct,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Accent {
    Hat,
    Tilde,
    Dot,
    Ddot,
    Vec,
    Bar,
    Overline,
    Underline,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum GridKind {
    Matrix,
    Cases,
    Aligned,
    Gathered,
}

#[derive(Clone, Debug)]
enum Node {
    Sym {
        text: String,
        class: Class,
        text_font: bool,
    },
    Row(Vec<Node>),
    Scripts {
        base: Box<Node>,
        sup: Option<Box<Node>>,
        sub: Option<Box<Node>>,
    },
    Frac {
        num: Box<Node>,
        den: Box<Node>,
        bar: bool,
    },
    Sqrt {
        index: Option<Box<Node>>,
        body: Box<Node>,
    },
    BigOp {
        text: String,
        limits: bool,
        func: bool,
    },
    Fenced {
        left: String,
        body: Box<Node>,
        right: String,
    },
    Big {
        delim: String,
        scale: f32,
    },
    Accent {
        kind: Accent,
        body: Box<Node>,
    },
    Space(f32), // em
    Grid {
        rows: Vec<Vec<Node>>,
        kind: GridKind,
    },
    Boxed(Box<Node>),
}

impl Node {
    fn class(&self) -> Class {
        match self {
            Node::Sym { class, .. } => *class,
            Node::Scripts { base, .. } => base.class(),
            Node::BigOp { .. } => Class::Op,
            _ => Class::Ord,
        }
    }
}

// ---------- 글자 변환 ----------

fn math_italic(c: char) -> char {
    let off = |base: u32, from: char| char::from_u32(base + (c as u32 - from as u32)).unwrap_or(c);
    match c {
        'h' => 'ℎ',
        'a'..='z' => off(0x1D44E, 'a'),
        'A'..='Z' => off(0x1D434, 'A'),
        'α'..='ω' => off(0x1D6FC, 'α'),
        _ => c,
    }
}

fn styled(c: char, font: &str) -> char {
    let off = |base: u32, from: char| char::from_u32(base + (c as u32 - from as u32)).unwrap_or(c);
    match font {
        "bf" => match c {
            'A'..='Z' => off(0x1D400, 'A'),
            'a'..='z' => off(0x1D41A, 'a'),
            '0'..='9' => off(0x1D7CE, '0'),
            _ => c,
        },
        "bb" => match c {
            'C' => 'ℂ',
            'H' => 'ℍ',
            'N' => 'ℕ',
            'P' => 'ℙ',
            'Q' => 'ℚ',
            'R' => 'ℝ',
            'Z' => 'ℤ',
            'A'..='Z' => off(0x1D538, 'A'),
            '0'..='9' => off(0x1D7D8, '0'),
            _ => c,
        },
        "cal" => match c {
            'B' => 'ℬ',
            'E' => 'ℰ',
            'F' => 'ℱ',
            'H' => 'ℋ',
            'I' => 'ℐ',
            'L' => 'ℒ',
            'M' => 'ℳ',
            'R' => 'ℛ',
            'A'..='Z' => off(0x1D49C, 'A'),
            _ => c,
        },
        "it" => math_italic(c),
        _ => c, // rm
    }
}

fn symbol(name: &str) -> Option<(&'static str, Class)> {
    use Class::*;
    let greek = [
        ("alpha", "α"),
        ("beta", "β"),
        ("gamma", "γ"),
        ("delta", "δ"),
        ("epsilon", "ϵ"),
        ("varepsilon", "ε"),
        ("zeta", "ζ"),
        ("eta", "η"),
        ("theta", "θ"),
        ("vartheta", "ϑ"),
        ("iota", "ι"),
        ("kappa", "κ"),
        ("lambda", "λ"),
        ("mu", "μ"),
        ("nu", "ν"),
        ("xi", "ξ"),
        ("pi", "π"),
        ("varpi", "ϖ"),
        ("rho", "ρ"),
        ("varrho", "ϱ"),
        ("sigma", "σ"),
        ("varsigma", "ς"),
        ("tau", "τ"),
        ("upsilon", "υ"),
        ("phi", "ϕ"),
        ("varphi", "φ"),
        ("chi", "χ"),
        ("psi", "ψ"),
        ("omega", "ω"),
        ("Gamma", "Γ"),
        ("Delta", "Δ"),
        ("Theta", "Θ"),
        ("Lambda", "Λ"),
        ("Xi", "Ξ"),
        ("Pi", "Π"),
        ("Sigma", "Σ"),
        ("Upsilon", "Υ"),
        ("Phi", "Φ"),
        ("Psi", "Ψ"),
        ("Omega", "Ω"),
    ];
    if let Some((_, g)) = greek.iter().find(|(n, _)| *n == name) {
        return Some((g, Ord));
    }
    Some(match name {
        "pm" => ("±", Bin),
        "mp" => ("∓", Bin),
        "times" => ("×", Bin),
        "div" => ("÷", Bin),
        "cdot" => ("⋅", Bin),
        "ast" => ("∗", Bin),
        "star" => ("⋆", Bin),
        "circ" => ("∘", Bin),
        "bullet" => ("∙", Bin),
        "cup" => ("∪", Bin),
        "cap" => ("∩", Bin),
        "wedge" | "land" => ("∧", Bin),
        "vee" | "lor" => ("∨", Bin),
        "oplus" => ("⊕", Bin),
        "otimes" => ("⊗", Bin),
        "setminus" => ("∖", Bin),
        "le" | "leq" => ("≤", Rel),
        "ge" | "geq" => ("≥", Rel),
        "ne" | "neq" => ("≠", Rel),
        "approx" => ("≈", Rel),
        "equiv" => ("≡", Rel),
        "sim" => ("∼", Rel),
        "simeq" => ("≃", Rel),
        "cong" => ("≅", Rel),
        "propto" => ("∝", Rel),
        "ll" => ("≪", Rel),
        "gg" => ("≫", Rel),
        "in" => ("∈", Rel),
        "notin" => ("∉", Rel),
        "ni" => ("∋", Rel),
        "subset" => ("⊂", Rel),
        "supset" => ("⊃", Rel),
        "subseteq" => ("⊆", Rel),
        "supseteq" => ("⊇", Rel),
        "to" | "rightarrow" => ("→", Rel),
        "leftarrow" | "gets" => ("←", Rel),
        "Rightarrow" => ("⇒", Rel),
        "Leftarrow" => ("⇐", Rel),
        "Leftrightarrow" => ("⇔", Rel),
        "leftrightarrow" => ("↔", Rel),
        "iff" => ("⟺", Rel),
        "implies" => ("⟹", Rel),
        "mapsto" => ("↦", Rel),
        "perp" => ("⊥", Rel),
        "parallel" => ("∥", Rel),
        "mid" => ("∣", Rel),
        "coloneqq" => ("≔", Rel),
        "infty" => ("∞", Ord),
        "partial" => ("∂", Ord),
        "nabla" => ("∇", Ord),
        "forall" => ("∀", Ord),
        "exists" => ("∃", Ord),
        "nexists" => ("∄", Ord),
        "emptyset" | "varnothing" => ("∅", Ord),
        "neg" | "lnot" => ("¬", Ord),
        "angle" => ("∠", Ord),
        "triangle" => ("△", Ord),
        "prime" => ("′", Ord),
        "hbar" => ("ℏ", Ord),
        "ell" => ("ℓ", Ord),
        "Re" => ("ℜ", Ord),
        "Im" => ("ℑ", Ord),
        "aleph" => ("ℵ", Ord),
        "degree" => ("°", Ord),
        "cdots" => ("⋯", Ord),
        "ldots" | "dots" => ("…", Ord),
        "vdots" => ("⋮", Ord),
        "ddots" => ("⋱", Ord),
        "top" => ("⊤", Ord),
        "bot" => ("⊥", Ord),
        "langle" => ("⟨", Open),
        "rangle" => ("⟩", Close),
        "lfloor" => ("⌊", Open),
        "rfloor" => ("⌋", Close),
        "lceil" => ("⌈", Open),
        "rceil" => ("⌉", Close),
        "lbrace" => ("{", Open),
        "rbrace" => ("}", Close),
        "vert" => ("|", Ord),
        "Vert" => ("‖", Ord),
        _ => return None,
    })
}

fn big_op(name: &str) -> Option<(&'static str, bool)> {
    Some(match name {
        "sum" => ("∑", true),
        "prod" => ("∏", true),
        "coprod" => ("∐", true),
        "bigcup" => ("⋃", true),
        "bigcap" => ("⋂", true),
        "bigoplus" => ("⨁", true),
        "bigotimes" => ("⨂", true),
        "int" => ("∫", false),
        "iint" => ("∬", false),
        "iiint" => ("∭", false),
        "oint" => ("∮", false),
        _ => return None,
    })
}

fn func(name: &str) -> Option<bool> {
    const SIDE: &[&str] = &[
        "sin", "cos", "tan", "cot", "sec", "csc", "arcsin", "arccos", "arctan", "sinh", "cosh",
        "tanh", "coth", "log", "ln", "lg", "exp", "det", "dim", "ker", "deg", "gcd", "arg", "hom",
        "Pr",
    ];
    const LIMITS: &[&str] = &[
        "lim", "limsup", "liminf", "max", "min", "sup", "inf", "argmax", "argmin",
    ];
    if LIMITS.contains(&name) {
        Some(true)
    } else if SIDE.contains(&name) {
        Some(false)
    } else {
        None
    }
}

// ---------- 파서 ----------

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Cmd(String),
    Ch(char),
}

fn tokenize(s: &str) -> Vec<Tok> {
    let cs: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c == '\\' && i + 1 < cs.len() {
            if cs[i + 1].is_ascii_alphabetic() {
                let mut j = i + 1;
                while j < cs.len() && cs[j].is_ascii_alphabetic() {
                    j += 1;
                }
                out.push(Tok::Cmd(cs[i + 1..j].iter().collect()));
                i = j;
            } else {
                out.push(Tok::Cmd(cs[i + 1].to_string()));
                i += 2;
            }
        } else if c == '%' {
            while i < cs.len() && cs[i] != '\n' {
                i += 1;
            }
        } else {
            out.push(Tok::Ch(c));
            i += 1;
        }
    }
    out
}

struct Parser {
    t: Vec<Tok>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(Tok::Ch(c)) if c.is_whitespace()) {
            self.i += 1;
        }
    }

    // 중괄호 안의 원문 글자 ({...}의 내용, \text 등에서 사용)
    fn raw_group(&mut self) -> String {
        self.skip_ws();
        if self.peek() != Some(&Tok::Ch('{')) {
            return match self.t.get(self.i).cloned() {
                Some(Tok::Ch(c)) => {
                    self.i += 1;
                    c.to_string()
                }
                Some(Tok::Cmd(c)) => {
                    self.i += 1;
                    c
                }
                None => String::new(),
            };
        }
        self.i += 1;
        let mut depth = 1;
        let mut s = String::new();
        while let Some(t) = self.t.get(self.i).cloned() {
            self.i += 1;
            match t {
                Tok::Ch('{') => {
                    depth += 1;
                    s.push('{');
                }
                Tok::Ch('}') => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    s.push('}');
                }
                Tok::Ch(c) => s.push(c),
                Tok::Cmd(c) if c.len() == 1 && !c.chars().all(|c| c.is_ascii_alphabetic()) => {
                    s.push_str(&c);
                }
                Tok::Cmd(c) => {
                    s.push('\\');
                    s.push_str(&c);
                }
            }
        }
        s
    }

    fn opt_bracket(&mut self) -> Option<Node> {
        self.skip_ws();
        if self.peek() != Some(&Tok::Ch('[')) {
            return None;
        }
        self.i += 1;
        Some(self.row(&[Stop::Bracket]))
    }

    // 인자 하나: {묶음} 또는 글자/명령 하나
    fn arg(&mut self) -> Node {
        self.skip_ws();
        match self.peek() {
            Some(Tok::Ch('{')) => {
                self.i += 1;
                self.row(&[Stop::Brace])
            }
            Some(_) => self.atom().unwrap_or(Node::Row(Vec::new())),
            None => Node::Row(Vec::new()),
        }
    }

    fn delim(&mut self) -> String {
        self.skip_ws();
        match self.t.get(self.i).cloned() {
            Some(Tok::Ch(c)) => {
                self.i += 1;
                if c == '.' {
                    String::new()
                } else {
                    c.to_string()
                }
            }
            Some(Tok::Cmd(c)) => {
                self.i += 1;
                match c.as_str() {
                    "{" | "lbrace" => "{".into(),
                    "}" | "rbrace" => "}".into(),
                    "|" | "Vert" => "‖".into(),
                    "vert" => "|".into(),
                    _ => symbol(&c).map_or(String::new(), |s| s.0.to_string()),
                }
            }
            None => String::new(),
        }
    }

    fn row(&mut self, stops: &[Stop]) -> Node {
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            let Some(t) = self.peek().cloned() else {
                break;
            };
            let stop = match &t {
                Tok::Ch('}') => Some(Stop::Brace),
                Tok::Ch(']') if stops.contains(&Stop::Bracket) => Some(Stop::Bracket),
                Tok::Ch('&') => Some(Stop::Amp),
                Tok::Cmd(c) if c == "\\" || c == "cr" => Some(Stop::Newline),
                Tok::Cmd(c) if c == "right" => Some(Stop::Right),
                Tok::Cmd(c) if c == "end" => Some(Stop::End),
                _ => None,
            };
            if let Some(s) = stop {
                if stops.contains(&s) {
                    if matches!(s, Stop::Brace | Stop::Bracket) {
                        self.i += 1;
                    }
                    break;
                }
                if s == Stop::Brace {
                    self.i += 1; // 짝 없는 } 는 버린다
                    continue;
                }
                if matches!(s, Stop::Amp | Stop::Newline) {
                    self.i += 1; // 표 밖의 & · \\ 는 무시 (최상위 \\ 는 parse()에서 처리)
                    continue;
                }
                if s == Stop::Right || s == Stop::End {
                    self.i += 1;
                    if s == Stop::End {
                        let _ = self.raw_group();
                    } else {
                        let _ = self.delim();
                    }
                    continue;
                }
            }
            let Some(mut base) = self.atom() else {
                continue;
            };
            // 첨자
            let (mut sup, mut sub) = (None, None);
            loop {
                self.skip_ws();
                match self.peek() {
                    Some(Tok::Ch('^')) => {
                        self.i += 1;
                        sup = Some(Box::new(self.arg()));
                    }
                    Some(Tok::Ch('_')) => {
                        self.i += 1;
                        sub = Some(Box::new(self.arg()));
                    }
                    Some(Tok::Ch('\'')) => {
                        self.i += 1;
                        sup = Some(Box::new(sym("′", Class::Ord)));
                    }
                    Some(Tok::Cmd(c)) if c == "limits" || c == "nolimits" => {
                        let lim = c == "limits";
                        self.i += 1;
                        if let Node::BigOp { limits, .. } = &mut base {
                            *limits = lim;
                        }
                    }
                    _ => break,
                }
            }
            if sup.is_some() || sub.is_some() {
                base = Node::Scripts {
                    base: Box::new(base),
                    sup,
                    sub,
                };
            }
            items.push(base);
        }
        Node::Row(items)
    }

    fn atom(&mut self) -> Option<Node> {
        let t = self.t.get(self.i).cloned()?;
        self.i += 1;
        Some(match t {
            Tok::Ch('{') => self.row(&[Stop::Brace]),
            Tok::Ch(c) => char_node(c),
            Tok::Cmd(name) => self.command(&name)?,
        })
    }

    fn command(&mut self, name: &str) -> Option<Node> {
        Some(match name {
            "frac" | "dfrac" | "tfrac" | "cfrac" => Node::Frac {
                num: Box::new(self.arg()),
                den: Box::new(self.arg()),
                bar: true,
            },
            "binom" | "dbinom" | "tbinom" => Node::Fenced {
                left: "(".into(),
                body: Box::new(Node::Frac {
                    num: Box::new(self.arg()),
                    den: Box::new(self.arg()),
                    bar: false,
                }),
                right: ")".into(),
            },
            "sqrt" => {
                let index = self.opt_bracket().map(Box::new);
                Node::Sqrt {
                    index,
                    body: Box::new(self.arg()),
                }
            }
            "left" => {
                let left = self.delim();
                let body = self.row(&[Stop::Right]);
                let right = if self.peek() == Some(&Tok::Cmd("right".into())) {
                    self.i += 1;
                    self.delim()
                } else {
                    String::new()
                };
                Node::Fenced {
                    left,
                    body: Box::new(body),
                    right,
                }
            }
            "big" | "bigl" | "bigr" | "bigm" => self.big(1.2),
            "Big" | "Bigl" | "Bigr" | "Bigm" => self.big(1.6),
            "bigg" | "biggl" | "biggr" | "biggm" => self.big(2.1),
            "Bigg" | "Biggl" | "Biggr" | "Biggm" => self.big(2.6),
            "text" | "textrm" | "textnormal" | "mbox" | "textit" | "textbf" => Node::Sym {
                text: self.raw_group(),
                class: Class::Ord,
                text_font: true,
            },
            "mathrm" | "operatorname" | "mathbf" | "mathbb" | "mathcal" | "mathit"
            | "boldsymbol" | "bm" | "mathsf" | "mathtt" => {
                let font = match name {
                    "mathbf" | "boldsymbol" | "bm" => "bf",
                    "mathbb" => "bb",
                    "mathcal" => "cal",
                    "mathit" => "it",
                    _ => "rm",
                };
                let raw = self.raw_group();
                if name == "operatorname" {
                    return Some(Node::BigOp {
                        text: raw,
                        limits: false,
                        func: true,
                    });
                }
                Node::Sym {
                    text: raw.chars().map(|c| styled(c, font)).collect(),
                    class: Class::Ord,
                    text_font: false,
                }
            }
            "hat" | "widehat" => self.accent(Accent::Hat),
            "tilde" | "widetilde" => self.accent(Accent::Tilde),
            "dot" => self.accent(Accent::Dot),
            "ddot" => self.accent(Accent::Ddot),
            "vec" | "overrightarrow" => self.accent(Accent::Vec),
            "bar" => self.accent(Accent::Bar),
            "overline" => self.accent(Accent::Overline),
            "underline" => self.accent(Accent::Underline),
            "boxed" => Node::Boxed(Box::new(self.arg())),
            "begin" => self.env(),
            "," | "thinspace" => Node::Space(3.0 / 18.0),
            ":" | ">" | "medspace" => Node::Space(4.0 / 18.0),
            ";" | "thickspace" => Node::Space(5.0 / 18.0),
            "!" | "negthinspace" => Node::Space(-3.0 / 18.0),
            " " => Node::Space(0.25),
            "quad" => Node::Space(1.0),
            "qquad" => Node::Space(2.0),
            "{" => sym("{", Class::Open),
            "}" => sym("}", Class::Close),
            "|" => sym("‖", Class::Ord),
            "%" | "$" | "&" | "#" | "_" => sym(name, Class::Ord),
            // 꾸밈 명령은 무시 (색상은 인자를 버린다)
            "displaystyle" | "textstyle" | "scriptstyle" | "limits" | "nolimits" | "nonumber"
            | "notag" => return None,
            "color" | "textcolor" | "tag" | "label" => {
                let _ = self.raw_group();
                if name == "textcolor" {
                    return Some(self.arg());
                }
                return None;
            }
            _ => {
                if let Some((t, limits)) = big_op(name) {
                    Node::BigOp {
                        text: t.into(),
                        limits,
                        func: false,
                    }
                } else if let Some(limits) = func(name) {
                    Node::BigOp {
                        text: name.into(),
                        limits,
                        func: true,
                    }
                } else if let Some((t, class)) = symbol(name) {
                    let text = if class == Class::Ord && t.chars().all(|c| ('α'..='ω').contains(&c))
                    {
                        t.chars().map(math_italic).collect()
                    } else {
                        t.to_string()
                    };
                    Node::Sym {
                        text,
                        class,
                        text_font: false,
                    }
                } else {
                    Node::Sym {
                        text: format!("\\{name}"),
                        class: Class::Ord,
                        text_font: true,
                    }
                }
            }
        })
    }

    fn big(&mut self, scale: f32) -> Node {
        Node::Big {
            delim: self.delim(),
            scale,
        }
    }

    fn accent(&mut self, kind: Accent) -> Node {
        Node::Accent {
            kind,
            body: Box::new(self.arg()),
        }
    }

    fn env(&mut self) -> Node {
        let name = self.raw_group();
        let name = name.trim_end_matches('*');
        if name == "array" {
            let _ = self.raw_group(); // 열 정렬 지정은 무시
        }
        let rows = self.grid_rows();
        let kind = match name {
            "cases" | "dcases" => GridKind::Cases,
            "aligned" | "align" | "alignat" | "split" | "eqnarray" => GridKind::Aligned,
            "gathered" | "gather" | "multline" => GridKind::Gathered,
            _ => GridKind::Matrix,
        };
        let grid = Node::Grid { rows, kind };
        let fence = |l: &str, r: &str| Node::Fenced {
            left: l.into(),
            body: Box::new(grid.clone()),
            right: r.into(),
        };
        match name {
            "pmatrix" => fence("(", ")"),
            "bmatrix" => fence("[", "]"),
            "Bmatrix" => fence("{", "}"),
            "vmatrix" => fence("|", "|"),
            "Vmatrix" => fence("‖", "‖"),
            "cases" | "dcases" => fence("{", ""),
            _ => grid,
        }
    }

    // & · \\ 로 나뉜 칸들을 \end{..}까지 읽는다
    fn grid_rows(&mut self) -> Vec<Vec<Node>> {
        let mut rows = vec![Vec::new()];
        loop {
            let cell = self.row(&[Stop::Amp, Stop::Newline, Stop::End, Stop::Brace]);
            rows.last_mut().unwrap().push(cell);
            match self.t.get(self.i).cloned() {
                Some(Tok::Ch('&')) => self.i += 1,
                Some(Tok::Cmd(c)) if c == "\\" || c == "cr" => {
                    self.i += 1;
                    let _ = self.opt_bracket(); // \\[2pt]
                    rows.push(Vec::new());
                }
                Some(Tok::Cmd(c)) if c == "end" => {
                    self.i += 1;
                    let _ = self.raw_group();
                    break;
                }
                _ => break,
            }
        }
        while rows.len() > 1 && rows.last().is_some_and(|r| r.iter().all(is_empty)) {
            rows.pop();
        }
        rows
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Stop {
    Brace,
    Bracket,
    Amp,
    Newline,
    Right,
    End,
}

fn is_empty(n: &Node) -> bool {
    matches!(n, Node::Row(v) if v.is_empty())
}

fn sym(t: &str, class: Class) -> Node {
    Node::Sym {
        text: t.into(),
        class,
        text_font: false,
    }
}

fn char_node(c: char) -> Node {
    let class = match c {
        '+' | '*' => Class::Bin,
        '-' => return sym("−", Class::Bin),
        '=' | '<' | '>' | ':' => Class::Rel,
        '(' | '[' => Class::Open,
        ')' | ']' => Class::Close,
        ',' | ';' => Class::Punct,
        '~' => return Node::Space(0.25),
        _ => Class::Ord,
    };
    Node::Sym {
        text: math_italic(c).to_string(),
        class,
        text_font: false,
    }
}

// 최상위 \\ 는 여러 줄(가운데 정렬)로 본다
fn parse(src: &str) -> Node {
    let mut p = Parser {
        t: tokenize(src),
        i: 0,
    };
    let mut lines = Vec::new();
    loop {
        lines.push(p.row(&[Stop::Newline]));
        match p.peek() {
            Some(Tok::Cmd(c)) if c == "\\" || c == "cr" => {
                p.i += 1;
                let _ = p.opt_bracket();
            }
            _ => break,
        }
    }
    lines.retain(|l| !is_empty(l));
    if lines.len() <= 1 {
        lines.pop().unwrap_or(Node::Row(Vec::new()))
    } else {
        Node::Grid {
            rows: lines.into_iter().map(|l| vec![l]).collect(),
            kind: GridKind::Gathered,
        }
    }
}

// ---------- 배치 ----------

#[derive(Clone)]
enum Op {
    Text { x: f32, y: f32, galley: Arc<Galley> }, // y = 기준선, galley는 기준선 보정 전
    Line { a: Pos2, b: Pos2, w: f32 },
    Path { pts: Vec<Pos2>, w: f32 },
    Rect { r: Rect, w: f32 },
}

#[derive(Clone, Default)]
pub(crate) struct MBox {
    pub w: f32,
    pub asc: f32,  // 기준선 위
    pub desc: f32, // 기준선 아래
    ops: Vec<Op>,
}

impl MBox {
    fn shifted(mut self, dx: f32, dy: f32) -> Vec<Op> {
        for op in &mut self.ops {
            match op {
                Op::Text { x, y, .. } => {
                    *x += dx;
                    *y += dy;
                }
                Op::Line { a, b, .. } => {
                    *a += vec2(dx, dy);
                    *b += vec2(dx, dy);
                }
                Op::Path { pts, .. } => pts.iter_mut().for_each(|p| *p += vec2(dx, dy)),
                Op::Rect { r, .. } => *r = r.translate(vec2(dx, dy)),
            }
        }
        self.ops
    }

    fn h(&self) -> f32 {
        self.asc + self.desc
    }
}

struct Lay<'a, 'b> {
    f: &'a mut FontsView<'b>,
    color: Color32,
    root: f32,
}

impl Lay<'_, '_> {
    fn glyphs(&mut self, text: &str, size: f32, text_font: bool) -> MBox {
        let family = if text_font {
            egui::FontFamily::Proportional
        } else {
            theme::math()
        };
        let g = self
            .f
            .layout_no_wrap(text.to_string(), FontId::new(size, family), self.color);
        let w = g.size().x;
        MBox {
            w,
            asc: size * 0.72,
            desc: size * 0.22,
            ops: vec![Op::Text {
                x: 0.0,
                y: 0.0,
                galley: g,
            }],
        }
    }

    fn row(&mut self, items: &[Node], size: f32, display: bool) -> MBox {
        let mut out = MBox::default();
        let mut prev: Option<Class> = None;
        for n in items {
            let mut class = n.class();
            if class == Class::Bin
                && matches!(
                    prev,
                    None | Some(Class::Bin | Class::Rel | Class::Open | Class::Punct | Class::Op)
                )
            {
                class = Class::Ord; // 앞에 피연산자가 없으면 부호 (−x)
            }
            if let Some(pc) = prev {
                out.w += size * space(pc, class, size < self.root * 0.9);
            }
            let b = self.node(n, size, display);
            out.asc = out.asc.max(b.asc);
            out.desc = out.desc.max(b.desc);
            let x = out.w;
            out.w += b.w;
            out.ops.extend(b.shifted(x, 0.0));
            if !matches!(n, Node::Space(_)) {
                prev = Some(class);
            }
        }
        if items.is_empty() {
            out.asc = size * 0.5;
        }
        out
    }

    fn node(&mut self, n: &Node, size: f32, display: bool) -> MBox {
        let axis = size * 0.25; // 분수선·괄호 중심 높이
        match n {
            Node::Sym {
                text, text_font, ..
            } => self.glyphs(text, size, *text_font),
            Node::Row(items) => self.row(items, size, display),
            Node::Space(em) => MBox {
                w: em * size,
                ..Default::default()
            },
            Node::Scripts { base, sup, sub } => {
                let ss = (size * 0.7).max(self.root * 0.5);
                if let Node::BigOp { limits, .. } = **base {
                    if limits && display {
                        return self.limits(base, sup.as_deref(), sub.as_deref(), size, ss);
                    }
                }
                let b = self.node(base, size, display);
                let up = sup.as_ref().map(|s| self.node(s, ss, false));
                let dn = sub.as_ref().map(|s| self.node(s, ss, false));
                let mut out = MBox {
                    w: b.w,
                    asc: b.asc,
                    desc: b.desc,
                    ops: Vec::new(),
                };
                let big = matches!(**base, Node::BigOp { func: false, .. });
                let x = b.w + size * 0.05;
                out.ops.extend(b.shifted(0.0, 0.0));
                let mut w = x;
                if let Some(u) = up {
                    let shift = (out.asc - size * 0.3).max(size * 0.42)
                        + if big { size * 0.1 } else { 0.0 };
                    out.asc = out.asc.max(shift + u.asc);
                    w = w.max(x + u.w);
                    out.ops.extend(u.shifted(x, -shift));
                }
                if let Some(d) = dn {
                    let shift = if sup.is_some() {
                        size * 0.32
                    } else {
                        size * 0.2
                    }
                    .max(out.desc - size * 0.1);
                    let dx = if big { x - size * 0.25 } else { x };
                    out.desc = out.desc.max(shift + d.desc);
                    w = w.max(dx + d.w);
                    out.ops.extend(d.shifted(dx, shift));
                }
                out.w = w + size * 0.05;
                out
            }
            Node::BigOp { text, func, .. } => {
                if *func {
                    let mut b = self.glyphs(text, size, false);
                    b.w += size * 0.08;
                    return b;
                }
                let s = if display { size * 1.55 } else { size * 1.1 };
                let mut b = self.glyphs(text, s, false);
                // 큰 연산자는 수식 축에 가운데를 맞춘다
                let mid = (b.asc - b.desc) / 2.0;
                let dy = mid - axis;
                b.asc -= dy;
                b.desc += dy;
                let ops = b.clone().shifted(0.0, dy);
                MBox { ops, ..b }
            }
            Node::Frac { num, den, bar } => {
                let s = if display {
                    size
                } else {
                    (size * 0.8).max(self.root * 0.5)
                };
                let nb = self.node(num, s, false);
                let db = self.node(den, s, false);
                let w = nb.w.max(db.w) + size * 0.25;
                let gap = size * 0.14;
                let t = (size * 0.055).max(0.6);
                let mut ops = Vec::new();
                let n_base = axis + gap + nb.desc;
                let d_base = -axis + gap + db.asc;
                let (nw, dw) = (nb.w, db.w);
                let (asc, desc) = (n_base + nb.asc, d_base + db.desc);
                ops.extend(nb.shifted((w - nw) / 2.0, -n_base));
                ops.extend(db.shifted((w - dw) / 2.0, d_base));
                if *bar {
                    ops.push(Op::Line {
                        a: pos2(size * 0.06, -axis),
                        b: pos2(w - size * 0.06, -axis),
                        w: t,
                    });
                }
                MBox { w, asc, desc, ops }
            }
            Node::Sqrt { index, body } => {
                let b = self.node(body, size, display);
                let gap = size * 0.12;
                let t = (size * 0.055).max(0.6);
                let top = b.asc + gap;
                let bot = b.desc.max(size * 0.15);
                let hook = size * 0.55;
                let mut ops = Vec::new();
                let mut x0 = 0.0;
                if let Some(ix) = index {
                    let ib = self.node(ix, (size * 0.5).max(self.root * 0.45), false);
                    let iw = ib.w;
                    let lift = (top + bot) * 0.45 - bot + ib.desc;
                    ops.extend(ib.shifted(0.0, -lift));
                    x0 = (iw - hook * 0.45).max(0.0);
                }
                ops.push(Op::Path {
                    pts: vec![
                        pos2(x0, -(top - bot) * 0.25 + size * 0.05),
                        pos2(x0 + hook * 0.2, -(top - bot) * 0.3),
                        pos2(x0 + hook * 0.5, bot),
                        pos2(x0 + hook, -top),
                        pos2(x0 + hook + b.w + size * 0.1, -top),
                    ],
                    w: t,
                });
                let bw = b.w;
                ops.extend(b.shifted(x0 + hook + size * 0.05, 0.0));
                MBox {
                    w: x0 + hook + bw + size * 0.15,
                    asc: top + t,
                    desc: bot,
                    ops,
                }
            }
            Node::Fenced { left, body, right } => {
                let b = self.node(body, size, display);
                let half = (b.asc - axis).max(b.desc + axis).max(size * 0.5) + size * 0.08;
                let mut ops = Vec::new();
                let mut x = 0.0;
                let mut asc = b.asc.max(axis + half);
                let mut desc = b.desc.max(half - axis);
                if !left.is_empty() {
                    let d = self.delimiter(left, half, axis, size);
                    asc = asc.max(d.asc);
                    desc = desc.max(d.desc);
                    x = d.w;
                    ops.extend(d.shifted(0.0, 0.0));
                }
                let bw = b.w;
                ops.extend(b.shifted(x, 0.0));
                x += bw;
                if !right.is_empty() {
                    let d = self.delimiter(right, half, axis, size);
                    let dw = d.w;
                    ops.extend(d.shifted(x, 0.0));
                    x += dw;
                }
                MBox {
                    w: x,
                    asc,
                    desc,
                    ops,
                }
            }
            Node::Big { delim, scale } => {
                let half = size * 0.6 * scale;
                self.delimiter(delim, half, axis, size)
            }
            Node::Accent { kind, body } => {
                let b = self.node(body, size, display);
                let t = (size * 0.05).max(0.6);
                let (w, asc, desc) = (b.w, b.asc, b.desc);
                let mut ops = b.shifted(0.0, 0.0);
                let top = -(asc + size * 0.08);
                let c = w / 2.0;
                let mut new_asc = asc + size * 0.25;
                match kind {
                    Accent::Bar | Accent::Overline => {
                        let pad = if *kind == Accent::Bar {
                            size * 0.1
                        } else {
                            0.0
                        };
                        ops.push(Op::Line {
                            a: pos2(pad, top),
                            b: pos2(w - pad, top),
                            w: t,
                        });
                        new_asc = asc + size * 0.12;
                    }
                    Accent::Underline => {
                        ops.push(Op::Line {
                            a: pos2(0.0, desc + size * 0.08),
                            b: pos2(w, desc + size * 0.08),
                            w: t,
                        });
                        return MBox {
                            w,
                            asc,
                            desc: desc + size * 0.14,
                            ops,
                        };
                    }
                    Accent::Hat => ops.push(Op::Path {
                        pts: vec![
                            pos2(c - size * 0.22, top),
                            pos2(c, top - size * 0.14),
                            pos2(c + size * 0.22, top),
                        ],
                        w: t,
                    }),
                    Accent::Vec => {
                        let (x0, x1, y) = (
                            size * 0.05,
                            w.max(size * 0.5) - size * 0.05,
                            top - size * 0.06,
                        );
                        ops.push(Op::Line {
                            a: pos2(x0, y),
                            b: pos2(x1, y),
                            w: t,
                        });
                        ops.push(Op::Path {
                            pts: vec![
                                pos2(x1 - size * 0.12, y - size * 0.08),
                                pos2(x1, y),
                                pos2(x1 - size * 0.12, y + size * 0.08),
                            ],
                            w: t,
                        });
                    }
                    Accent::Tilde => ops.push(Op::Path {
                        pts: (0..=8)
                            .map(|i| {
                                let u = i as f32 / 8.0;
                                pos2(
                                    c - size * 0.22 + u * size * 0.44,
                                    top - size * 0.05
                                        - (u * std::f32::consts::TAU).sin() * size * 0.05,
                                )
                            })
                            .collect(),
                        w: t,
                    }),
                    Accent::Dot | Accent::Ddot => {
                        let r = size * 0.045;
                        let xs: &[f32] = if *kind == Accent::Dot {
                            &[0.0]
                        } else {
                            &[-0.12, 0.12]
                        };
                        for dx in xs {
                            let cx = c + dx * size;
                            ops.push(Op::Path {
                                pts: (0..=8)
                                    .map(|i| {
                                        let a = i as f32 / 8.0 * std::f32::consts::TAU;
                                        pos2(cx + a.cos() * r, top - size * 0.05 + a.sin() * r)
                                    })
                                    .collect(),
                                w: r * 2.0,
                            });
                        }
                    }
                }
                MBox {
                    w,
                    asc: new_asc,
                    desc,
                    ops,
                }
            }
            Node::Boxed(body) => {
                let b = self.node(body, size, display);
                let pad = size * 0.2;
                let (w, asc, desc) = (b.w, b.asc, b.desc);
                let mut ops = b.shifted(pad, 0.0);
                ops.push(Op::Rect {
                    r: Rect::from_min_max(pos2(0.0, -asc - pad), pos2(w + 2.0 * pad, desc + pad)),
                    w: (size * 0.05).max(0.6),
                });
                MBox {
                    w: w + 2.0 * pad,
                    asc: asc + pad,
                    desc: desc + pad,
                    ops,
                }
            }
            Node::Grid { rows, kind } => self.grid(rows, *kind, size, display),
        }
    }

    fn limits(
        &mut self,
        base: &Node,
        sup: Option<&Node>,
        sub: Option<&Node>,
        size: f32,
        ss: f32,
    ) -> MBox {
        let b = self.node(base, size, true);
        let up = sup.map(|s| self.node(s, ss, false));
        let dn = sub.map(|s| self.node(s, ss, false));
        let w = [
            Some(b.w),
            up.as_ref().map(|u| u.w),
            dn.as_ref().map(|d| d.w),
        ]
        .into_iter()
        .flatten()
        .fold(0.0, f32::max);
        let gap = size * 0.1;
        let (bw, mut asc, mut desc) = (b.w, b.asc, b.desc);
        let mut ops = b.shifted((w - bw) / 2.0, 0.0);
        if let Some(u) = up {
            let base_y = -(asc + gap + u.desc);
            asc = asc + gap + u.h();
            let uw = u.w;
            ops.extend(u.shifted((w - uw) / 2.0, base_y));
        }
        if let Some(d) = dn {
            let base_y = desc + gap + d.asc;
            desc = desc + gap + d.h();
            let dw = d.w;
            ops.extend(d.shifted((w - dw) / 2.0, base_y));
        }
        MBox { w, asc, desc, ops }
    }

    // 늘어나는 괄호: ( ) [ ] | ‖ 는 선으로, 나머지는 글자를 키워 그린다
    fn delimiter(&mut self, d: &str, half: f32, axis: f32, size: f32) -> MBox {
        let t = (size * 0.06).max(0.7);
        let (top, bot) = (-(axis + half), half - axis);
        let w = size * 0.38;
        let mut ops = Vec::new();
        match d {
            "(" | ")" => {
                let left = d == "(";
                let (xi, xo) = if left {
                    (w * 0.8, w * 0.25)
                } else {
                    (w * 0.2, w * 0.75)
                };
                let pts: Vec<Pos2> = (0..=16)
                    .map(|i| {
                        let u = i as f32 / 16.0;
                        let x = xi + (xo - xi) * (1.0 - (2.0 * u - 1.0).powi(2));
                        pos2(x, top + (bot - top) * u)
                    })
                    .collect();
                ops.push(Op::Path { pts, w: t * 1.3 });
            }
            "[" | "]" | "⌊" | "⌋" | "⌈" | "⌉" => {
                let left = matches!(d, "[" | "⌊" | "⌈");
                let (xs, xe) = if left {
                    (w * 0.75, w * 0.3)
                } else {
                    (w * 0.25, w * 0.7)
                };
                let top_arm = !matches!(d, "⌊" | "⌋");
                let bot_arm = !matches!(d, "⌈" | "⌉");
                let mut pts = Vec::new();
                if top_arm {
                    pts.push(pos2(xs, top));
                }
                pts.push(pos2(xe, top));
                pts.push(pos2(xe, bot));
                if bot_arm {
                    pts.push(pos2(xs, bot));
                }
                ops.push(Op::Path { pts, w: t });
            }
            "|" => ops.push(Op::Line {
                a: pos2(w / 2.0, top),
                b: pos2(w / 2.0, bot),
                w: t,
            }),
            "‖" => {
                for x in [w * 0.35, w * 0.65] {
                    ops.push(Op::Line {
                        a: pos2(x, top),
                        b: pos2(x, bot),
                        w: t,
                    });
                }
            }
            "{" | "}" => {
                // 중괄호: 위아래 두 곡선 + 가운데 뾰족
                let left = d == "{";
                let (xin, xmid, xout) = if left {
                    (w * 0.85, w * 0.5, w * 0.15)
                } else {
                    (w * 0.15, w * 0.5, w * 0.85)
                };
                let mid = -axis;
                let r = (half * 0.15).min(size * 0.2);
                let pts = vec![
                    pos2(xin, top),
                    pos2(xmid, top + r),
                    pos2(xmid, mid - r),
                    pos2(xout, mid),
                    pos2(xmid, mid + r),
                    pos2(xmid, bot - r),
                    pos2(xin, bot),
                ];
                ops.push(Op::Path { pts, w: t });
            }
            _ => {
                let s = (half * 2.0 / 0.95).max(size);
                let mut b = self.glyphs(d, s, false);
                let mid = (b.asc - b.desc) / 2.0;
                let dy = mid - axis;
                b.asc -= dy;
                b.desc += dy;
                let ops = b.clone().shifted(0.0, dy);
                return MBox { ops, ..b };
            }
        }
        MBox {
            w,
            asc: -top,
            desc: bot,
            ops,
        }
    }

    fn grid(&mut self, rows: &[Vec<Node>], kind: GridKind, size: f32, display: bool) -> MBox {
        let cells: Vec<Vec<MBox>> = rows
            .iter()
            .map(|r| {
                r.iter()
                    .enumerate()
                    .map(|(ci, c)| {
                        let d = display && kind != GridKind::Matrix;
                        match c {
                            // aligned의 "&=": 정렬점 뒤 칸 앞에 빈 글자를 둬 관계 기호 간격을 살린다 (TeX의 {}=)
                            Node::Row(items) if kind == GridKind::Aligned && ci % 2 == 1 => {
                                let mut v = vec![sym("", Class::Ord)];
                                v.extend(items.iter().cloned());
                                self.row(&v, size, d)
                            }
                            _ => self.node(c, size, d),
                        }
                    })
                    .collect()
            })
            .collect();
        let ncol = cells.iter().map(Vec::len).max().unwrap_or(0);
        let mut colw = vec![0.0f32; ncol];
        for r in &cells {
            for (i, c) in r.iter().enumerate() {
                colw[i] = colw[i].max(c.w);
            }
        }
        let col_gap = match kind {
            GridKind::Matrix => size * 0.9,
            GridKind::Cases => size * 1.0,
            _ => 0.0,
        };
        let row_gap = size * 0.3;
        let total_w: f32 = colw.iter().sum::<f32>() + col_gap * ncol.saturating_sub(1) as f32;
        let heights: Vec<(f32, f32)> = cells
            .iter()
            .map(|r| {
                r.iter().fold((size * 0.72, size * 0.22), |(a, d), c| {
                    (a.max(c.asc), d.max(c.desc))
                })
            })
            .collect();
        let total_h: f32 = heights.iter().map(|(a, d)| a + d).sum::<f32>()
            + row_gap * heights.len().saturating_sub(1) as f32;
        let top = -(size * 0.25) - total_h / 2.0; // 표 전체를 수식 축에 가운데 맞춤
        let mut ops = Vec::new();
        let mut y = top;
        for (ri, r) in cells.into_iter().enumerate() {
            let (a, d) = heights[ri];
            let base = y + a;
            let mut x = 0.0;
            for (ci, c) in r.into_iter().enumerate() {
                let cw = c.w;
                let dx = match kind {
                    GridKind::Matrix => (colw[ci] - cw) / 2.0,
                    GridKind::Gathered => (total_w - cw) / 2.0 - x,
                    GridKind::Cases => 0.0,
                    GridKind::Aligned => {
                        if ci % 2 == 0 {
                            colw[ci] - cw
                        } else {
                            0.0
                        }
                    }
                };
                ops.extend(c.shifted(x + dx, base));
                x += colw[ci] + col_gap;
            }
            y = base + d + row_gap;
        }
        MBox {
            w: total_w,
            asc: -top,
            desc: total_h + top,
            ops,
        }
    }
}

// TeX 원자 간격 (em). 첨자 크기에서는 넓은 간격을 뺀다
fn space(a: Class, b: Class, script: bool) -> f32 {
    use Class::*;
    let (thin, med, thick) = (3.0 / 18.0, 4.0 / 18.0, 5.0 / 18.0);
    let s = match (a, b) {
        (Ord, Op) | (Op, Ord) | (Op, Op) | (Close, Op) => thin,
        (Bin, _) | (_, Bin) => med,
        (Rel, Rel) => 0.0,
        (Rel, _) | (_, Rel) => thick,
        (Punct, _) => thin,
        _ => 0.0,
    };
    if script && s > thin {
        0.0
    } else {
        s
    }
}

// ---------- 공개 API ----------

pub(crate) fn layout(
    ctx: &egui::Context,
    src: &str,
    size: f32,
    display: bool,
    color: Color32,
) -> MBox {
    let node = parse(src.trim());
    ctx.fonts_mut(|f| {
        let mut l = Lay {
            f,
            color,
            root: size,
        };
        l.node(&node, size, display)
    })
}

pub(crate) fn paint(painter: &egui::Painter, b: &MBox, origin: Pos2, color: Color32) {
    for op in &b.ops {
        match op {
            Op::Text { x, y, galley } => {
                // galley 위쪽에서 기준선까지 거리만큼 올려 그린다
                let base_off = galley
                    .rows
                    .first()
                    .and_then(|r| r.row.glyphs.first().map(|g| r.pos.y + g.pos.y))
                    .unwrap_or(0.0);
                painter.galley(origin + vec2(*x, *y - base_off), Arc::clone(galley), color);
            }
            Op::Line { a, b, w } => {
                painter.line_segment(
                    [origin + a.to_vec2(), origin + b.to_vec2()],
                    Stroke::new(*w, color),
                );
            }
            Op::Path { pts, w } => {
                painter.add(egui::Shape::line(
                    pts.iter().map(|p| origin + p.to_vec2()).collect(),
                    Stroke::new(*w, color),
                ));
            }
            Op::Rect { r, w } => {
                painter.rect_stroke(
                    r.translate(origin.to_vec2()),
                    0.0,
                    Stroke::new(*w, color),
                    egui::StrokeKind::Middle,
                );
            }
        }
    }
}

// 블록 수식을 가운데에 그린다. 넓으면 폭에 맞춰 줄인다. 그린 영역과 기준선 y를 돌려준다.
pub fn render(
    ui: &mut egui::Ui,
    p: &Pal,
    src: &str,
    size: f32,
    display: bool,
) -> Option<(Rect, f32)> {
    if src.trim().is_empty() {
        return None;
    }
    let avail = ui.available_width().max(40.0);
    let mut b = layout(ui.ctx(), src, size, display, p.text);
    if b.w > avail {
        b = layout(
            ui.ctx(),
            src,
            (size * avail / b.w).max(size * 0.5),
            display,
            p.text,
        );
    }
    let pad = 2.0;
    let (rect, _) = ui.allocate_exact_size(vec2(avail, b.h() + pad * 2.0), egui::Sense::hover());
    let x = if display {
        rect.left() + ((avail - b.w) / 2.0).max(0.0)
    } else {
        rect.left()
    };
    let base = rect.top() + pad + b.asc;
    paint(ui.painter(), &b, pos2(x, base), p.text);
    let drawn = Rect::from_min_max(pos2(x, rect.top()), pos2(x + b.w, rect.bottom()));
    Some((drawn, base))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxed(src: &str) -> MBox {
        let ctx = egui::Context::default();
        let mut b = None;
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            b = Some(layout(ui.ctx(), src, 20.0, true, Color32::BLACK));
        });
        out.textures_delta.clear();
        b.unwrap()
    }

    #[test]
    fn parses_and_lays_out_common_latex() {
        // 분수는 위아래로 높아지고, 첨자는 기준선 위로 올라간다
        let plain = boxed("x");
        let frac = boxed(r"\frac{a+b}{c}");
        assert!(frac.asc > plain.asc && frac.desc > plain.desc);
        let sup = boxed("x^2");
        assert!(sup.asc > plain.asc && sup.w > plain.w);
        // 표시 수식의 ∑는 아래위 첨자를 위아래에 쌓는다 → 옆에 붙일 때보다 좁다
        let lim = boxed(r"\sum_{i=1}^{n} i");
        let side = boxed(r"\sum\nolimits_{i=1}^{n} i");
        assert!(lim.w < side.w && lim.h() > side.h());
        // 행렬·cases·여러 줄은 줄 수만큼 높다
        let m = boxed(r"\begin{pmatrix} 1 & 2 \\ 3 & 4 \end{pmatrix}");
        assert!(m.h() > frac.h() * 0.9);
        let two = boxed(r"a = b \\ c = d");
        assert!(two.h() > plain.h() * 1.8);
        // 변수는 수학 이탤릭, 함수 이름·\text는 그대로
        match parse("x") {
            Node::Row(v) => assert!(matches!(&v[0], Node::Sym { text, .. } if text == "𝑥")),
            n => panic!("{n:?}"),
        }
        assert!(
            matches!(parse(r"\sin").clone(), Node::Row(v) if matches!(&v[0], Node::BigOp { text, func: true, .. } if text == "sin"))
        );
        // 깨진 입력에도 패닉 없이 무언가 그린다
        for bad in [
            r"\frac{",
            r"\left( x",
            r"}}}",
            r"\begin{matrix} 1 &",
            r"\unknown{a}",
            "^_^",
            r"\sqrt[",
            "\\",
        ] {
            let _ = boxed(bad);
        }
    }
}
