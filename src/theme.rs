// 웹 버전(index.html)의 디자인 토큰·글꼴·아이콘을 그대로 옮긴 것
use eframe::egui;
use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, Shadow, Stroke, TextStyle};

#[derive(Clone, Copy)]
pub struct Pal {
    pub bg: Color32,
    pub surface: Color32,
    pub elevated: Color32,
    pub hover: Color32,
    pub border: Color32,
    pub border_hover: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub code_bg: Color32,
    pub overlay: Color32,
    pub shadow: Color32,
}

pub const AMBER: Color32 = Color32::from_rgb(0xf5, 0x9e, 0x0b);
pub const GREEN: Color32 = Color32::from_rgb(0x10, 0xb9, 0x81);
pub const RED: Color32 = Color32::from_rgb(0xef, 0x44, 0x44);

const fn a(r: u8, g: u8, b: u8, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied_const(r, g, b, alpha)
}

pub fn pal(dark: bool) -> Pal {
    if dark {
        Pal {
            bg: Color32::from_rgb(0x19, 0x19, 0x19),
            surface: Color32::from_rgb(0x20, 0x20, 0x20),
            elevated: Color32::from_rgb(0x26, 0x26, 0x26),
            hover: Color32::from_rgb(0x30, 0x30, 0x30),
            border: a(255, 255, 255, 20),
            border_hover: a(255, 255, 255, 41),
            text: Color32::from_rgb(0xeb, 0xeb, 0xeb),
            muted: Color32::from_rgb(0x9b, 0x9b, 0x9b),
            faint: Color32::from_rgb(0x6e, 0x6e, 0x6e),
            accent: Color32::from_rgb(0x23, 0x83, 0xe2),
            code_bg: Color32::from_rgb(0x22, 0x22, 0x22),
            overlay: a(0, 0, 0, 173),
            shadow: a(0, 0, 0, 153),
        }
    } else {
        Pal {
            bg: Color32::WHITE,
            surface: Color32::from_rgb(0xf7, 0xf7, 0xf5),
            elevated: Color32::WHITE,
            hover: Color32::from_rgb(0xef, 0xef, 0xed),
            border: a(0, 0, 0, 20),
            border_hover: a(0, 0, 0, 41),
            text: Color32::from_rgb(0x37, 0x35, 0x2f),
            muted: Color32::from_rgb(0x78, 0x77, 0x74),
            faint: Color32::from_rgb(0x9b, 0x9a, 0x97),
            accent: Color32::from_rgb(0x23, 0x83, 0xe2),
            code_bg: Color32::from_rgb(0xf7, 0xf6, 0xf3),
            overlay: a(15, 15, 15, 107),
            shadow: a(0, 0, 0, 31),
        }
    }
}

pub fn shadow(p: &Pal) -> Shadow {
    Shadow {
        offset: [0, 16],
        blur: 36,
        spread: 0,
        color: p.shadow,
    }
}

pub fn apply(ctx: &egui::Context, dark: bool) {
    let p = pal(dark);
    let mut v = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(12);
    v.window_shadow = shadow(&p);
    v.popup_shadow = Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: p.shadow,
    };
    v.menu_corner_radius = CornerRadius::same(10);
    v.extreme_bg_color = p.elevated;
    v.text_edit_bg_color = Some(p.elevated);
    v.faint_bg_color = p.surface;
    v.code_bg_color = p.code_bg;
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.accent.gamma_multiply(0.3);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.override_text_color = None;
    let w = &mut v.widgets;
    for s in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        s.corner_radius = CornerRadius::same(8);
        s.fg_stroke = Stroke::new(1.0, p.text);
        s.expansion = 0.0;
    }
    w.noninteractive.bg_fill = p.surface;
    w.noninteractive.weak_bg_fill = p.surface;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.inactive.weak_bg_fill = Color32::TRANSPARENT; // 고스트 버튼
    w.inactive.bg_fill = p.elevated;
    w.inactive.bg_stroke = Stroke::NONE;
    w.hovered.weak_bg_fill = p.elevated;
    w.hovered.bg_fill = p.hover;
    w.hovered.bg_stroke = Stroke::new(1.0, p.border);
    w.active.weak_bg_fill = p.hover;
    w.active.bg_fill = p.hover;
    w.active.bg_stroke = Stroke::new(1.0, p.border_hover);
    w.open.weak_bg_fill = p.elevated;
    w.open.bg_fill = p.elevated;
    w.open.bg_stroke = Stroke::new(1.0, p.border);
    let theme = if dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    ctx.set_visuals_of(theme, v);
    ctx.set_theme(theme);
    ctx.all_styles_mut(|s| {
        s.text_styles = [
            (TextStyle::Small, FontId::proportional(11.5)),
            (TextStyle::Body, FontId::proportional(13.5)),
            (TextStyle::Button, FontId::proportional(12.5)),
            (TextStyle::Heading, FontId::new(15.0, bold())),
            (TextStyle::Monospace, FontId::monospace(12.5)),
        ]
        .into();
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(8.0, 5.0);
        s.spacing.window_margin = Margin::same(0);
        s.spacing.menu_margin = Margin::same(4);
        s.spacing.interact_size.y = 26.0;
    });
}

pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

// 웹과 같은 글꼴: KoPubWorld 돋움(설치돼 있으면) → Segoe UI → 맑은 고딕, 코드는 Consolas
pub fn install_fonts(ctx: &egui::Context) {
    let win = std::path::PathBuf::from(r"C:\Windows\Fonts");
    let user = std::env::var_os("LOCALAPPDATA")
        .map(|d| std::path::PathBuf::from(d).join(r"Microsoft\Windows\Fonts"));
    let dirs: Vec<std::path::PathBuf> = user.into_iter().chain([win]).collect();
    // 파일명이 이 접두어로 시작하는 첫 글꼴 (사용자 글꼴은 "… 1.ttf"처럼 접미어가 붙기도 함)
    let find = |prefixes: &[&str]| -> Option<Vec<u8>> {
        prefixes.iter().find_map(|pre| {
            dirs.iter().find_map(|d| {
                std::fs::read_dir(d).ok()?.flatten().find_map(|e| {
                    let n = e.file_name().to_string_lossy().to_lowercase();
                    (n.starts_with(&pre.to_lowercase()) && n.ends_with(".ttf"))
                        .then(|| std::fs::read(e.path()).ok())
                        .flatten()
                })
            })
        })
    };
    let mut fonts = egui::FontDefinitions::default();
    let defaults = fonts.families[&FontFamily::Proportional].clone();
    let mut add = |key: &str, bytes: Option<Vec<u8>>| -> Option<String> {
        let b = bytes?;
        fonts
            .font_data
            .insert(key.into(), egui::FontData::from_owned(b).into());
        Some(key.to_string())
    };
    let regular = add("regular", find(&["KoPubWorld Dotum Medium", "segoeui.ttf"]));
    let bold_f = add("bold", find(&["KoPubWorld Dotum Bold", "segoeuib.ttf"]));
    let ko = add("malgun", find(&["malgun.ttf"]));
    let ko_bold = add("malgunbd", find(&["malgunbd.ttf"]));
    let mono = add("mono", find(&["consola.ttf"]));
    // 기호·이모지 대체 (코드 복사 버튼 아이콘 등)
    let sym = add("symbol", find(&["seguisym.ttf"]));
    let emoji = add("emoji", find(&["seguiemj.ttf"]));
    let chain = |list: &[&Option<String>], tail: &[String]| -> Vec<String> {
        list.iter()
            .filter_map(|o| (*o).clone())
            .chain(tail.iter().cloned())
            .collect()
    };
    let mono_defaults = fonts.families[&FontFamily::Monospace].clone();
    fonts.families.insert(
        FontFamily::Proportional,
        chain(&[&regular, &ko, &sym, &emoji], &defaults),
    );
    fonts.families.insert(
        FontFamily::Monospace,
        chain(&[&mono, &ko, &sym, &emoji], &mono_defaults),
    );
    fonts.families.insert(
        bold(),
        chain(&[&bold_f, &ko_bold, &regular, &ko, &sym, &emoji], &defaults),
    );
    ctx.set_fonts(fonts);
}

// ---------- 아이콘 (웹과 같은 Lucide SVG, 흰색으로 그린 뒤 tint로 색칠) ----------

pub type Icon = (&'static str, &'static str);

macro_rules! icon {
    ($name:ident, $body:literal) => {
        pub const $name: Icon = (
            concat!("bytes://", stringify!($name), ".svg"),
            concat!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">"#,
                $body,
                "</svg>"
            ),
        );
    };
}

icon!(
    BRAND,
    r#"<path d="M4 4v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8.342a2 2 0 0 0-.602-1.43l-4.44-4.342A2 2 0 0 0 13.56 2H6a2 2 0 0 0-2 2z"/><path d="M14 2v6h6"/><path d="m9 13 2 2 4-4"/>"#
);
icon!(
    FILE,
    r#"<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/>"#
);
icon!(
    FOLDER,
    r#"<path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>"#
);
icon!(
    UPLOAD,
    r#"<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/>"#
);
icon!(
    PLUS,
    r#"<line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>"#
);
icon!(
    SAVE,
    r#"<path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z"/><polyline points="17 21 17 13 7 13 7 21"/><polyline points="7 3 7 8 15 8"/>"#
);
icon!(
    LIST,
    r#"<line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="21" y2="18"/><line x1="3" y1="6" x2="3.01" y2="6"/><line x1="3" y1="12" x2="3.01" y2="12"/><line x1="3" y1="18" x2="3.01" y2="18"/>"#
);
icon!(
    SEARCH,
    r#"<circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>"#
);
icon!(
    IMAGE,
    r#"<rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/>"#
);
icon!(
    PRINTER,
    r#"<polyline points="6 9 6 2 18 2 18 9"/><path d="M6 18H4a2 2 0 0 1-2-2v-5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2v5a2 2 0 0 1-2 2h-2"/><rect x="6" y="14" width="12" height="8"/>"#
);
icon!(
    DOWNLOAD,
    r#"<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/>"#
);
icon!(
    SETTINGS,
    r#"<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/>"#
);
icon!(
    SUN,
    r#"<circle cx="12" cy="12" r="5"/><line x1="12" y1="1" x2="12" y2="3"/><line x1="12" y1="21" x2="12" y2="23"/><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/><line x1="1" y1="12" x2="3" y2="12"/><line x1="21" y1="12" x2="23" y2="12"/><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/>"#
);
icon!(
    MAXIMIZE,
    r#"<polyline points="15 3 21 3 21 9"/><polyline points="9 21 3 21 3 15"/><line x1="21" y1="3" x2="14" y2="10"/><line x1="3" y1="21" x2="10" y2="14"/>"#
);
icon!(
    TYPE,
    r#"<polyline points="4 7 4 4 20 4 20 7"/><line x1="9" y1="20" x2="15" y2="20"/><line x1="12" y1="4" x2="12" y2="20"/>"#
);
icon!(
    COPY,
    r#"<rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/>"#
);
icon!(
    CODE,
    r#"<polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/>"#
);
icon!(
    HELP,
    r#"<circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/><line x1="12" y1="17" x2="12.01" y2="17"/>"#
);
icon!(
    EXIT,
    r#"<path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><polyline points="16 17 21 12 16 7"/><line x1="21" y1="12" x2="9" y2="12"/>"#
);
icon!(
    ALERT,
    r#"<circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/>"#
);
icon!(
    TRASH,
    r#"<polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>"#
);
icon!(
    REPLACE,
    r#"<polyline points="1 4 1 10 7 10"/><path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10"/>"#
);
icon!(
    BOLD,
    r#"<path d="M6 4h8a4 4 0 0 1 4 4 4 4 0 0 1-4 4H6z"/><path d="M6 12h9a4 4 0 0 1 4 4 4 4 0 0 1-4 4H6z"/>"#
);
icon!(
    ITALIC,
    r#"<line x1="19" y1="4" x2="10" y2="4"/><line x1="14" y1="20" x2="5" y2="20"/><line x1="15" y1="4" x2="9" y2="20"/>"#
);
icon!(
    STRIKE,
    r#"<path d="M16 4H9a3 3 0 0 0-2.83 4"/><path d="M14 12a4 4 0 0 1 0 8H6"/><line x1="4" y1="12" x2="20" y2="12"/>"#
);
icon!(
    HIGHLIGHT,
    r#"<path d="m9 11-6 6v3h3l6-6"/><path d="m22 12-4.6 4.6a2 2 0 0 1-2.8 0l-5.2-5.2a2 2 0 0 1 0-2.8L14 4"/>"#
);
icon!(
    LINK,
    r#"<path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>"#
);
icon!(
    X,
    r#"<line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>"#
);

pub fn img(icon: Icon, size: f32, color: Color32) -> egui::Image<'static> {
    egui::Image::from_bytes(icon.0, icon.1.as_bytes())
        .fit_to_exact_size(egui::vec2(size, size))
        .tint(color)
}
