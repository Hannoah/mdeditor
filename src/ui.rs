// 화면 구성: 웹 버전(index.html)의 헤더·노션 캔버스·목차 서랍·상태바·모달을 그대로 옮긴 것
use super::*;
use crate::theme::{self, img, Icon, Pal, AMBER, GREEN, RED};
use egui::{
    pos2, vec2, Align, Align2, Color32, CursorIcon, FontFamily, FontId, Frame, Margin, Order, Rect,
    RichText, Sense, Stroke,
};

const HEADING_SCALE: [f32; 6] = [1.95, 1.5, 1.25, 1.1, 1.0, 0.95];
const HEADING_TOP: [f32; 6] = [26.0, 20.0, 14.0, 10.0, 8.0, 8.0];
const PLACEHOLDER: &str = "글을 작성하거나 '/'를 눌러 명령어를 사용하세요";

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let (close_req, win_focused, pasted, ime_preedit) = ctx.input(|i| {
            let close_req = i.viewport().close_requested();
            let mut win_focused = false;
            let mut pasted = Vec::new();
            let mut ime_preedit: Option<bool> = None;
            for e in &i.events {
                match e {
                    egui::Event::WindowFocused(true) => win_focused = true,
                    egui::Event::PasteImage(img) => pasted.push(img.clone()),
                    egui::Event::Ime(ime) => match ime {
                        egui::ImeEvent::Preedit { text, .. } => {
                            ime_preedit = Some(!text.is_empty());
                        }
                        egui::ImeEvent::Commit(_) => {
                            ime_preedit = Some(false);
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
            (close_req, win_focused, pasted, ime_preedit)
        });
        if close_req && !self.allow_close {
            self.save_all();
            if self.any_dirty() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.dialog = Some(Dialog::Unsaved(After::Close));
            }
        }
        if win_focused {
            self.sync_from_disk();
        }
        self.handle_drops(&ctx);
        for img in pasted {
            self.insert_pasted_image(&ctx, &img);
        }
        self.handle_shortcuts(&ctx);
        self.auto_save_tick(&ctx, frame.storage_mut());
        self.title_bar(&ctx);

        let p = theme::pal(self.dark);
        // 패널 구분선 자리(1px)는 아무도 칠하지 않는다 → 배경을 먼저 깔아야 선이 은은하게 보인다
        ui.painter().rect_filled(ui.max_rect(), 0.0, p.bg);
        if !self.focus_mode {
            egui::Panel::top("header")
                .frame(
                    Frame::new()
                        .fill(p.surface)
                        .inner_margin(Margin::symmetric(16, 7)),
                )
                .show(ui, |ui| self.header(ui, &p));
            egui::Panel::top("tab_bar")
                .frame(Frame::new().fill(p.surface).inner_margin(Margin {
                    left: 12,
                    right: 12,
                    top: 2,
                    bottom: 0,
                }))
                .show(ui, |ui| self.tab_bar(ui, &p));
            egui::Panel::bottom("status")
                .frame(
                    Frame::new()
                        .fill(p.surface)
                        .inner_margin(Margin::symmetric(16, 5)),
                )
                .show(ui, |ui| {
                    let r = ui.max_rect().expand2(vec2(16.0, 5.0));
                    ui.painter()
                        .hline(r.x_range(), r.top(), Stroke::new(1.0, p.border));
                    self.status_bar(ui, &p);
                });
            if self.toc_open {
                egui::Panel::left("toc")
                    .exact_size(272.0)
                    .resizable(false)
                    .frame(Frame::new().fill(p.surface))
                    .show(ui, |ui| {
                        let r = ui.max_rect();
                        self.toc(ui, &p);
                        ui.painter().vline(
                            r.right() - 0.5,
                            r.y_range(),
                            Stroke::new(1.0, p.border),
                        );
                    });
            }
        }
        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.bg))
            .show(ui, |ui| self.canvas(ui, &p));
        if let Some(comp) = ime_preedit {
            self.ime_composing = comp;
        }

        if self.focus_mode {
            let sr = ctx.content_rect();
            egui::Area::new(egui::Id::unique("focus-mode-bar"))
                .order(Order::Foreground)
                .anchor(Align2::RIGHT_TOP, vec2(-14.0, 12.0))
                .fixed_pos(pos2(sr.right() - 14.0, sr.top() + 12.0))
                .show(&ctx, |ui| {
                    Frame::new()
                        .fill(p.elevated.gamma_multiply(0.94))
                        .stroke(Stroke::new(1.0, p.border))
                        .corner_radius(255)
                        .shadow(theme::shadow(&p))
                        .inner_margin(Margin::symmetric(8, 4))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                let w_lbl = if self.full_width {
                                    "전체 너비"
                                } else {
                                    "가운데"
                                };
                                if ui
                                    .add(
                                        egui::Button::new(RichText::new(w_lbl).size(11.5).color(
                                            if self.full_width { p.accent } else { p.muted },
                                        ))
                                        .fill(Color32::TRANSPARENT)
                                        .corner_radius(255)
                                        .min_size(vec2(0.0, 22.0)),
                                    )
                                    .on_hover_cursor(CursorIcon::PointingHand)
                                    .on_hover_text("본문 너비 전환 (Ctrl+Alt+W)")
                                    .clicked()
                                {
                                    self.full_width = !self.full_width;
                                }
                                divider(ui, &p);
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new("기본 보기 (F11)")
                                                .size(11.5)
                                                .color(p.text),
                                        )
                                        .fill(p.hover)
                                        .corner_radius(255)
                                        .min_size(vec2(0.0, 22.0)),
                                    )
                                    .on_hover_cursor(CursorIcon::PointingHand)
                                    .clicked()
                                {
                                    self.focus_mode = false;
                                }
                            });
                        });
                });
        }

        self.show_toast(&ctx, &p);
        self.dialogs(&ctx, &p);
        self.context_menus(&ctx, &p);
        self.handle_links(&ctx);
        if self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string("root", self.root.to_string_lossy().into_owned());
        let file = self
            .current
            .as_deref()
            .filter(|n| !is_scratch_key(n))
            .unwrap_or("");
        storage.set_string("file", file.to_string());
        storage.set_string("draft", self.draft_text().to_string());
        storage.set_string("theme", if self.dark { "dark" } else { "light" }.into());
        storage.set_string("full_width", self.full_width.to_string());
        storage.set_string("small_text", self.small_text.to_string());
        storage.set_string("font_family", self.font_family.clone());
    }
}

// ---------- 공용 위젯 ----------

fn icon_btn(ui: &mut egui::Ui, p: &Pal, icon: Icon, tip: &str) -> egui::Response {
    ui.add(
        egui::Button::image(img(icon, 16.0, p.text))
            .min_size(vec2(30.0, 28.0))
            .corner_radius(8),
    )
    .on_hover_text(tip)
}

fn toggle_icon_btn(
    ui: &mut egui::Ui,
    p: &Pal,
    icon: Icon,
    active: bool,
    tip: &str,
) -> egui::Response {
    let mut btn = egui::Button::image(img(icon, 16.0, if active { p.accent } else { p.text }))
        .min_size(vec2(30.0, 28.0))
        .corner_radius(8);
    if active {
        btn = btn
            .fill(p.accent.gamma_multiply(0.16))
            .stroke(Stroke::new(1.0, p.accent.gamma_multiply(0.45)));
    }
    ui.add(btn).on_hover_text(tip)
}

fn small_icon_btn(
    ui: &mut egui::Ui,
    p: &Pal,
    icon: Icon,
    color: Color32,
    tip: &str,
) -> egui::Response {
    let _ = p;
    ui.add(
        egui::Button::image(img(icon, 13.0, color))
            .min_size(vec2(26.0, 26.0))
            .corner_radius(5),
    )
    .on_hover_text(tip)
}

fn divider(ui: &mut egui::Ui, p: &Pal) {
    let (r, _) = ui.allocate_exact_size(vec2(9.0, 16.0), Sense::hover());
    ui.painter()
        .vline(r.center().x, r.y_range(), Stroke::new(1.0, p.border));
}

fn dot(ui: &mut egui::Ui, color: Color32, radius: f32, ring: bool) {
    let (r, _) =
        ui.allocate_exact_size(vec2(radius * 2.0 + 4.0, radius * 2.0 + 4.0), Sense::hover());
    ui.painter().circle_filled(r.center(), radius, color);
    if ring {
        ui.painter()
            .circle_stroke(r.center(), radius + 2.0, Stroke::new(1.5, color));
    }
}

fn hline(ui: &mut egui::Ui, p: &Pal) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter()
        .hline(r.x_range(), r.center().y, Stroke::new(1.0, p.border));
}

// 아이콘 + 글자 한 줄짜리 클릭 행 (메뉴·파일 목록·목차)
fn row(
    ui: &mut egui::Ui,
    p: &Pal,
    icon: Option<(Icon, Color32)>,
    text: &str,
    color: Color32,
    height: f32,
    bg: Option<Color32>,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    if let Some(c) = bg.or(resp.hovered().then_some(p.hover)) {
        ui.painter().rect_filled(rect, 6.0, c);
    }
    let mut x = rect.left() + 10.0;
    if let Some((ic, c)) = icon {
        img(ic, 15.0, c).paint_at(
            ui,
            Rect::from_min_size(pos2(x, rect.center().y - 7.5), vec2(15.0, 15.0)),
        );
        x += 24.0;
    }
    ui.painter().text(
        pos2(x, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(13.0),
        color,
    );
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

// 아이콘 + 라벨 + 오른쪽 단축키 힌트가 있는 컨텍스트·설정 메뉴 행
fn menu_row(
    ui: &mut egui::Ui,
    p: &Pal,
    icon: Option<(Icon, Color32)>,
    text: &str,
    shortcut: &str,
    color: Color32,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 6.0, p.hover);
    }
    let mut x = rect.left() + 8.0;
    if let Some((ic, c)) = icon {
        img(ic, 14.0, c).paint_at(
            ui,
            Rect::from_min_size(pos2(x, rect.center().y - 7.0), vec2(14.0, 14.0)),
        );
        x += 22.0;
    }
    ui.painter().text(
        pos2(x, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(12.5),
        color,
    );
    if !shortcut.is_empty() {
        ui.painter().text(
            pos2(rect.right() - 8.0, rect.center().y),
            Align2::RIGHT_CENTER,
            shortcut,
            FontId::monospace(10.5),
            p.faint,
        );
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), text));
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

fn submenu_row(
    ui: &mut egui::Ui,
    p: &Pal,
    icon: Option<(Icon, Color32)>,
    text: &str,
    active: bool,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
    if active || resp.hovered() {
        ui.painter().rect_filled(rect, 6.0, p.hover);
    }
    let mut x = rect.left() + 8.0;
    if let Some((ic, c)) = icon {
        img(ic, 14.0, c).paint_at(
            ui,
            Rect::from_min_size(pos2(x, rect.center().y - 7.0), vec2(14.0, 14.0)),
        );
        x += 22.0;
    }
    ui.painter().text(
        pos2(x, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(12.5),
        p.text,
    );
    let ax = rect.right() - 10.0;
    let cy = rect.center().y;
    ui.painter().line(
        vec![
            pos2(ax - 2.0, cy - 3.5),
            pos2(ax + 2.0, cy),
            pos2(ax - 2.0, cy + 3.5),
        ],
        Stroke::new(1.5, p.faint),
    );
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), text));
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

fn block_chip(ui: &mut egui::Ui, p: &Pal, badge: &str, rgb: [u8; 3], tip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 26.0), Sense::click());
    let c = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
    let bg = if resp.hovered() {
        c.gamma_multiply(0.26)
    } else {
        c.gamma_multiply(0.14)
    };
    ui.painter().rect_filled(rect, 5.0, bg);
    if resp.hovered() {
        ui.painter().rect_stroke(
            rect,
            5.0,
            Stroke::new(1.0, p.accent),
            egui::StrokeKind::Inside,
        );
    }
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        badge,
        FontId::monospace(11.0),
        c,
    );
    resp.on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(tip)
}

pub(crate) fn primary(ui: &mut egui::Ui, p: &Pal, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            RichText::new(text)
                .size(13.0)
                .color(Color32::WHITE)
                .strong(),
        )
        .fill(p.accent)
        .stroke(Stroke::NONE)
        .corner_radius(6)
        .min_size(vec2(0.0, 30.0)),
    )
}

pub(crate) fn ghost(
    ui: &mut egui::Ui,
    p: &Pal,
    text: &str,
    color: Option<Color32>,
) -> egui::Response {
    let c = color.unwrap_or(p.text);
    let border = color.map_or(p.border, |c| c.gamma_multiply(0.35));
    ui.add(
        egui::Button::new(RichText::new(text).size(13.0).color(c))
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::new(1.0, border))
            .corner_radius(6)
            .min_size(vec2(0.0, 30.0)),
    )
}

// 테두리 있는 입력창 (포커스되면 강조색 테두리)
fn input(
    ui: &mut egui::Ui,
    p: &Pal,
    id: egui::Id,
    text: &mut String,
    hint: &str,
    width: f32,
) -> egui::Response {
    let focused = ui.memory(|m| m.has_focus(id));
    Frame::new()
        .fill(p.elevated)
        .stroke(Stroke::new(1.0, if focused { p.accent } else { p.border }))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(12, 7))
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(text)
                    .id(id)
                    .frame(Frame::NONE)
                    .margin(Margin::ZERO)
                    .hint_text(RichText::new(hint).color(p.faint))
                    .desired_width(width),
            )
        })
        .inner
}

fn modal_header(ui: &mut egui::Ui, p: &Pal, icon: Icon, icon_color: Color32, title: &str) -> bool {
    let mut close = false;
    Frame::new()
        .inner_margin(Margin::symmetric(20, 14))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(img(icon, 16.0, icon_color));
                ui.label(
                    RichText::new(title)
                        .font(FontId::new(14.5, theme::bold()))
                        .color(p.text),
                );
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    close = small_icon_btn(ui, p, theme::X, p.muted, "닫기 (Esc)").clicked();
                });
            });
        });
    hline(ui, p);
    close
}

fn footer(ui: &mut egui::Ui, p: &Pal, fill: Option<Color32>, add: impl FnOnce(&mut egui::Ui)) {
    hline(ui, p);
    Frame::new()
        .fill(fill.unwrap_or(Color32::TRANSPARENT))
        .corner_radius(egui::CornerRadius {
            nw: 0,
            ne: 0,
            sw: 14,
            se: 14,
        })
        .inner_margin(Margin::symmetric(20, 12))
        .show(ui, |ui| {
            ui.spacing_mut().button_padding = vec2(14.0, 6.0);
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), 30.0),
                egui::Layout::right_to_left(Align::Center),
                add,
            );
        });
}

// 확인 대화상자 머리: 색 배경 아이콘 상자 + 제목 (웹의 dialog-icon-wrap)
fn dialog_title(ui: &mut egui::Ui, p: &Pal, color: Color32, title: &str) {
    Frame::new()
        .inner_margin(Margin {
            left: 20,
            right: 20,
            top: 18,
            bottom: 8,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
                ui.painter().rect_filled(r, 8.0, color.gamma_multiply(0.15));
                img(theme::ALERT, 18.0, color)
                    .paint_at(ui, Rect::from_center_size(r.center(), vec2(18.0, 18.0)));
                ui.add_space(4.0);
                ui.label(
                    RichText::new(title)
                        .font(FontId::new(15.0, theme::bold()))
                        .color(p.text),
                );
            });
        });
}

fn alerts(dark: bool) -> egui_commonmark::AlertBundle {
    use egui_commonmark::Alert;
    let c = |d: (u8, u8, u8), l: (u8, u8, u8)| {
        let (r, g, b) = if dark { d } else { l };
        Color32::from_rgb(r, g, b)
    };
    let mk = |accent, icon, id: &str, shown: &str| Alert {
        accent_color: accent,
        icon,
        identifier: id.into(),
        identifier_rendered: shown.into(),
    };
    egui_commonmark::AlertBundle::from_alerts(vec![
        mk(
            c((0x38, 0xbd, 0xf8), (0x02, 0x84, 0xc7)),
            '●',
            "NOTE",
            "Note",
        ),
        mk(c((0x10, 0xb9, 0x81), (0x05, 0x96, 0x69)), '★', "TIP", "Tip"),
        mk(
            c((0x8b, 0x5c, 0xf6), (0x7c, 0x3a, 0xed)),
            '◆',
            "IMPORTANT",
            "Important",
        ),
        mk(
            c((0xf5, 0x9e, 0x0b), (0xd9, 0x77, 0x06)),
            '▲',
            "WARNING",
            "Warning",
        ),
        mk(
            c((0xef, 0x44, 0x44), (0xdc, 0x26, 0x26)),
            '■',
            "CAUTION",
            "Caution",
        ),
    ])
}

fn highlight_lang_alias(lang: &str) -> &str {
    match lang.trim().to_ascii_lowercase().as_str() {
        "" => "",
        "ps1" | "powershell" | "pwsh" | "bat" | "cmd" => "sh",
        "go" | "golang" | "java" | "kt" | "kotlin" | "cs" | "csharp" | "swift" | "dart" => "cpp",
        "zig" => "rs",
        _ => lang.trim(),
    }
}

fn estimate_block_height(src: &str, body: f32) -> f32 {
    let t = src.trim_start();
    if let Some(lv) = heading_level(t) {
        return HEADING_TOP[lv - 1] + body * HEADING_SCALE[lv - 1] * 1.45;
    }
    let lines = (src.bytes().filter(|&b| b == b'\n').count() + 1) as f32;
    if t.starts_with("```") || t.starts_with("~~~") {
        (lines * body * 1.26 + 56.0).max(80.0)
    } else {
        (lines * body * 1.6 + 4.0).max(body * 1.6)
    }
}

fn cache_hash(val: impl std::hash::Hash) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    val.hash(&mut h);
    h.finish()
}

#[allow(clippy::too_many_arguments)]
fn build_code_layout_job(
    ctx: &egui::Context,
    code: &str,
    lang: &str,
    font_size: f32,
    wrap_w: f32,
    line_height: f32,
    p: &Pal,
    dark: bool,
) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, LayoutSection, TextFormat};
    let alias = highlight_lang_alias(lang);
    let font_id = FontId::new(font_size, FontFamily::Monospace);
    if alias.eq_ignore_ascii_case("sas") {
        let mut job = crate::sas::highlight(code, font_id, line_height, dark, p.text);
        job.wrap.max_width = wrap_w;
        return job;
    }
    let mut job = if alias.is_empty() {
        LayoutJob::default()
    } else {
        let code_theme = if dark {
            egui_extras::syntax_highlighting::CodeTheme::dark(font_size)
        } else {
            egui_extras::syntax_highlighting::CodeTheme::light(font_size)
        };
        let style = egui::Style {
            override_font_id: Some(font_id.clone()),
            ..Default::default()
        };
        let mut j =
            egui_extras::syntax_highlighting::highlight(ctx, &style, &code_theme, code, alias);
        let supported = matches!(
            alias.to_ascii_lowercase().as_str(),
            "c" | "h"
                | "hpp"
                | "cpp"
                | "c++"
                | "js"
                | "javascript"
                | "jsx"
                | "mjs"
                | "ts"
                | "typescript"
                | "tsx"
                | "json"
                | "jsonc"
                | "json5"
                | "py"
                | "python"
                | "rs"
                | "rust"
                | "bash"
                | "sh"
                | "shell"
                | "zsh"
                | "sql"
                | "toml"
                | "yaml"
                | "yml"
        );
        if !supported {
            j.sections.clear();
        } else {
            for sec in &mut j.sections {
                sec.format.font_id = font_id.clone();
                sec.format.line_height = Some(line_height);
            }
        }
        j
    };
    if job.sections.is_empty() && !code.is_empty() {
        job.text = code.to_string();
        job.sections.push(LayoutSection {
            leading_space: 0.0,
            byte_range: egui::text::ByteIndex(0)..egui::text::ByteIndex(code.len()),
            format: TextFormat {
                font_id,
                color: p.text,
                line_height: Some(line_height),
                ..Default::default()
            },
        });
    }
    job.wrap.max_width = wrap_w;
    job
}

// ==강조== 배경 (편집·보기 화면 공용)
fn mark_bg() -> Color32 {
    Color32::from_rgba_unmultiplied(0xf5, 0x9e, 0x0b, 64)
}

// 편집 라인 내 인라인 마크다운(**굵게**, __굵게__, *기울임*, ~~취소선~~, ==강조==, `코드`) 강조
// 바이트 길이를 100% 보존하여 TextEdit 커서·IME 위치가 어긋나지 않는다
fn append_inline_md_source(
    job: &mut egui::text::LayoutJob,
    line: &str,
    size: f32,
    p: &Pal,
) {
    use egui::text::TextFormat;
    let mk_fmt = |sz: f32, family: FontFamily, color: Color32| TextFormat {
        font_id: FontId::new(sz, family),
        color,
        line_height: Some(size * 1.6),
        ..Default::default()
    };
    // 서식 기호(** ` ~~ == *)는 폭 0·투명으로 숨긴다. 텍스트에는 남아 있어 복사·저장은 마크다운 그대로.
    let hidden = TextFormat {
        font_id: FontId::new(0.1, FontFamily::Proportional),
        color: Color32::TRANSPARENT,
        line_height: Some(size * 1.6),
        ..Default::default()
    };
    let mut at = 0usize;
    for (s, bs, be, e, kind) in inline_spans(line) {
        if at < s {
            job.append(&line[at..s], 0.0, mk_fmt(size, FontFamily::Proportional, p.text));
        }
        job.append(&line[s..bs], 0.0, hidden.clone());
        let f = match kind {
            Inline::Code => {
                let mut f = mk_fmt(size * 0.9, FontFamily::Monospace, p.text);
                f.background = p.code_bg;
                f
            }
            Inline::Bold => mk_fmt(size, theme::bold(), p.text),
            Inline::Mark => {
                let mut f = mk_fmt(size, theme::bold(), p.text);
                f.background = mark_bg();
                f
            }
            Inline::Strike => {
                let mut f = mk_fmt(size, FontFamily::Proportional, p.muted);
                f.strikethrough = Stroke::new(1.0, p.muted);
                f
            }
            Inline::Italic => {
                let mut f = mk_fmt(size, FontFamily::Proportional, p.text);
                f.italics = true;
                f
            }
        };
        job.append(&line[bs..be], 0.0, f);
        job.append(&line[be..e], 0.0, hidden.clone());
        at = e;
    }
    if at < line.len() {
        job.append(&line[at..], 0.0, mk_fmt(size, FontFamily::Proportional, p.text));
    }
}

// 편집 중인 블록의 원문 서식: 제목은 크기·굵기, 코드는 고정폭+구문 강조, 기호는 흐리게
pub(crate) fn md_job(
    ctx: &egui::Context,
    text: &str,
    body: f32,
    p: &Pal,
    dark: bool,
) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, TextFormat};
    let fmt = |size: f32, family: FontFamily, color: Color32| TextFormat {
        font_id: FontId::new(size, family),
        color,
        line_height: Some(size * 1.6),
        ..Default::default()
    };
    let mut job = LayoutJob::default();
    let trimmed = text.trim_start();
    if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
        let code_size = body * 0.86;
        let code_lh = code_size * 1.55;
        let fence = &trimmed[..3];
        let lead = text.len() - trimmed.len();
        let line0_end = text.find('\n').map_or(text.len(), |i| i + 1);
        let line0_body = text[lead..line0_end].trim_end_matches(['\n', '\r']);
        let lang_str = line0_body.get(3..).unwrap_or("").trim();
        // ``` 줄은 접어서 숨긴다 (언어는 카드 머리에 표시). 텍스트에는 남아 있어 복사·저장은 마크다운 그대로.
        let hidden = TextFormat {
            font_id: FontId::new(0.1, FontFamily::Monospace),
            color: Color32::TRANSPARENT,
            line_height: Some(0.0),
            ..Default::default()
        };
        job.append(&text[..line0_end], 0.0, hidden.clone());
        if line0_end < text.len() {
            let rest = &text[line0_end..];
            let last_line_start = rest
                .trim_end_matches(['\n', '\r'])
                .rfind('\n')
                .map_or(0, |i| i + 1);
            let close_start = if rest[last_line_start..].trim_start().starts_with(fence) {
                line0_end + last_line_start
            } else {
                text.len()
            };
            let body_slice = &text[line0_end..close_start];
            if !body_slice.is_empty() {
                let body_job = build_code_layout_job(
                    ctx,
                    body_slice,
                    lang_str,
                    code_size,
                    f32::INFINITY,
                    code_lh,
                    p,
                    dark,
                );
                let offset = job.text.len();
                job.text.push_str(&body_job.text);
                for mut sec in body_job.sections {
                    sec.byte_range.start += offset;
                    sec.byte_range.end += offset;
                    job.sections.push(sec);
                }
            }
            if close_start < text.len() {
                job.append(&text[close_start..], 0.0, hidden);
            }
        }
        debug_assert_eq!(job.text, text);
        return job;
    }

    let mut fence = false;
    let mut list_stack = Vec::new();
    for line in text.split_inclusive('\n') {
        let t = line.trim_start();
        let is_fence = t.starts_with("```") || t.starts_with("~~~");
        if is_fence || fence {
            let c = if is_fence { p.faint } else { p.text };
            job.append(line, 0.0, fmt(body * 0.86, FontFamily::Monospace, c));
            fence ^= is_fence;
            continue;
        }
        if let Some(level) = heading_level(t) {
            let size = body * HEADING_SCALE[level - 1];
            let m = (line.len() - t.len() + level + 1).min(line.len());
            // "## " 기호는 폭 0·투명으로 숨긴다 (노션식). 줄 높이는 제목 크기 그대로
            let mut hf = fmt(0.1, theme::bold(), Color32::TRANSPARENT);
            hf.line_height = Some(size * 1.6);
            job.append(&line[..m], 0.0, hf);
            job.append(&line[m..], 0.0, fmt(size, theme::bold(), p.text));
            continue;
        }
        let bare = line.trim_end_matches(['\n', '\r']);
        if let (Some(len), Some((depth, _))) =
            (list_marker_bytes(bare), list_item_step(&mut list_stack, bare))
        {
            // 노션식: 들여쓰기·기호(- / 1. / - [ ])는 투명하게 두고 렌더링 화면과 같은 자리에서 내용이 시작한다.
            // 기호 자리의 글머리·번호·체크박스는 paint_list_markers가 그린다. 텍스트는 마크다운 그대로.
            let len = len.min(line.len());
            let lh = body * 1.6 + LIST_ROW_GAP;
            // 숨긴 기호는 작은 글자로 둬서 "- [ ] "처럼 긴 기호도 내용 열 안에 들어가게 한다
            let hidden_size = body * 0.4;
            let font = FontId::new(hidden_size, FontFamily::Proportional);
            let w = ctx
                .fonts_mut(|f| {
                    f.layout_no_wrap(line[..len].to_owned(), font, Color32::TRANSPARENT)
                })
                .size()
                .x;
            let content_x = LIST_STEP * (depth + 1) as f32;
            let mut f = fmt(hidden_size, FontFamily::Proportional, Color32::TRANSPARENT);
            f.line_height = Some(lh);
            f.valign = Align::TOP;
            job.append(&line[..len], (content_x - w).max(0.0), f);
            let s0 = job.sections.len();
            append_inline_md_source(&mut job, &line[len..], body, p);
            for sec in &mut job.sections[s0..] {
                sec.format.line_height = Some(lh);
                sec.format.valign = Align::TOP;
            }
            continue;
        }
        let marker = if let Some(rest) = t.strip_prefix('>') {
            line.len() - t.len() + 1 + usize::from(rest.starts_with(' '))
        } else {
            0
        };
        let marker = marker.min(line.len());
        if marker > 0 {
            job.append(
                &line[..marker],
                0.0,
                fmt(body, FontFamily::Proportional, p.faint),
            );
        }
        append_inline_md_source(&mut job, &line[marker..], body, p);
    }
    debug_assert_eq!(job.text, text);
    job
}

// 목록 배치 (렌더링·편집 화면 공용): [여백 PAD][기호 칸 MARKER_W][GAP] 내용. 한 단계 = LIST_STEP
const LIST_PAD: f32 = 4.0;
const LIST_MARKER_W: f32 = 18.0;
const LIST_GAP: f32 = 6.0;
const LIST_STEP: f32 = LIST_PAD + LIST_MARKER_W + LIST_GAP;
const LIST_ROW_GAP: f32 = 6.0; // 렌더링 화면의 항목 사이 간격(item_spacing.y)
const BULLETS: [&str; 3] = ["•", "◦", "▪"];

// egui 기본 체크박스와 같은 모양 (렌더링·편집 화면 공용)
fn paint_checkbox(painter: &egui::Painter, c: egui::Pos2, checked: bool) {
    let style = painter.ctx().global_style();
    let w = &style.visuals.widgets.inactive;
    let r = Rect::from_center_size(c, egui::Vec2::splat(style.spacing.icon_width));
    painter.rect(r, w.corner_radius, w.bg_fill, w.bg_stroke, egui::StrokeKind::Inside);
    if checked {
        let r = r.shrink(3.0);
        painter.add(egui::Shape::line(
            vec![
                pos2(r.left(), r.center().y),
                pos2(r.center().x, r.bottom()),
                pos2(r.right(), r.top()),
            ],
            w.fg_stroke,
        ));
    }
}

// 편집 중인 블록에서 투명하게 둔 목록 기호 자리에 렌더링 화면과 같은 글머리·번호·체크박스를 그린다
fn paint_list_markers(
    painter: &egui::Painter,
    g: &egui::Galley,
    origin: egui::Pos2,
    text: &str,
    body: f32,
    p: &Pal,
) {
    use egui::text::CCursor;
    let mut stack = Vec::new();
    let (mut ci, mut fence) = (0, false);
    for line in text.split('\n') {
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = !fence;
        } else if !fence {
            if let (Some(len), Some((depth, num))) =
                (list_marker_bytes(line), list_item_step(&mut stack, line))
            {
                let row_top = g.pos_from_cursor(CCursor::new(ci + len)).top();
                let c = origin
                    + vec2(
                        LIST_STEP * depth as f32 + LIST_PAD + LIST_MARKER_W * 0.5,
                        row_top + body * 0.75,
                    );
                let m = line[..len].trim_start();
                if m.ends_with("] ") {
                    paint_checkbox(painter, c, m.contains(['x', 'X']));
                } else {
                    let label =
                        num.map_or_else(|| BULLETS[depth % 3].to_string(), |n| format!("{n}."));
                    painter.text(
                        c,
                        Align2::CENTER_CENTER,
                        label,
                        FontId::proportional(body),
                        p.muted,
                    );
                }
            }
        }
        ci += line.chars().count() + 1;
    }
}

struct BlockOut {
    rect: Rect,
    changed: Option<String>,
    consumed: bool,
}

#[allow(clippy::too_many_arguments)]
fn editor_galley_cached(
    ctx: &egui::Context,
    cache: &mut Option<(u64, u32, u32, bool, std::sync::Arc<egui::Galley>)>,
    s: &str,
    body: f32,
    wrap: f32,
    pal: &Pal,
    dark: bool,
    ppp_bits: u32,
) -> std::sync::Arc<egui::Galley> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    ppp_bits.hash(&mut hasher);
    s.hash(&mut hasher);
    let key = (hasher.finish(), body.to_bits(), wrap.to_bits(), dark);
    if let Some((ch, cb, cw, cd, g)) = cache.as_ref() {
        if (*ch, *cb, *cw, *cd) == key && g.text() == s {
            return std::sync::Arc::clone(g);
        }
    }
    let mut job = md_job(ctx, s, body, pal, dark);
    job.wrap.max_width = wrap;
    let g = ctx.fonts_mut(|f| f.layout_job(job));
    *cache = Some((key.0, key.1, key.2, key.3, std::sync::Arc::clone(&g)));
    g
}

struct EditorOut {
    rect: Rect,
    changed: Option<String>,
    fmt: Option<Fmt>,
    slash: Option<usize>,
    interacted: bool,
    deactivate: bool,
    delete_block: bool,
}

impl App {
    fn title_bar(&mut self, ctx: &egui::Context) {
        let title = match self.current.as_deref() {
            Some(n) if is_scratch_key(n) => format!("{} — MDEditor", self.tab_title(n)),
            Some(n) if self.docs.get(n).is_some_and(Doc::dirty) => format!("● {n} — MDEditor"),
            Some(n) => format!("{n} — MDEditor"),
            None => "MDEditor".into(),
        };
        if title != self.last_title {
            self.last_title = title.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
    }

    // ---------- 상단 바 (웹: 항상 왼쪽 정렬된 한 줄) ----------
    fn header(&mut self, ui: &mut egui::Ui, p: &Pal) {
        let ctx = ui.ctx().clone();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
            ui.painter().rect_filled(r, 6.0, p.accent);
            img(theme::BRAND, 13.0, Color32::WHITE)
                .paint_at(ui, Rect::from_center_size(r.center(), vec2(13.0, 13.0)));
            ui.add_space(5.0);
            ui.label(
                RichText::new("MDEditor")
                    .font(FontId::new(14.0, theme::bold()))
                    .color(p.text),
            );
            ui.add_space(7.0);
            divider(ui, p);
            ui.add_space(7.0);

            let name = self.current.clone();
            let is_scratch = self.is_scratch();
            let dirty = !is_scratch && self.doc().is_some_and(Doc::dirty);
            let display_name = match name.as_deref() {
                None => std::borrow::Cow::Borrowed("새 문서"),
                Some(n) => self.tab_title(n),
            };
            let badge_tip = if is_scratch {
                "클릭하여 파일로 저장 (Ctrl+S)"
            } else {
                "클릭하여 파일 이름 변경 (F2)"
            };
            let badge = Frame::new()
                .fill(p.elevated)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(8)
                .inner_margin(Margin::symmetric(10, 4))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 7.0;
                    ui.set_max_width(270.0);
                    ui.add(img(theme::FILE, 14.0, p.muted));
                    ui.add(
                        egui::Label::new(
                            RichText::new(display_name.as_ref())
                                .size(12.5)
                                .color(p.text),
                        )
                        .truncate(),
                    );
                    if is_scratch {
                        Frame::new()
                            .fill(p.accent.gamma_multiply(0.12))
                            .corner_radius(4)
                            .inner_margin(Margin::symmetric(5, 1))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new("임시 메모 · 저장").size(10.5).color(p.accent),
                                );
                            });
                    } else if dirty {
                        dot(ui, AMBER, 3.5, false);
                    }
                })
                .response
                .interact(Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_text(badge_tip);
            if badge.clicked() {
                if is_scratch {
                    self.dialog = Some(Dialog::SaveScratch(String::new()));
                } else if let Some(n) = &name {
                    self.ask_rename(n);
                }
            }
            ui.add_space(7.0);
            divider(ui, p);
            ui.add_space(7.0);

            if icon_btn(ui, p, theme::FOLDER, "작업 폴더 파일 탐색기 (Ctrl+O)").clicked() {
                self.open_files();
            }
            if icon_btn(
                ui,
                p,
                theme::UPLOAD,
                "로컬 파일 열기 — OS 탐색기 (Ctrl+Alt+O)",
            )
            .clicked()
            {
                self.open_local_file();
            }
            if icon_btn(
                ui,
                p,
                theme::PLUS,
                "새 탭 열기 (Ctrl+N / 새 파일 Ctrl+Shift+N)",
            )
            .clicked()
            {
                self.new_tab();
            }
            if icon_btn(ui, p, theme::SAVE, "문서 저장 (Ctrl+S)").clicked() {
                self.trigger_save();
            }
            ui.add_space(6.0);
            divider(ui, p);
            ui.add_space(6.0);
            if icon_btn(ui, p, theme::UNDO, "실행 취소 (Ctrl+Z)").clicked() {
                self.history(true);
            }
            if icon_btn(ui, p, theme::REDO, "다시 실행 (Ctrl+Y)").clicked() {
                self.history(false);
            }
            ui.add_space(6.0);
            divider(ui, p);
            ui.add_space(6.0);
            if icon_btn(ui, p, theme::TABLE, "표 (Table) 삽입 (Ctrl+Alt+T)").clicked() {
                self.insert_block_template("| 제목 | 내용 |\n| --- | --- |\n|  |  |", 2);
            }
            if icon_btn(ui, p, theme::IMAGE, "이미지 삽입 (Ctrl+Shift+I)").clicked() {
                self.dialog = Some(Dialog::Image {
                    url_tab: false,
                    url: String::new(),
                    alt: String::new(),
                });
            }
            if toggle_icon_btn(
                ui,
                p,
                theme::LIST,
                self.toc_open,
                "문서 목차 (TOC) 열기/닫기 (Ctrl+Shift+L)",
            )
            .clicked()
            {
                self.toc_open = !self.toc_open;
            }
            if toggle_icon_btn(
                ui,
                p,
                theme::SEARCH,
                self.find.open,
                "본문 찾기 및 바꾸기 (Ctrl+F / Ctrl+H)",
            )
            .clicked()
            {
                if self.find.open {
                    self.find.open = false;
                    if self.active.is_some() {
                        self.focus_editor = true;
                    }
                } else {
                    self.open_find(&ctx, false);
                }
            }
            ui.add_space(6.0);
            divider(ui, p);
            ui.add_space(6.0);
            if icon_btn(ui, p, theme::PRINTER, "PDF 인쇄 (Ctrl+P)").clicked() {
                self.print(&ctx);
            }
            if icon_btn(
                ui,
                p,
                theme::DOWNLOAD,
                "HTML 파일로 내보내기 (Ctrl+Shift+E)",
            )
            .clicked()
            {
                self.export();
            }
            if toggle_icon_btn(
                ui,
                p,
                theme::WIDTH,
                self.full_width,
                if self.full_width {
                    "전체 너비 켜짐 — 클릭하여 가운데 보기로 전환 (Ctrl+Alt+W)"
                } else {
                    "가운데 보기 — 클릭하여 전체 너비로 크게 보기 (Ctrl+Alt+W)"
                },
            )
            .clicked()
            {
                self.full_width = !self.full_width;
            }
            if toggle_icon_btn(
                ui,
                p,
                theme::MAXIMIZE,
                self.focus_mode,
                "집중 크게 보기 — 상단·하단 바 숨기기 (F11)",
            )
            .clicked()
            {
                self.focus_mode = !self.focus_mode;
            }
            let (theme_ic, theme_tip) = if self.dark {
                (theme::SUN, "라이트 모드로 전환 (Ctrl+Alt+D)")
            } else {
                (theme::MOON, "다크 모드로 전환 (Ctrl+Alt+D)")
            };
            if icon_btn(ui, p, theme_ic, theme_tip).clicked() {
                self.dark = !self.dark;
                theme::apply(&ctx, self.dark);
            }
            let settings = icon_btn(ui, p, theme::SETTINGS, "설정");
            egui::Popup::menu(&settings).show(|ui| self.settings_menu(ui, p, &ctx));
        });
    }

    fn tab_bar(&mut self, ui: &mut egui::Ui, p: &Pal) {
        let active_key = self.current.as_deref();
        let scroll_active = std::mem::take(&mut self.scroll_tab_to_active);
        let pointer_pos = ui.ctx().input(|i| i.pointer.interact_pos());
        let mut switch_to: Option<String> = None;
        let mut close_target: Option<String> = None;
        let mut context_tab: Option<(String, egui::Pos2)> = None;
        let mut add_new_tab = false;
        let mut focus_current = false;

        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let mut active_tab_x: Option<egui::Rangef> = None;
            egui::ScrollArea::horizontal()
                .id_salt("tab-bar")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for idx in 0..self.tabs.len() {
                            let key = &self.tabs[idx];
                            let is_active = active_key == Some(key.as_str());
                            let dirty = self
                                .docs
                                .get(key)
                                .is_some_and(|d| !is_scratch_key(key) && d.dirty());
                            let title = self.tab_title(key);
                            let fill = if is_active {
                                p.bg
                            } else {
                                p.elevated.gamma_multiply(0.45)
                            };
                            let stroke = if is_active {
                                Stroke::new(1.0, p.border)
                            } else {
                                Stroke::NONE
                            };
                            let frame_corner = egui::CornerRadius {
                                nw: 7,
                                ne: 7,
                                sw: 0,
                                se: 0,
                            };
                            let hover_bg_idx = (!is_active).then(|| ui.painter().add(egui::Shape::Noop));
                            let frame_out = Frame::new()
                                .fill(fill)
                                .stroke(stroke)
                                .corner_radius(frame_corner)
                                .inner_margin(Margin {
                                    left: 11,
                                    right: 6,
                                    top: 5,
                                    bottom: 5,
                                })
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 5.0;
                                        let body_resp = ui
                                            .horizontal(|ui| {
                                                ui.set_max_width(160.0);
                                                ui.add(img(
                                                    theme::FILE,
                                                    12.5,
                                                    if is_active { p.accent } else { p.muted },
                                                ));
                                                ui.add(
                                                    egui::Label::new(
                                                        RichText::new(title.as_ref())
                                                            .size(12.0)
                                                            .color(if is_active {
                                                                p.text
                                                            } else {
                                                                p.muted
                                                            }),
                                                    )
                                                    .truncate(),
                                                );
                                                if dirty {
                                                    dot(ui, AMBER, 3.0, false);
                                                }
                                            })
                                            .response
                                            .interact(Sense::click())
                                            .on_hover_cursor(CursorIcon::PointingHand);
                                        let body_resp = if body_resp.hovered() {
                                            let tip = if is_scratch_key(key) {
                                                let preview = self
                                                    .docs
                                                    .get(key)
                                                    .and_then(|d| {
                                                        d.text
                                                            .lines()
                                                            .map(str::trim)
                                                            .find(|l| !l.is_empty())
                                                    })
                                                    .unwrap_or("빈 임시 메모");
                                                format!("{title} — {preview}\n(우클릭: 탭 메뉴, Ctrl+S: 저장, Ctrl+W: 닫기)")
                                            } else {
                                                format!("{key}\n(우클릭: 탭 메뉴, 가운데 클릭 / Ctrl+W: 닫기)")
                                            };
                                            body_resp.on_hover_text(tip)
                                        } else {
                                            body_resp
                                        };
                                        let close_resp = small_icon_btn(
                                            ui,
                                            p,
                                            theme::X,
                                            p.muted,
                                            "탭 닫기 (Ctrl+W)",
                                        );
                                        (body_resp, close_resp)
                                    })
                                    .inner
                                });
                            let (body_resp, close_resp) = frame_out.inner;
                            if is_active {
                                let r = frame_out.response.rect;
                                active_tab_x = Some(r.x_range());
                                ui.painter().line_segment(
                                    [
                                        pos2(r.left() + 7.0, r.top() + 1.0),
                                        pos2(r.right() - 7.0, r.top() + 1.0),
                                    ],
                                    Stroke::new(2.0, p.accent),
                                );
                                if scroll_active {
                                    frame_out.response.scroll_to_me(Some(Align::Center));
                                }
                            } else if body_resp.hovered() {
                                if let Some(idx_bg) = hover_bg_idx {
                                    ui.painter().set(
                                        idx_bg,
                                        egui::Shape::rect_filled(
                                            frame_out.response.rect,
                                            frame_corner,
                                            p.hover.gamma_multiply(0.55),
                                        ),
                                    );
                                }
                            }
                            if body_resp.secondary_clicked() {
                                let pos =
                                    pointer_pos.unwrap_or_else(|| frame_out.response.rect.left_bottom());
                                context_tab = Some((key.clone(), pos));
                            } else if close_resp.clicked() || body_resp.middle_clicked() {
                                close_target = Some(key.clone());
                            } else if body_resp.clicked() {
                                if is_active {
                                    focus_current = true;
                                } else {
                                    switch_to = Some(key.clone());
                                }
                            }
                        }
                        ui.add_space(2.0);
                        if small_icon_btn(
                            ui,
                            p,
                            theme::PLUS,
                            p.muted,
                            "새 탭 추가 (Ctrl+N / Ctrl+T)",
                        )
                        .clicked()
                        {
                            add_new_tab = true;
                        }
                        let rest = ui.available_width().max(24.0);
                        let (_, empty_resp) =
                            ui.allocate_exact_size(vec2(rest, 26.0), Sense::click());
                        if empty_resp.double_clicked() {
                            add_new_tab = true;
                        }
                    });
                });
            let (hr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            let full_x = hr.expand2(vec2(12.0, 0.0)).x_range();
            ui.painter()
                .hline(full_x, hr.center().y, Stroke::new(1.0, p.border));
            if let Some(ax) = active_tab_x {
                // 활성 탭 아래 선은 캔버스 색으로 덮어 탭이 본문과 자연스럽게 이어지게 한다
                ui.painter().hline(
                    egui::Rangef::new(ax.min + 1.0, ax.max - 1.0),
                    hr.center().y,
                    Stroke::new(2.0, p.bg),
                );
            }
        });

        if let Some((key, pos)) = context_tab {
            self.context_menu = Some(ContextMenu::Tab(key, pos));
        } else if add_new_tab {
            self.context_menu = None;
            self.new_tab();
        } else if let Some(key) = close_target {
            self.context_menu = None;
            self.close_tab(&key);
        } else if let Some(key) = switch_to {
            self.context_menu = None;
            self.switch_to_tab(&key);
        } else if focus_current && self.active.is_some() {
            self.context_menu = None;
            self.focus_editor = true;
        }
    }

    fn settings_menu(&mut self, ui: &mut egui::Ui, p: &Pal, ctx: &egui::Context) {
        ui.set_width(248.0);
        ui.spacing_mut().item_spacing.y = 1.0;
        let ic = |i: Icon| Some((i, p.muted));
        let theme_label = if self.dark {
            "라이트 모드로 전환"
        } else {
            "다크 모드로 전환"
        };
        if menu_row(ui, p, ic(theme::SUN), theme_label, "Ctrl+Alt+D", p.text).clicked() {
            self.dark = !self.dark;
            theme::apply(ctx, self.dark);
        }
        let fw = format!(
            "본문 너비: {}",
            if self.full_width {
                "전체 너비 (크게)"
            } else {
                "가운데 (기본)"
            }
        );
        if menu_row(ui, p, ic(theme::WIDTH), &fw, "Ctrl+Alt+W", p.text).clicked() {
            self.full_width = !self.full_width;
        }
        let fm = format!(
            "집중 크게 보기: {}",
            if self.focus_mode { "켬" } else { "끔" }
        );
        if menu_row(ui, p, ic(theme::MAXIMIZE), &fm, "F11", p.text).clicked() {
            self.focus_mode = !self.focus_mode;
            ui.close();
        }
        let fs = format!(
            "글꼴 크기: {}",
            if self.small_text { "작게" } else { "보통" }
        );
        if menu_row(ui, p, ic(theme::TYPE), &fs, "Ctrl+Alt+F", p.text).clicked() {
            self.small_text = !self.small_text;
        }
        let font_lbl = format!("글꼴: {}", theme::font_display_name(&self.font_family));
        ui.menu_button(
            RichText::new(format!("  {font_lbl}"))
                .size(12.5)
                .color(p.text),
            |ui| {
                self.font_picker_menu(ui, ctx, p);
            },
        );
        if menu_row(
            ui,
            p,
            ic(theme::COPY),
            "마크다운 원본 복사",
            "Ctrl+Shift+C",
            p.text,
        )
        .clicked()
        {
            self.copy_markdown_source(ctx);
            ui.close();
        }
        if menu_row(
            ui,
            p,
            ic(theme::CODE),
            "HTML 코드 복사",
            "Ctrl+Alt+C",
            p.text,
        )
        .clicked()
        {
            self.copy_html_code(ctx);
            ui.close();
        }
        ui.add_space(4.0);
        hline(ui, p);
        ui.add_space(4.0);
        if menu_row(ui, p, ic(theme::HELP), "도움말 및 단축키", "F1", p.text).clicked() {
            self.dialog = Some(Dialog::Help);
            ui.close();
        }
        ui.add_space(4.0);
        hline(ui, p);
        ui.add_space(4.0);
        if menu_row(ui, p, ic(theme::EXIT), "프로그램 종료", "Ctrl+Q", RED).clicked() {
            self.guarded(After::Close);
            ui.close();
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(
                RichText::new(concat!("MDEditor v", env!("CARGO_PKG_VERSION")))
                    .size(11.0)
                    .color(p.faint),
            );
        });
        ui.add_space(2.0);
    }

    // ---------- 하단 상태바 ----------
    fn status_bar(&mut self, ui: &mut egui::Ui, p: &Pal) {
        let mut do_save = false;
        let mut do_open_files = false;
        let mut toggle_width = false;
        let mut toggle_font = false;
        let folder_name = self.root_label().to_string();
        let stats = self
            .doc()
            .map(|d| (d.chars, d.lines, self.selected_chars()));

        ui.horizontal(|ui| {
            let (label, color, ring) = match self.doc() {
                None => ("준비됨", GREEN, false),
                Some(d) if d.scratch => {
                    if d.text.is_empty() {
                        ("새 문서", GREEN, false)
                    } else if d.dirty() {
                        ("임시 보관 중...", AMBER, true)
                    } else {
                        ("자동 보관됨", GREEN, false)
                    }
                }
                Some(d) if d.dirty() => ("저장되지 않음", AMBER, true),
                Some(_) => ("저장됨", GREEN, false),
            };
            let pill = Frame::new()
                .fill(p.elevated)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(255)
                .inner_margin(Margin::symmetric(9, 2))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    dot(ui, color, 3.0, ring);
                    ui.label(RichText::new(label).size(12.0).color(p.muted));
                })
                .response
                .interact(Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_text("클릭하여 저장 (Ctrl+S)");
            if pill.clicked() && stats.is_some() {
                do_save = true;
            }

            ui.add_space(4.0);
            let mut folder_pill = Frame::new()
                .fill(Color32::TRANSPARENT)
                .corner_radius(6)
                .inner_margin(Margin::symmetric(6, 2))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.add(img(theme::FOLDER, 12.0, p.faint));
                    ui.label(RichText::new(&folder_name).size(11.5).color(p.faint));
                })
                .response
                .interact(Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            if folder_pill.hovered() {
                folder_pill = folder_pill.on_hover_text(format!(
                    "작업 폴더: {}\n클릭하여 파일 탐색기 열기 (Ctrl+O)",
                    self.root.display()
                ));
            }
            if folder_pill.clicked() {
                do_open_files = true;
            }

            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if let Some((chars, lines, sel_len)) = stats {
                    ui.label(
                        RichText::new(format!(
                            "{chars}자 · {lines}줄 · ~{}분",
                            chars.div_ceil(500).max(1)
                        ))
                        .family(FontFamily::Monospace)
                        .size(11.5)
                        .color(p.faint),
                    );
                    if sel_len > 0 {
                        ui.label(
                            RichText::new(format!("{sel_len}자 선택됨"))
                                .family(FontFamily::Monospace)
                                .size(11.5)
                                .color(p.faint),
                        );
                    }
                    divider(ui, p);
                }
                let pill_btn = |ui: &mut egui::Ui, text: &str, active: bool, tip: &str| {
                    ui.add(
                        egui::Button::new(RichText::new(text).size(11.5).color(if active {
                            p.accent
                        } else {
                            p.muted
                        }))
                        .fill(if active {
                            p.accent.gamma_multiply(0.12)
                        } else {
                            Color32::TRANSPARENT
                        })
                        .stroke(if active {
                            Stroke::new(1.0, p.accent.gamma_multiply(0.32))
                        } else {
                            Stroke::NONE
                        })
                        .corner_radius(6)
                        .min_size(vec2(0.0, 22.0)),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .on_hover_text(tip)
                };
                let font_family_btn = pill_btn(
                    ui,
                    &format!("글꼴: {}", theme::font_display_name(&self.font_family)),
                    false,
                    "클릭하여 글꼴 선택",
                );
                egui::Popup::menu(&font_family_btn).show(|ui| {
                    let ctx = ui.ctx().clone();
                    self.font_picker_menu(ui, &ctx, p);
                });
                let font_btn = pill_btn(
                    ui,
                    if self.small_text {
                        "크기: 작게"
                    } else {
                        "크기: 보통"
                    },
                    self.small_text,
                    "클릭하여 글꼴 크기 전환 (Ctrl+Alt+F)",
                );
                if font_btn.clicked() {
                    toggle_font = true;
                }
                let width_btn = pill_btn(
                    ui,
                    if self.full_width {
                        "전체 너비"
                    } else {
                        "가운데"
                    },
                    self.full_width,
                    "클릭하여 가운데 ↔ 전체 너비(크게 보기) 전환 (Ctrl+Alt+W)",
                );
                if width_btn.clicked() {
                    toggle_width = true;
                }
            });
        });

        if do_save {
            self.trigger_save();
        } else if do_open_files {
            self.open_files();
        } else if toggle_width {
            self.full_width = !self.full_width;
        } else if toggle_font {
            self.small_text = !self.small_text;
        }
    }

    fn font_picker_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, p: &Pal) {
        ui.set_width(240.0);
        ui.spacing_mut().item_spacing.y = 2.0;
        let mut chosen: Option<String> = None;
        for (id, label) in theme::available_fonts() {
            let selected = self.font_family == *id || (self.font_family.is_empty() && id == "auto");
            if ui
                .selectable_label(selected, RichText::new(label).size(12.0))
                .clicked()
            {
                chosen = Some(id.clone());
                ui.close();
            }
        }
        ui.add_space(3.0);
        hline(ui, p);
        ui.add_space(3.0);
        if ui
            .button(
                RichText::new("글꼴 파일 직접 선택 (.ttf/.otf)...")
                    .size(11.5)
                    .color(p.accent),
            )
            .clicked()
        {
            ui.close();
            if let Some(f) = rfd::FileDialog::new()
                .add_filter("글꼴", &["ttf", "otf"])
                .pick_file()
            {
                chosen = Some(f.to_string_lossy().into_owned());
            }
        }
        if let Some(font) = chosen {
            self.apply_font(ctx, font);
            self.status = format!(
                "글꼴을 변경했습니다: {}",
                theme::font_display_name(&self.font_family)
            );
        }
    }

    fn show_toast(&mut self, ctx: &egui::Context, p: &Pal) {
        if self.status != self.toast.0 {
            self.toast = (self.status.clone(), Some(Instant::now()));
        }
        let (Some(t), false) = (self.toast.1, self.toast.0.is_empty()) else {
            return;
        };
        let shown = Duration::from_millis(2500);
        let Some(left) = shown.checked_sub(t.elapsed()) else {
            // 지나간 알림은 비운다 → 같은 알림(예: 연속 Ctrl+S)도 다시 뜬다
            self.status.clear();
            self.toast.0.clear();
            return;
        };
        ctx.request_repaint_after(left);
        egui::Area::new(egui::Id::unique("toast"))
            .anchor(Align2::RIGHT_BOTTOM, vec2(-20.0, -38.0))
            .order(Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(p.elevated)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(8)
                    .shadow(theme::shadow(p))
                    .inner_margin(Margin::symmetric(14, 8))
                    .show(ui, |ui| {
                        ui.label(RichText::new(&self.toast.0).size(12.5).color(p.text));
                    });
            });
    }

    // ---------- 목차 서랍 ----------
    fn toc(&mut self, ui: &mut egui::Ui, p: &Pal) {
        Frame::new()
            .inner_margin(Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(img(theme::LIST, 14.0, p.text));
                    ui.label(
                        RichText::new("문서 목차 (TOC)")
                            .font(FontId::new(13.0, theme::bold()))
                            .color(p.text),
                    );
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if small_icon_btn(ui, p, theme::X, p.muted, "목차 닫기").clicked() {
                            self.toc_open = false;
                        }
                    });
                });
            });
        hline(ui, p);
        let Some(name) = self.current.as_deref() else {
            return;
        };
        let Some(doc) = self.docs.get(name) else {
            return;
        };
        let mut clicked_byte = None;
        let toc_active = self.toc_active;
        let total = doc.outline.len();
        Frame::new().inner_margin(Margin::same(12)).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            if total == 0 {
                ui.label(
                    RichText::new("제목(#)이 없습니다")
                        .size(13.0)
                        .color(p.faint),
                );
            } else {
                egui::ScrollArea::vertical().show_rows(ui, 28.0, total, |ui, range| {
                    for &(level, ref title, _, _, b) in &doc.outline[range] {
                        let active = toc_active == Some(b);
                        let indent = match level {
                            1 => 0.0,
                            2 => 12.0,
                            _ => 24.0,
                        };
                        let color = if active {
                            p.accent
                        } else if level == 1 {
                            p.text
                        } else {
                            p.muted
                        };
                        let bg = active.then(|| p.accent.gamma_multiply(0.10));
                        let r = ui
                            .horizontal(|ui| {
                                ui.add_space(indent);
                                let resp = row(ui, p, None, title, color, 28.0, bg);
                                if active {
                                    let rr = resp.rect;
                                    ui.painter().rect_filled(
                                        Rect::from_min_size(
                                            pos2(rr.left() + 2.0, rr.top() + 5.0),
                                            vec2(2.5, rr.height() - 10.0),
                                        ),
                                        2.0,
                                        p.accent,
                                    );
                                }
                                resp
                            })
                            .inner;
                        if r.clicked() {
                            clicked_byte = Some(b);
                        }
                    }
                });
            }
        });
        if let Some(b) = clicked_byte {
            self.scroll_to = Some(b);
        }
    }

    // ---------- 노션 라이브 캔버스 ----------
    fn canvas(&mut self, ui: &mut egui::Ui, p: &Pal) {
        let ctx = ui.ctx().clone();
        let area = ui.max_rect();
        let Some(name) = self.current.clone() else {
            self.welcome(ui, p);
            return;
        };
        if self.block_heights.len() > 16_384 {
            self.block_heights.clear(); // 오래 편집하면 쌓이는 높이 캐시 상한
        }
        if let Some(sel) = self.pending_sel.take() {
            self.place_cursor(&ctx, &name, sel);
        } else if let Some((start, caret)) = self.pending_byte_caret.take() {
            self.place_byte_caret(&ctx, &name, start, caret);
        }
        let active = self.active_block();
        if let Some(r) = &active {
            self.active = Some(r.start);
        }
        let mut slots = std::mem::take(&mut self.canvas_slots);
        {
            let doc = &self.docs[&name];
            collect_doc_slots_into(
                &doc.text,
                &doc.blocks,
                active.as_ref().filter(|r| r.is_empty()).map(|r| r.start),
                &mut slots,
            );
        }
        let mut items = std::mem::take(&mut self.canvas_items);
        items.clear();
        items.reserve(slots.len());
        let mut block_idx = 0usize;
        for slot in slots.drain(..) {
            let is_act = active.as_ref() == Some(&slot);
            let idx = if slot.is_empty() {
                0
            } else {
                let i = block_idx;
                block_idx += 1;
                i
            };
            items.push((slot, is_act, idx));
        }
        self.canvas_slots = slots;
        // 전체 문서 String 복제 없이 잠시 꺼내 쓰고 프레임 끝에 되돌린다 (Zero-Copy)
        let (text, doc_rev) = {
            let doc = self.docs.get_mut(&name).unwrap();
            (std::mem::take(&mut doc.text), doc.rev)
        };
        let body: f32 = if self.small_text { 14.5 } else { 16.0 };
        let scheme = self.doc_scheme().to_string();
        let menu_was_open = self.context_menu.is_some();
        let (clicked, secondary_clicked, click_pos, shift_held) = ctx.input(|i| {
            (
                i.pointer.primary_clicked() && !menu_was_open,
                i.pointer.secondary_clicked(),
                i.pointer.interact_pos(),
                i.modifiers.shift,
            )
        });
        let cmds_before = ctx.output(|o| o.commands.len());
        let mut change: Option<(Range<usize>, String)> = None;
        let mut go_to: Option<(usize, usize)> = None;
        let mut go_click_pos: Option<egui::Pos2> = None;
        let mut shift_click_block: Option<Range<usize>> = None;
        let mut open_canvas_menu: Option<egui::Pos2> = None;
        let mut consumed = menu_was_open;
        let mut below = false;
        let mut deactivate_now = false;
        let mut delete_block_now = false;
        let mut fmt = None;
        let mut slash = None;
        let mut toc_active = None;
        let mut block_rects = std::mem::take(&mut self.canvas_rects);
        block_rects.clear();

        egui::ScrollArea::vertical()
            .id_salt(("canvas", &name))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let full = ui.available_width();
                let w = if self.full_width {
                    (full - 72.0).max(200.0)
                } else {
                    (full - 64.0).clamp(200.0, 708.0)
                };
                let h_cache_key = (w.to_bits(), body.to_bits());
                if self.height_cache_key != h_cache_key {
                    self.height_cache_key = h_cache_key;
                    self.block_heights.clear();
                }
                let pad = ((full - w) / 2.0).max(16.0);
                ui.add_space(36.0);
                ui.horizontal_top(|ui| {
                    ui.add_space(pad);
                    ui.vertical(|ui| {
                        ui.set_width(w);
                        canvas_style(ui, body, p);
                        let clip = ui.clip_rect();
                        let view_top = clip.top();
                        block_rects.reserve(items.len());
                        for (r, is_active, block_idx) in &items {
                            if *is_active {
                                let out = self.block_editor(
                                    ui,
                                    &ctx,
                                    p,
                                    &name,
                                    &text,
                                    doc_rev,
                                    r.clone(),
                                    body,
                                    area,
                                );
                                if out.changed.is_some() {
                                    change = out.changed.map(|s| (r.clone(), s));
                                }
                                deactivate_now |= out.deactivate;
                                delete_block_now |= out.delete_block;
                                fmt = fmt.or(out.fmt);
                                slash = slash.or(out.slash);
                                consumed |= out.interacted;
                                if clicked && ui.rect_contains_pointer(out.rect) {
                                    consumed = true;
                                }
                                let full_row = Rect::from_x_y_ranges(
                                    ui.max_rect().x_range(),
                                    out.rect.y_range(),
                                );
                                if secondary_clicked && ui.rect_contains_pointer(full_row) {
                                    if let Some(pos) = click_pos {
                                        open_canvas_menu = Some(pos);
                                        consumed = true;
                                    }
                                }
                                block_rects.push((r.clone(), full_row));
                            } else {
                                let block_rect = if r.is_empty() {
                                    let (rect, _) =
                                        ui.allocate_exact_size(vec2(w, body * 1.6), Sense::hover());
                                    rect
                                } else {
                                    let h_key = egui::Id::unique((
                                        &text[r.clone()],
                                        w.to_bits(),
                                        body.to_bits(),
                                    ));
                                    let cursor_y = ui.cursor().top();
                                    let cached_h = self.block_heights.get(&h_key).copied();
                                    let margin = 240.0;
                                    let below_fold = cursor_y > clip.bottom() + margin
                                        && self.scroll_to.is_none_or(|st| r.start > st);
                                    let offscreen = self.scroll_to != Some(r.start)
                                        && (below_fold
                                            || cached_h.is_some_and(|h| {
                                                cursor_y + h < clip.top() - margin
                                            }));
                                    if offscreen {
                                        let h = cached_h.unwrap_or_else(|| {
                                            estimate_block_height(&text[r.clone()], body)
                                        });
                                        let (_, rect) = ui.allocate_space(vec2(w, h));
                                        rect
                                    } else {
                                        let out = self.render_block(
                                            ui,
                                            p,
                                            &text,
                                            r.clone(),
                                            *block_idx,
                                            body,
                                            w,
                                            &scheme,
                                        );
                                        self.block_heights.insert(h_key, out.rect.height());
                                        if let Some(s) = out.changed {
                                            change = Some((r.clone(), s));
                                            consumed = true;
                                        }
                                        consumed |= out.consumed;
                                        out.rect
                                    }
                                };
                                let full_row = Rect::from_x_y_ranges(
                                    ui.max_rect().x_range(),
                                    block_rect.y_range(),
                                );
                                block_rects.push((r.clone(), full_row));
                                if secondary_clicked && ui.rect_contains_pointer(full_row) {
                                    if let Some(pos) = click_pos {
                                        let in_block_sel =
                                            self.block_sel.as_ref().is_some_and(|bs| {
                                                !r.is_empty()
                                                    && r.start < bs.end
                                                    && r.end > bs.start
                                            });
                                        if !in_block_sel {
                                            go_to = Some((r.start, r.end));
                                            go_click_pos = (!r.is_empty()).then_some(pos);
                                        }
                                        open_canvas_menu = Some(pos);
                                        consumed = true;
                                    }
                                } else if clicked && !consumed && ui.rect_contains_pointer(full_row)
                                {
                                    if shift_held && !r.is_empty() {
                                        shift_click_block = Some(r.clone());
                                        consumed = true;
                                    } else {
                                        go_to = Some((r.start, r.end));
                                        go_click_pos = click_pos.filter(|_| !r.is_empty());
                                        consumed = true;
                                    }
                                }
                                if ui.rect_contains_pointer(full_row) {
                                    ctx.set_cursor_icon(CursorIcon::Text);
                                }
                                if !r.is_empty()
                                    && block_rect.top() <= view_top + 80.0
                                    && text[r.clone()].trim_start().starts_with('#')
                                    && heading_level(text[r.clone()].trim_start()).is_some()
                                {
                                    toc_active = Some(r.start);
                                }
                                if self.scroll_to == Some(r.start) {
                                    ui.scroll_to_rect(block_rect, Some(Align::TOP));
                                    self.scroll_to = None;
                                }
                            }
                            ui.add_space(4.0);
                        }
                        if let Some(doc) = self.docs.get_mut(&name) {
                            doc.text = text;
                        }

                        // 마우스 드래그: 단일 블록 텍스트 선택 또는 다중 블록 범위 선택
                        let (drag_active, origin_opt, pos_opt) = ctx.input(|i| {
                            let active = !menu_was_open
                                && (i.pointer.primary_down() || i.pointer.primary_released())
                                && i.pointer.is_decidedly_dragging();
                            (active, i.pointer.press_origin(), i.pointer.interact_pos())
                        });
                        if drag_active && !consumed && self.dialog.is_none() {
                            if let (Some(origin), Some(pos)) = (origin_opt, pos_opt) {
                                let on_fg = ctx
                                    .layer_id_at(origin)
                                    .is_some_and(|l| l.order == Order::Foreground);
                                if area.contains(origin) && !on_fg {
                                    let edge = 32.0;
                                    if pos.y < area.top() + edge {
                                        ui.scroll_with_delta(vec2(
                                            0.0,
                                            (area.top() + edge - pos.y).min(18.0),
                                        ));
                                        ctx.request_repaint();
                                    } else if pos.y > area.bottom() - edge {
                                        ui.scroll_with_delta(vec2(
                                            0.0,
                                            -(pos.y - (area.bottom() - edge)).min(18.0),
                                        ));
                                        ctx.request_repaint();
                                    }
                                    let y_min = origin.y.min(pos.y);
                                    let y_max = origin.y.max(pos.y);
                                    let mut first_hit: Option<Range<usize>> = None;
                                    let mut last_hit: Option<Range<usize>> = None;
                                    let mut hit_count = 0usize;
                                    for (r, rect) in &block_rects {
                                        if !r.is_empty()
                                            && rect.bottom() + 2.0 >= y_min
                                            && rect.top() - 2.0 <= y_max
                                        {
                                            if first_hit.is_none() {
                                                first_hit = Some(r.clone());
                                            }
                                            last_hit = Some(r.clone());
                                            hit_count += 1;
                                        }
                                    }
                                    if hit_count == 1 {
                                        let b = first_hit.unwrap();
                                        self.block_sel = None;
                                        self.block_sel_anchor = None;
                                        self.block_sel_head = None;
                                        go_click_pos = None;
                                        if self.active != Some(b.start) {
                                            self.drag_anchor = Some((b.start, origin));
                                            self.drag_last_pos = Some(pos);
                                            go_to = Some((b.start, b.end));
                                            consumed = true;
                                            ctx.request_repaint();
                                        } else if self.drag_anchor.is_some() {
                                            self.drag_last_pos = Some(pos);
                                            consumed = true;
                                        }
                                    } else if hit_count >= 2 {
                                        let first = first_hit.unwrap();
                                        let last = last_hit.unwrap();
                                        let start = first.start;
                                        let end = last.end;
                                        let (anchor_blk, head_blk) = if pos.y >= origin.y {
                                            (first, last)
                                        } else {
                                            (last, first)
                                        };
                                        self.leave_blank();
                                        self.active = None;
                                        self.drag_anchor = None;
                                        self.drag_last_pos = None;
                                        self.block_sel_anchor = Some(anchor_blk.start);
                                        self.block_sel_head = Some(head_blk.start);
                                        self.block_sel = Some(start..end);
                                        go_to = None;
                                        go_click_pos = None;
                                        consumed = true;
                                    }
                                }
                            }
                        }

                        // 다중 블록 / 전체 문서 선택 하이라이트 및 서식 막대 표시
                        let mut first_sel_rect: Option<Rect> = None;
                        if let Some(sel_range) = self.block_sel.clone() {
                            for (r, full_row) in &block_rects {
                                if !r.is_empty()
                                    && r.start < sel_range.end
                                    && r.end > sel_range.start
                                {
                                    let box_rect = Rect::from_x_y_ranges(
                                        ui.min_rect().x_range(),
                                        full_row.y_range(),
                                    )
                                    .expand2(vec2(6.0, 2.0));
                                    ui.painter().rect(
                                        box_rect,
                                        6.0,
                                        p.accent.gamma_multiply(0.16),
                                        Stroke::new(1.0, p.accent.gamma_multiply(0.45)),
                                        egui::StrokeKind::Inside,
                                    );
                                    if first_sel_rect.is_none() && box_rect.bottom() >= clip.top() {
                                        first_sel_rect = Some(box_rect);
                                    }
                                }
                            }
                        }
                        if let Some(sr) = first_sel_rect.filter(|_| {
                            self.context_menu.is_none() && !ctx.input(|i| i.pointer.primary_down())
                        }) {
                            let (f, bar_hit) = self.show_format_bar(&ctx, p, sr, area);
                            fmt = fmt.or(f);
                            consumed |= bar_hit;
                        }

                        // 마지막 블록 아래 빈 곳을 누르면 새 블록 (노션과 같음)
                        let rest = ui.available_height().max(140.0);
                        let (_, resp) = ui.allocate_exact_size(vec2(w, rest), Sense::click());
                        if resp.hovered() {
                            ctx.set_cursor_icon(CursorIcon::Text);
                        }
                        if resp.secondary_clicked() {
                            if let Some(pos) = click_pos {
                                open_canvas_menu = Some(pos);
                                consumed = true;
                            }
                        } else if !menu_was_open && resp.clicked() {
                            below = true;
                            consumed = true;
                        }
                    });
                });
            });

        if secondary_clicked && open_canvas_menu.is_none() && ui.rect_contains_pointer(area) {
            open_canvas_menu = click_pos;
        }
        if let Some(pos) = open_canvas_menu {
            self.context_menu = Some(ContextMenu::Canvas(pos));
            self.canvas_sub = None;
        }

        // 프레임 재사용 벡터를 즉시 복구
        self.canvas_items = items;
        self.canvas_rects = block_rects;

        // 링크·복사 버튼을 누른 클릭은 블록 편집으로 넘어가지 않는다
        if ctx.output(|o| o.commands.len()) > cmds_before {
            go_to = None;
            go_click_pos = None;
            shift_click_block = None;
        }
        if let Some((r, mut s)) = change.take() {
            if s.len() < r.len() && s.starts_with('\n') {
                let lead = s.bytes().take_while(|&b| b == b'\n').count();
                s.drain(..lead);
            }
            let mut auto_code_caret = None;
            let mut next_cache = None;
            if let Some(doc) = self.docs.get_mut(&name) {
                // 노션식 코드 모드 트리거: 한 줄짜리 "```" / "~~~" 뒤 스페이스 입력 시 즉시 코드 블록 열기
                let old_blk = &doc.text[r.clone()];
                if !old_blk.contains('\n') {
                    let old_trim = old_blk.trim();
                    if (old_trim == "```" || old_trim == "~~~")
                        && s.len() == old_blk.len() + 1
                        && s.starts_with(old_blk)
                        && s.ends_with(' ')
                    {
                        let indent_len = old_blk.len() - old_blk.trim_start().len();
                        let indent = &old_blk[..indent_len];
                        let expanded = format!("{indent}{old_trim}\n{indent}\n{indent}{old_trim}");
                        auto_code_caret = Some(r.start + indent_len + 3 + 1 + indent_len);
                        s = expanded;
                    }
                }
                if !r.is_empty() && s.is_empty() {
                    let lead = doc.text[..r.start]
                        .bytes()
                        .rev()
                        .take_while(|&b| b == b'\n')
                        .count()
                        .min(2);
                    self.blank = if lead > 0 {
                        lead
                    } else {
                        doc.text[r.end..]
                            .bytes()
                            .take_while(|&b| b == b'\n')
                            .count()
                            .min(2)
                    };
                } else if !s.is_empty() {
                    self.blank = 0;
                }
                let prev_rev = doc.rev;
                let base_chars = (self.active_char_cache.0 == r.start
                    && self.active_char_cache.1 == prev_rev)
                    .then_some(self.active_char_cache.2);
                let new_chars = doc.apply_block_edit(r.clone(), &s);
                if let Some(info) = &mut self.cursor_info {
                    if info.block_start == r.start {
                        info.doc_rev = doc.rev;
                    }
                }
                if let Some(base) = base_chars {
                    next_cache = Some((r.start, doc.rev, base, new_chars));
                }
            }
            if let Some(c) = next_cache {
                self.active_char_cache = c;
            }
            if let Some(caret) = auto_code_caret {
                self.place_byte_caret(&ctx, &name, r.start, caret);
                self.focus_editor = true;
                ctx.request_repaint();
            }
            s.clear();
            self.edit_buf = s;
            go_to = None;
            go_click_pos = None;
            shift_click_block = None;
        }
        if self.pending_enter {
            self.pending_enter = false;
            if let Some(q) = self.slash_query() {
                let matches = slash_matches(&q);
                if let Some(&i) = matches.get(self.slash_sel.min(matches.len().saturating_sub(1))) {
                    self.apply_slash(i);
                }
            } else if !self.enter(&ctx) {
                self.insert(&ctx, "\n");
            }
            ctx.request_repaint();
        }
        self.toc_active = toc_active;
        if delete_block_now {
            if self.delete_current_blocks() {
                self.status = "표를 삭제했습니다".into();
            }
        } else if deactivate_now {
            self.deactivate();
        } else if let Some(target) = shift_click_block {
            let anchor_start = self
                .block_sel_anchor
                .or_else(|| {
                    self.active_block()
                        .filter(|b| !b.is_empty())
                        .map(|b| b.start)
                })
                .unwrap_or(target.start);
            let anchor_blk = self
                .docs
                .get(&name)
                .and_then(|d| d.blocks.iter().find(|b| b.start == anchor_start).cloned())
                .unwrap_or_else(|| target.clone());
            let start = anchor_blk.start.min(target.start);
            let end = anchor_blk.end.max(target.end);
            self.leave_blank();
            self.active = None;
            self.drag_anchor = None;
            self.drag_last_pos = None;
            self.pending_click_pos = None;
            self.block_sel_anchor = Some(anchor_blk.start);
            self.block_sel_head = Some(target.start);
            self.block_sel = Some(start..end);
        } else if let Some((s, c)) = go_to {
            if let Some(pos) = go_click_pos {
                self.go_click(s, pos);
            } else {
                self.go(s, c);
            }
            ctx.request_repaint();
        } else if below {
            self.block_sel = None;
            self.block_sel_anchor = None;
            self.block_sel_head = None;
            let last_is_blank = self.active_block().is_some_and(|r| r.is_empty());
            if !last_is_blank {
                self.append_block();
            }
        } else if clicked && !consumed && ui.rect_contains_pointer(area) {
            self.deactivate();
        }
        if let Some(f) = fmt {
            self.format(&ctx, f);
        }
        if let Some(i) = slash {
            self.apply_slash(i);
        }
        self.find_bar(&ctx, p, area);
    }

    // 열린 문서가 없을 때: 첫 실행·빈 폴더에서 바로 시작할 수 있는 화면
    fn welcome(&mut self, ui: &mut egui::Ui, p: &Pal) {
        let top = (ui.available_height() * 0.5 - 130.0).max(24.0);
        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(top);
            let (r, _) = ui.allocate_exact_size(vec2(52.0, 52.0), Sense::hover());
            ui.painter().rect_filled(r, 13.0, p.accent);
            img(theme::BRAND, 28.0, Color32::WHITE)
                .paint_at(ui, Rect::from_center_size(r.center(), vec2(28.0, 28.0)));
            ui.add_space(18.0);
            ui.label(
                RichText::new("무엇을 쓸까요?")
                    .font(FontId::new(20.0, theme::bold()))
                    .color(p.text),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new("새 문서를 만들거나, 가지고 있는 Markdown 파일·폴더를 여세요.")
                    .size(13.5)
                    .color(p.muted),
            );
            ui.add_space(20.0);
            ui.spacing_mut().button_padding = vec2(16.0, 6.0);
            // 버튼 세 개를 가운데 정렬하려고 폭을 미리 잡는다
            let width = 330.0_f32.min(ui.available_width());
            ui.allocate_ui_with_layout(
                vec2(width, 32.0),
                egui::Layout::left_to_right(Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if primary(ui, p, "새 탭  Ctrl+N").clicked() {
                        self.new_tab();
                    }
                    if ghost(ui, p, "폴더 열기", None).clicked() {
                        self.pick_folder();
                    }
                    if ghost(ui, p, "파일 열기", None).clicked() {
                        self.open_local_file();
                    }
                },
            );
            ui.add_space(22.0);
            ui.label(
                RichText::new(".md 파일이나 폴더를 이 창에 끌어다 놓아도 열립니다.")
                    .size(12.0)
                    .color(p.faint),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!("작업 폴더  {}", self.root.display()))
                    .size(11.5)
                    .color(p.faint),
            );
        });
    }

    // 전체 문서 기준 선택(sel)을 블록 편집기 상태로 옮긴다
    fn place_cursor(&mut self, ctx: &egui::Context, name: &str, sel: Sel) {
        let doc = &self.docs[name];
        let text = &doc.text;
        let b0 = byte_idx(text, sel.0);
        let b1 = byte_idx(text, sel.1);
        let block = doc
            .blocks
            .iter()
            .find(|r| r.start <= b0 && b0 <= r.end)
            .cloned();
        if let Some(r) = &block {
            if b1 > r.end {
                self.active = None;
                self.block_sel = Some(b0..b1);
                return;
            }
        }
        let start = block.map_or(b0, |r| r.start);
        let base = char_len(&text[..start]);
        let id = Self::editor_id(name, start);
        let mut st = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        st.cursor.set_char_range(Some(CCursorRange::two(
            CCursor::new(sel.0.saturating_sub(base)),
            CCursor::new(sel.1.saturating_sub(base)),
        )));
        egui::TextEdit::store_state(ctx, id, st);
        self.block_sel = None;
        self.block_sel_anchor = None;
        self.block_sel_head = None;
        self.active = Some(start);
        self.last_sel = Some(sel);
        if self.scroll_cursor.is_none() {
            self.scroll_cursor = Some(None);
        }
    }

    // 블록 시작/바이트 오프셋으로 즉시 커서 배치 (방향키 이동 등 — 전체 문서 char_len 회피)
    fn place_byte_caret(&mut self, ctx: &egui::Context, name: &str, start: usize, caret: usize) {
        let Some(doc) = self.docs.get(name) else {
            return;
        };
        let text = &doc.text;
        let len = text.len();
        let mut s = start.min(len);
        while s > 0 && !text.is_char_boundary(s) {
            s -= 1;
        }
        let mut c_byte = caret.clamp(s, len);
        while c_byte > s && !text.is_char_boundary(c_byte) {
            c_byte -= 1;
        }
        let idx = doc.blocks.partition_point(|b| b.start <= s);
        let r = idx
            .checked_sub(1)
            .and_then(|i| doc.blocks.get(i))
            .filter(|b| s <= b.end)
            .cloned()
            .unwrap_or(s..s);
        let local_c = char_len(&text[r.start..c_byte.min(r.end)]);
        let id = Self::editor_id(name, r.start);
        let mut st = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        st.cursor
            .set_char_range(Some(CCursorRange::one(CCursor::new(local_c))));
        egui::TextEdit::store_state(ctx, id, st);

        let base = if self.active_char_cache.1 == doc.rev
            && self.active_char_cache.0 <= len
            && text.is_char_boundary(self.active_char_cache.0)
        {
            let prev_start = self.active_char_cache.0;
            let prev_base = self.active_char_cache.2;
            if r.start >= prev_start {
                prev_base + char_len(&text[prev_start..r.start])
            } else {
                prev_base.saturating_sub(char_len(&text[r.start..prev_start]))
            }
        } else {
            char_len(&text[..r.start])
        };
        let blen = if r.is_empty() {
            0
        } else {
            char_len(&text[r.clone()])
        };
        self.active_char_cache = (r.start, doc.rev, base, blen);
        self.block_sel = None;
        self.block_sel_anchor = None;
        self.block_sel_head = None;
        self.active = Some(r.start);
        let abs_c = base + local_c.min(blen);
        self.last_sel = Some((abs_c, abs_c));
        self.scroll_cursor = Some(None);
    }

    fn show_format_bar(
        &self,
        ctx: &egui::Context,
        p: &Pal,
        sel_rect: Rect,
        bounds: Rect,
    ) -> (Option<Fmt>, bool) {
        let bar_w = 276.0;
        let bar_h = 34.0;
        let x = sel_rect.left().clamp(
            bounds.left() + 8.0,
            (bounds.right() - bar_w).max(bounds.left() + 8.0),
        );
        let above = sel_rect.top() - bar_h - 6.0;
        let y = if above >= bounds.top() + 4.0 {
            above
        } else {
            (sel_rect.bottom() + 6.0).min((bounds.bottom() - bar_h - 4.0).max(bounds.top() + 4.0))
        };
        let mut fmt = None;
        let area_resp = egui::Area::new(egui::Id::unique("format-bar"))
            .order(Order::Foreground)
            .fixed_pos(pos2(x, y))
            .show(ctx, |ui| {
                Frame::new()
                    .fill(p.elevated)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(8)
                    .shadow(theme::shadow(p))
                    .inner_margin(Margin::symmetric(5, 3))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            let m = p.muted;
                            let mut pick = None;
                            let mut kbd = false;
                            let mut b = |ui: &mut egui::Ui, icon, color, tip, f| {
                                if small_icon_btn(ui, p, icon, color, tip).clicked() {
                                    pick = Some(f);
                                }
                            };
                            b(
                                ui,
                                theme::BOLD,
                                m,
                                "굵게 (Ctrl+B)",
                                Fmt::Wrap("**", "**", "굵게"),
                            );
                            b(
                                ui,
                                theme::ITALIC,
                                m,
                                "기울임 (Ctrl+I)",
                                Fmt::Wrap("*", "*", "기울임"),
                            );
                            b(
                                ui,
                                theme::STRIKE,
                                m,
                                "취소선 (Ctrl+Shift+S)",
                                Fmt::Wrap("~~", "~~", "취소선"),
                            );
                            divider(ui, p);
                            b(
                                ui,
                                theme::HIGHLIGHT,
                                Color32::from_rgb(0xea, 0xb3, 0x08),
                                "형광펜 강조 (Ctrl+Shift+H)",
                                Fmt::Wrap("==", "==", "강조"),
                            );
                            b(
                                ui,
                                theme::CODE,
                                m,
                                "인라인 코드 (Ctrl+E)",
                                Fmt::Wrap("`", "`", "code"),
                            );
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("K").monospace().size(11.0).color(m),
                                    )
                                    .min_size(vec2(26.0, 26.0))
                                    .corner_radius(5),
                                )
                                .on_hover_text("키보드 키 — [[키]] (Ctrl+Shift+K)")
                                .clicked()
                            {
                                kbd = true;
                            }
                            divider(ui, p);
                            b(
                                ui,
                                theme::LINK,
                                m,
                                "링크 (Ctrl+K)",
                                Fmt::Wrap("[", "](https://)", "링크"),
                            );
                            b(ui, theme::X, m, "서식 지우기 (Ctrl+\\)", Fmt::Clear);
                            fmt = fmt
                                .or(pick)
                                .or(kbd.then_some(Fmt::Wrap("[[", "]]", "Ctrl")));
                        });
                    });
            });
        let hit = fmt.is_some()
            || ctx
                .input(|i| i.pointer.interact_pos())
                .is_some_and(|pos| area_resp.response.rect.contains(pos));
        (fmt, hit)
    }

    #[allow(clippy::too_many_arguments)]
    fn block_editor(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        p: &Pal,
        name: &str,
        text: &str,
        doc_rev: u64,
        r: Range<usize>,
        body: f32,
        canvas_area: Rect,
    ) -> EditorOut {
        let id = Self::editor_id(name, r.start);
        let mut buf = std::mem::take(&mut self.edit_buf);
        buf.clear();
        buf.push_str(&text[r.clone()]);
        let pal = *p;
        let dark = self.dark;
        let ppp_bits = ctx.pixels_per_point().to_bits();
        let mut galley_cache = self.editor_galley_cache.take();
        if !self.raw_table_mode {
            if let Some(tbl) = parse_md_table(&buf) {
                self.pending_click_pos = None;
                self.drag_anchor = None;
                self.drag_last_pos = None;
                let (rect, tbl_changed, tbl_interacted, tbl_deactivate, tbl_delete) =
                    self.render_table_editor(ui, ctx, p, id, body, tbl, &mut buf);
                self.editor_galley_cache = galley_cache;
                let changed = if tbl_changed {
                    Some(buf)
                } else {
                    buf.clear();
                    self.edit_buf = buf;
                    None
                };
                return EditorOut {
                    rect,
                    changed,
                    fmt: None,
                    slash: None,
                    interacted: tbl_interacted,
                    deactivate: tbl_deactivate,
                    delete_block: tbl_delete,
                };
            }
        }
        // 제목은 렌더링 화면처럼 위 여백을 둬서 편집을 시작해도 자리가 움직이지 않게 한다
        if let Some(lv) = heading_level(buf.trim_start()) {
            ui.add_space(HEADING_TOP[lv - 1]);
        }
        let mut pre_click = None;
        let pre_pos = ui.cursor().min;
        if let Some((target_start, click_pos)) = self.pending_click_pos {
            if target_start == r.start && self.drag_anchor.is_none() {
                self.pending_click_pos = None;
                let wrap = ui.available_width();
                let g = editor_galley_cached(
                    ctx,
                    &mut galley_cache,
                    &buf,
                    body,
                    wrap,
                    &pal,
                    dark,
                    ppp_bits,
                );
                let c = g.cursor_from_pos(click_pos - pre_pos);
                let cr = CCursorRange::one(c);
                let mut st = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
                st.cursor.set_char_range(Some(cr));
                egui::TextEdit::store_state(ctx, id, st);
                pre_click = Some((pre_pos, click_pos));
            }
        }
        let mut header_interacted = false;
        let mut deactivate = false;
        let mut delete_block = false;
        let is_raw_table = self.raw_table_mode && parse_md_table(&buf).is_some();
        if is_raw_table {
            ui.horizontal(|ui| {
                ui.add(img(theme::TABLE, 13.0, p.accent));
                ui.label(
                    RichText::new("표 마크다운 편집")
                        .size(11.5)
                        .strong()
                        .color(p.muted),
                );
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    if table_delete_btn(ui).clicked() {
                        delete_block = true;
                        header_interacted = true;
                    }
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("표 직접 편집").size(11.0).color(p.accent),
                            )
                            .fill(p.surface)
                            .stroke(Stroke::new(1.0, p.border))
                            .corner_radius(6)
                            .min_size(vec2(0.0, 23.0)),
                        )
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .clicked()
                    {
                        self.raw_table_mode = false;
                        self.table_focus_cell = Some((0, 0));
                        header_interacted = true;
                    }
                });
            });
            ui.add_space(4.0);
        }
        let code_meta = {
            let bt_init = buf.trim_start();
            if bt_init.starts_with("```") || bt_init.starts_with("~~~") {
                let lead = buf.len() - bt_init.len();
                let fence = bt_init[..3].to_string();
                let first_nl_rel = bt_init.find('\n').unwrap_or(bt_init.len());
                let cur_lang = bt_init[3..first_nl_rel].trim().to_string();
                Some((lead, fence, first_nl_rel, cur_lang))
            } else {
                None
            }
        };
        let mut code_frame_rect = None;
        let mut copy_inner_code: Option<String> = None;

        let take_focus = std::mem::take(&mut self.focus_editor);
        if take_focus
            || (self.dialog.is_none()
                && !self.find.open
                && self.context_menu.is_none()
                && ctx.memory(|m| m.focused().is_none()))
        {
            ctx.memory_mut(|m| {
                if !m.has_focus(id) {
                    m.request_focus(id);
                }
            });
        }
        let out = if let Some((lead, fence, first_nl_rel, cur_lang)) = code_meta {
            let mut picked_lang: Option<&'static str> = None;

            let framed = Frame::new()
                .fill(p.code_bg)
                .stroke(Stroke::new(1.0, p.accent.gamma_multiply(0.55)))
                .corner_radius(8)
                .inner_margin(Margin::symmetric(14, 9))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        ui.add(img(theme::CODE, 13.0, p.accent));
                        ui.label(
                            RichText::new("코드 블록")
                                .size(11.5)
                                .strong()
                                .color(p.muted),
                        );
                        let lang_label = if cur_lang.is_empty() {
                            "언어 선택"
                        } else {
                            cur_lang.as_str()
                        };
                        let lang_btn = ui
                            .add(
                                egui::Button::new(
                                    RichText::new(format!("{lang_label} ▾"))
                                        .family(FontFamily::Monospace)
                                        .size(11.0)
                                        .color(if cur_lang.is_empty() {
                                            p.muted
                                        } else {
                                            p.accent
                                        }),
                                )
                                .fill(p.surface)
                                .stroke(Stroke::new(1.0, p.border))
                                .corner_radius(6)
                                .min_size(vec2(0.0, 23.0)),
                            )
                            .on_hover_cursor(CursorIcon::PointingHand)
                            .on_hover_text("코드 언어 선택");
                        if lang_btn.clicked() {
                            header_interacted = true;
                        }
                        egui::Popup::menu(&lang_btn).show(|ui| {
                            ui.set_width(140.0);
                            ui.spacing_mut().item_spacing.y = 1.0;
                            for lang_opt in [
                                "",
                                "rust",
                                "python",
                                "javascript",
                                "typescript",
                                "json",
                                "bash",
                                "sql",
                                "sas",
                                "c",
                                "cpp",
                                "go",
                                "yaml",
                                "toml",
                                "mermaid",
                            ] {
                                let lbl = if lang_opt.is_empty() {
                                    "없음 (일반)"
                                } else {
                                    lang_opt
                                };
                                let selected = cur_lang.eq_ignore_ascii_case(lang_opt);
                                if ui
                                    .selectable_label(
                                        selected,
                                        RichText::new(lbl).family(FontFamily::Monospace).size(11.5),
                                    )
                                    .clicked()
                                {
                                    picked_lang = Some(lang_opt);
                                    header_interacted = true;
                                }
                            }
                        });
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("완료 (Esc)").size(11.0).color(p.accent),
                                    )
                                    .fill(p.accent.gamma_multiply(0.12))
                                    .stroke(Stroke::new(1.0, p.accent.gamma_multiply(0.35)))
                                    .corner_radius(6)
                                    .min_size(vec2(0.0, 23.0)),
                                )
                                .on_hover_cursor(CursorIcon::PointingHand)
                                .on_hover_text("편집 마치고 코드 카드로 보기 (Esc)")
                                .clicked()
                            {
                                deactivate = true;
                                header_interacted = true;
                            }
                            if ui
                                .add(
                                    egui::Button::image_and_text(
                                        img(theme::COPY, 11.0, p.muted),
                                        RichText::new("복사").size(11.0).color(p.muted),
                                    )
                                    .fill(p.surface)
                                    .stroke(Stroke::new(1.0, p.border))
                                    .corner_radius(6)
                                    .min_size(vec2(50.0, 23.0)),
                                )
                                .on_hover_cursor(CursorIcon::PointingHand)
                                .on_hover_text("코드 내용 복사")
                                .clicked()
                            {
                                let body_start = lead + first_nl_rel + 1;
                                let body_part = if body_start < buf.len() {
                                    &buf[body_start..]
                                } else {
                                    ""
                                };
                                let inner = body_part
                                    .trim_end()
                                    .strip_suffix(fence.as_str())
                                    .unwrap_or(body_part)
                                    .trim_end_matches(['\n', '\r']);
                                copy_inner_code = Some(inner.to_string());
                                header_interacted = true;
                            }
                        });
                    });
                    if let Some(new_lang) = picked_lang {
                        let old_end = lead + first_nl_rel;
                        let old_l0_chars = char_len(&buf[..old_end]);
                        let new_l0 = format!("{}{fence}{new_lang}", &buf[..lead]);
                        let new_l0_chars = char_len(&new_l0);
                        buf.replace_range(..old_end, &new_l0);
                        let delta = new_l0_chars as isize - old_l0_chars as isize;
                        if let Some(mut st) = egui::TextEdit::load_state(ctx, id) {
                            if let Some(cr) = st.cursor.char_range() {
                                let shift = |idx: usize| -> usize {
                                    if idx >= old_l0_chars {
                                        (idx as isize + delta).max(0) as usize
                                    } else {
                                        idx.min(new_l0_chars)
                                    }
                                };
                                st.cursor.set_char_range(Some(CCursorRange::two(
                                    CCursor::new(shift(cr.primary.index.into())),
                                    CCursor::new(shift(cr.secondary.index.into())),
                                )));
                                egui::TextEdit::store_state(ctx, id, st);
                            }
                        }
                        galley_cache = None;
                        self.focus_editor = true;
                    }
                    ui.add_space(6.0);
                    hline(ui, p);
                    ui.add_space(6.0);
                    let mut layouter = |ui: &egui::Ui, b: &dyn egui::TextBuffer, wrap: f32| {
                        editor_galley_cached(
                            ui.ctx(),
                            &mut galley_cache,
                            b.as_str(),
                            body,
                            wrap,
                            &pal,
                            dark,
                            ppp_bits,
                        )
                    };
                    egui::TextEdit::multiline(&mut buf)
                        .id(id)
                        .frame(Frame::NONE)
                        .margin(Margin::ZERO)
                        .desired_width(f32::INFINITY)
                        .desired_rows(1)
                        .lock_focus(true)
                        .hint_text(RichText::new(PLACEHOLDER).size(body).color(p.faint))
                        .layouter(&mut layouter)
                        .show(ui)
                });
            code_frame_rect = Some(framed.response.rect);
            framed.inner
        } else {
            let mut layouter = |ui: &egui::Ui, b: &dyn egui::TextBuffer, wrap: f32| {
                editor_galley_cached(
                    ui.ctx(),
                    &mut galley_cache,
                    b.as_str(),
                    body,
                    wrap,
                    &pal,
                    dark,
                    ppp_bits,
                )
            };
            egui::TextEdit::multiline(&mut buf)
                .id(id)
                .frame(Frame::NONE)
                .margin(Margin::ZERO)
                .desired_width(f32::INFINITY)
                .desired_rows(1)
                .lock_focus(true)
                .hint_text(RichText::new(PLACEHOLDER).size(body).color(p.faint))
                .layouter(&mut layouter)
                .show(ui)
        };
        if let Some(copied) = copy_inner_code {
            ctx.copy_text(copied);
            self.status = "코드를 복사했습니다".into();
        }
        self.editor_galley_cache = galley_cache;
        if code_frame_rect.is_none() {
            paint_list_markers(ui.painter(), &out.galley, out.galley_pos, &buf, body, &pal);
        }
        let mut sel = out.cursor_range.map(|c| c.as_sorted_char_range());
        let mut primary_cursor = out.cursor_range.map(|c| c.primary);
        if let Some((pre_pos, click_pos)) = pre_click {
            if (out.galley_pos - pre_pos).length_sq() > 0.25 {
                let c = out.galley.cursor_from_pos(click_pos - out.galley_pos);
                let cr = CCursorRange::one(c);
                let mut st = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
                st.cursor.set_char_range(Some(cr));
                egui::TextEdit::store_state(ctx, id, st);
                sel = Some(cr.as_sorted_char_range());
                primary_cursor = Some(cr.primary);
                ctx.request_repaint();
            }
        }
        if let Some((target_start, click_pos)) = self.pending_click_pos {
            if target_start == r.start {
                self.pending_click_pos = None;
                if self.drag_anchor.is_none() {
                    let c = out.galley.cursor_from_pos(click_pos - out.galley_pos);
                    let cr = CCursorRange::one(c);
                    let mut st = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
                    st.cursor.set_char_range(Some(cr));
                    egui::TextEdit::store_state(ctx, id, st);
                    sel = Some(cr.as_sorted_char_range());
                    primary_cursor = Some(cr.primary);
                    ctx.request_repaint();
                }
            }
        }
        if let Some((anchor_start, origin)) = self.drag_anchor {
            if anchor_start == r.start {
                let (down, i_pos) =
                    ctx.input(|i| (i.pointer.primary_down(), i.pointer.interact_pos()));
                if let Some(p_cur) = i_pos {
                    self.drag_last_pos = Some(p_cur);
                }
                let cur_pos = self.drag_last_pos.unwrap_or(origin);
                let c1 = out.galley.cursor_from_pos(origin - out.galley_pos);
                let c2 = out.galley.cursor_from_pos(cur_pos - out.galley_pos);
                let cr = CCursorRange::two(c1, c2);
                let mut st = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
                st.cursor.set_char_range(Some(cr));
                egui::TextEdit::store_state(ctx, id, st);
                sel = Some(cr.as_sorted_char_range());
                primary_cursor = Some(cr.primary);
                out.response.request_focus();
                ctx.request_repaint();
                if !down {
                    self.drag_anchor = None;
                    self.drag_last_pos = None;
                }
            } else {
                self.drag_anchor = None;
                self.drag_last_pos = None;
            }
        }
        // 숨긴 ``` 줄이 지워지거나 바뀌었으면 그 부분만 되돌린다 (전체 선택 후 입력, 맨 앞 Backspace 등)
        if code_frame_rect.is_some() {
            if let Some((fixed, caret)) = keep_code_fences(&text[r.clone()], &buf) {
                buf = fixed;
                let cr = CCursorRange::one(CCursor::new(caret));
                let mut st = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
                st.cursor.set_char_range(Some(cr));
                egui::TextEdit::store_state(ctx, id, st);
                sel = Some(cr.as_sorted_char_range());
                primary_cursor = Some(cr.primary);
                ctx.request_repaint();
            }
        }
        let is_changed = buf != text[r.clone()];
        let (base, blen) =
            if self.active_char_cache.0 == r.start && self.active_char_cache.1 == doc_rev {
                let l = if is_changed {
                    char_len(&buf)
                } else {
                    self.active_char_cache.3
                };
                (self.active_char_cache.2, l)
            } else {
                let b = char_len(&text[..r.start]);
                let orig_l = char_len(&text[r.clone()]);
                self.active_char_cache = (r.start, doc_rev, b, orig_l);
                let l = if is_changed { char_len(&buf) } else { orig_l };
                (b, l)
            };
        // 커서가 숨긴 목록 기호·코드 펜스 안으로 들어가면 보이는 자리로 옮긴다
        if let Some(s) = sel.clone().filter(|s| s.start == s.end && !self.ime_composing) {
            let c = s.start.0;
            let to = match code_body_range(&buf) {
                Some((a, b)) if code_frame_rect.is_some() => c.clamp(a, b),
                _ => {
                    let prev = self
                        .last_sel
                        .and_then(|(a, b)| (a == b && a >= base).then(|| a - base));
                    clamp_hidden(&buf, c, prev)
                }
            };
            if to != c {
                let cr = CCursorRange::one(CCursor::new(to));
                let mut st = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
                st.cursor.set_char_range(Some(cr));
                egui::TextEdit::store_state(ctx, id, st);
                sel = Some(cr.as_sorted_char_range());
                primary_cursor = Some(cr.primary);
                ctx.request_repaint();
            }
        }
        let has_range_sel = self
            .last_sel
            .is_some_and(|(la, lb)| la < lb && la >= base && lb <= base + blen);
        if has_range_sel
            && (self.context_menu.is_some()
                || ctx.input(|i| {
                    i.pointer.button_pressed(egui::PointerButton::Secondary)
                        || i.pointer.button_down(egui::PointerButton::Secondary)
                        || i.pointer.secondary_clicked()
                }))
        {
            let (la, lb) = self.last_sel.unwrap();
            let cr = CCursorRange::two(CCursor::new(la - base), CCursor::new(lb - base));
            let mut st = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
            st.cursor.set_char_range(Some(cr));
            egui::TextEdit::store_state(ctx, id, st);
            sel = Some(cr.as_sorted_char_range());
            primary_cursor = Some(cr.primary);
        } else if let Some(s) = &sel {
            if !self.ime_composing
                && ((out.response.has_focus() && self.context_menu.is_none())
                    || s.start.0 < s.end.0)
            {
                self.last_sel = Some((base + s.start.0.min(blen), base + s.end.0.min(blen)));
            }
        }
        self.cursor_info = primary_cursor.map(|pc| {
            // 접어 둔(높이 0) ``` 줄은 빼고 보이는 줄 기준으로 첫·마지막 줄을 판단
            let rows: Vec<_> = out.galley.rows.iter().filter(|r| r.rect().height() > 1.0).collect();
            if rows.len() <= 1 {
                EditorCursorInfo {
                    block_start: r.start,
                    doc_rev,
                    on_first_row: true,
                    on_last_row: true,
                }
            } else {
                let cy = out.galley.pos_from_cursor(pc).center().y;
                EditorCursorInfo {
                    block_start: r.start,
                    doc_rev,
                    on_first_row: cy <= rows[0].rect().bottom() + 0.5,
                    on_last_row: cy >= rows[rows.len() - 1].rect().top() - 0.5,
                }
            }
        });
        let cursor_of = |c: usize| {
            out.galley
                .pos_from_cursor(CCursor::new(c))
                .translate(out.galley_pos.to_vec2())
        };
        if let Some(align) = self.scroll_cursor.take() {
            let c = sel.as_ref().map_or(0, |s| s.end.0);
            ui.scroll_to_rect(cursor_of(c), align);
        } else if is_changed {
            if let Some(s) = sel.as_ref() {
                let cr = cursor_of(s.end.0).expand2(vec2(0.0, 28.0));
                if cr.bottom() > canvas_area.bottom() || cr.top() < canvas_area.top() {
                    ui.scroll_to_rect(cr, None);
                }
            }
        }

        // 선택 영역 위 서식 막대 (드래그 중이거나 우클릭 메뉴가 열려 있을 때는 띄우지 않음)
        let mut fmt = None;
        let mut interacted = header_interacted;
        if let Some(s) = sel.filter(|s| {
            s.start.0 != s.end.0
                && !self.ime_composing
                && self.context_menu.is_none()
                && !ctx.input(|i| i.pointer.primary_down())
        }) {
            let (f, hit) = self.show_format_bar(ctx, p, cursor_of(s.start.0), canvas_area);
            fmt = f;
            interacted |= hit;
        }

        // 슬래시 메뉴
        let mut slash = None;
        if let Some(q) = buf
            .strip_prefix('/')
            .filter(|q| !q.contains(char::is_whitespace))
        {
            let matches = slash_matches(q);
            let sel_idx = self.slash_sel.min(matches.len().saturating_sub(1));
            let est_h = ((matches.len().max(1) as f32) * 40.0 + 48.0).min(352.0);
            let below_y = out.response.rect.bottom() + 4.0;
            let slash_pos = if below_y + est_h <= canvas_area.bottom()
                || out.response.rect.top() - est_h - 4.0 < canvas_area.top()
            {
                pos2(
                    out.response.rect.left(),
                    below_y.min((canvas_area.bottom() - est_h).max(canvas_area.top())),
                )
            } else {
                pos2(
                    out.response.rect.left(),
                    (out.response.rect.top() - est_h - 4.0).max(canvas_area.top()),
                )
            };
            let slash_resp = egui::Area::new(egui::Id::unique("slash-menu"))
                .order(Order::Foreground)
                .fixed_pos(slash_pos)
                .show(ctx, |ui| {
                    Frame::new()
                        .fill(p.elevated)
                        .stroke(Stroke::new(1.0, p.border))
                        .corner_radius(10)
                        .shadow(theme::shadow(p))
                        .inner_margin(Margin::same(6))
                        .show(ui, |ui| {
                            ui.set_width(270.0);
                            egui::ScrollArea::vertical()
                                .max_height(340.0)
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing.y = 2.0;
                                    if matches.is_empty() {
                                        ui.label(
                                            RichText::new("일치하는 블록 명령어가 없습니다")
                                                .size(12.0)
                                                .color(p.faint),
                                        );
                                    }
                                    let mut group = None;
                                    for (n, &i) in matches.iter().enumerate() {
                                        let it = &SLASH[i];
                                        if group != Some(it.advanced) {
                                            group = Some(it.advanced);
                                            let g = if it.advanced {
                                                "고급 및 미디어"
                                            } else {
                                                "기본 블록"
                                            };
                                            ui.label(
                                                RichText::new(g).size(11.0).strong().color(p.faint),
                                            );
                                        }
                                        if slash_row(ui, p, it, n == sel_idx).clicked() {
                                            slash = Some(i);
                                        }
                                    }
                                });
                        });
                });
            interacted |= slash.is_some()
                || ctx
                    .input(|i| i.pointer.interact_pos())
                    .is_some_and(|pos| slash_resp.response.rect.contains(pos));
        }

        // 편집 중인 블록이 Mermaid 펜스면 코드 입력란 바로 아래에 실시간 다이어그램 미리보기를 함께 렌더링한다
        let mut total_rect =
            code_frame_rect.map_or(out.response.rect, |fr| fr.union(out.response.rect));
        let bt = buf.trim_start();
        if (bt.starts_with("```") || bt.starts_with("~~~"))
            && bt[3..]
                .lines()
                .next()
                .is_some_and(|l| l.trim().eq_ignore_ascii_case("mermaid"))
        {
            let fence = &bt[..3];
            let first_nl = bt.find('\n').unwrap_or(bt.len());
            let body_part = if first_nl < bt.len() {
                &bt[first_nl + 1..]
            } else {
                ""
            };
            let m_code = body_part
                .trim_end()
                .strip_suffix(fence)
                .unwrap_or(body_part)
                .trim_end_matches(['\n', '\r']);
            let preview_w = ui.available_width();
            let preview_resp = ui.vertical(|ui| {
                let _ = card(
                    ui,
                    p,
                    Some("Mermaid 다이어그램 (실시간 미리보기)"),
                    false,
                    |ui| {
                        let _ = crate::mermaid::render(
                            ui,
                            p,
                            m_code,
                            body,
                            (preview_w - 44.0).max(160.0),
                        );
                    },
                );
            });
            total_rect = total_rect.union(preview_resp.response.rect);
        }

        let changed = if is_changed {
            Some(buf)
        } else {
            buf.clear();
            self.edit_buf = buf;
            None
        };
        EditorOut {
            rect: total_rect,
            changed,
            fmt,
            slash,
            interacted,
            deactivate,
            delete_block,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_table_editor(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        p: &Pal,
        block_id: egui::Id,
        body: f32,
        mut tbl: MdTable,
        buf: &mut String,
    ) -> (Rect, bool, bool, bool, bool) {
        let cell_id = |r_i: usize, c_i: usize| block_id.with(("tbl-cell", r_i, c_i));
        let cols = tbl.headers.len().max(1);
        let row_count = tbl.rows.len() + 1;
        let focused_id = ctx.memory(|m| m.focused());
        let mut cur_cell: Option<(usize, usize)> = None;
        if let Some(fid) = focused_id {
            'outer: for r_i in 0..row_count {
                for c_i in 0..cols {
                    if cell_id(r_i, c_i) == fid {
                        cur_cell = Some((r_i, c_i));
                        break 'outer;
                    }
                }
            }
        }
        let ime_busy = ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Ime(_))))
            || self.ime_composing;
        let tab_next =
            cur_cell.is_some() && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab));
        let tab_prev =
            cur_cell.is_some() && ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::Tab));
        let enter_down = cur_cell.is_some()
            && !ime_busy
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));

        let mut table_changed = false;
        let mut interacted = false;
        let mut deactivate = false;
        let mut delete_table = false;
        let mut next_focus = self.table_focus_cell.take();
        if std::mem::take(&mut self.focus_editor) && next_focus.is_none() && cur_cell.is_none() {
            next_focus = Some((0, 0));
        }

        if let Some((r_i, c_i)) = cur_cell {
            if tab_next {
                if c_i + 1 < cols {
                    next_focus = Some((r_i, c_i + 1));
                } else if r_i + 1 < row_count {
                    next_focus = Some((r_i + 1, 0));
                } else {
                    tbl.rows.push(vec![String::new(); cols]);
                    table_changed = true;
                    next_focus = Some((row_count, 0));
                }
            } else if tab_prev {
                if c_i > 0 {
                    next_focus = Some((r_i, c_i - 1));
                } else if r_i > 0 {
                    next_focus = Some((r_i - 1, cols - 1));
                }
            } else if enter_down {
                if r_i + 1 < row_count {
                    next_focus = Some((r_i + 1, c_i));
                } else {
                    tbl.rows.push(vec![String::new(); cols]);
                    table_changed = true;
                    next_focus = Some((row_count, c_i));
                }
            }
        }

        let wrap_out = ui.vertical(|ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;

            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(5.0, 4.0);
                Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(6)
                    .inner_margin(Margin::symmetric(8, 3))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 5.0;
                        ui.add(img(theme::TABLE, 12.5, p.accent));
                        ui.label(RichText::new("표").size(11.5).strong().color(p.text));
                        ui.label(
                            RichText::new(format!("{cols}×{row_count}"))
                                .family(FontFamily::Monospace)
                                .size(10.5)
                                .color(p.faint),
                        );
                    });
                let tool_btn = |ui: &mut egui::Ui, label: &str, color: Color32, tip: &str| {
                    ui.add(
                        egui::Button::new(RichText::new(label).size(11.0).color(color))
                            .fill(p.surface)
                            .stroke(Stroke::new(1.0, p.border))
                            .corner_radius(6)
                            .min_size(vec2(0.0, 23.0)),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .on_hover_text(tip)
                };
                if tool_btn(
                    ui,
                    "+ 행",
                    p.text,
                    "맨 아래에 새 행 추가 (마지막 셀에서 Tab)",
                )
                .clicked()
                {
                    tbl.rows.push(vec![String::new(); cols]);
                    table_changed = true;
                    next_focus = Some((tbl.rows.len(), 0));
                    interacted = true;
                }
                if tool_btn(ui, "+ 열", p.text, "오른쪽에 새 열 추가").clicked() {
                    let c_new = tbl.headers.len();
                    tbl.headers.push(format!("열 {}", c_new + 1));
                    tbl.aligns.push(TableAlign::None);
                    for row in &mut tbl.rows {
                        row.push(String::new());
                    }
                    table_changed = true;
                    next_focus = Some((0, c_new));
                    interacted = true;
                }
                if !tbl.rows.is_empty()
                    && tool_btn(ui, "- 행", p.muted, "선택(또는 마지막) 행 삭제").clicked()
                {
                    let rm = cur_cell
                        .and_then(|(r_i, _)| r_i.checked_sub(1))
                        .unwrap_or(tbl.rows.len() - 1)
                        .min(tbl.rows.len() - 1);
                    tbl.rows.remove(rm);
                    table_changed = true;
                    next_focus = Some((rm.min(tbl.rows.len()), 0));
                    interacted = true;
                }
                if cols > 1 && tool_btn(ui, "- 열", p.muted, "선택(또는 마지막) 열 삭제").clicked()
                {
                    let rm = cur_cell.map_or(cols - 1, |(_, c_i)| c_i).min(cols - 1);
                    tbl.headers.remove(rm);
                    if rm < tbl.aligns.len() {
                        tbl.aligns.remove(rm);
                    }
                    for row in &mut tbl.rows {
                        if rm < row.len() {
                            row.remove(rm);
                        }
                    }
                    table_changed = true;
                    next_focus = Some((0, rm.min(tbl.headers.len().saturating_sub(1))));
                    interacted = true;
                }
                let align_col = cur_cell.map_or(0, |(_, c_i)| c_i).min(cols - 1);
                let align_lbl = match tbl
                    .aligns
                    .get(align_col)
                    .copied()
                    .unwrap_or(TableAlign::None)
                {
                    TableAlign::None => "정렬: 기본",
                    TableAlign::Left => "정렬: 왼쪽",
                    TableAlign::Center => "정렬: 가운데",
                    TableAlign::Right => "정렬: 오른쪽",
                };
                if tool_btn(ui, align_lbl, p.muted, "현재 열의 정렬 방향 전환").clicked()
                {
                    if align_col >= tbl.aligns.len() {
                        tbl.aligns.resize(cols, TableAlign::None);
                    }
                    tbl.aligns[align_col] = match tbl.aligns[align_col] {
                        TableAlign::None => TableAlign::Left,
                        TableAlign::Left => TableAlign::Center,
                        TableAlign::Center => TableAlign::Right,
                        TableAlign::Right => TableAlign::None,
                    };
                    table_changed = true;
                    next_focus = cur_cell.or(Some((0, align_col)));
                    interacted = true;
                }
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    if table_delete_btn(ui).clicked() {
                        delete_table = true;
                        interacted = true;
                    }
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("완료 (Esc)").size(11.0).color(p.accent),
                            )
                            .fill(p.accent.gamma_multiply(0.12))
                            .stroke(Stroke::new(1.0, p.accent.gamma_multiply(0.35)))
                            .corner_radius(6)
                            .min_size(vec2(0.0, 23.0)),
                        )
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .on_hover_text("표 편집 마치기 (Esc)")
                        .clicked()
                    {
                        deactivate = true;
                        interacted = true;
                    }
                    if tool_btn(ui, "마크다운 보기", p.muted, "마크다운 원문(| |)으로 전환")
                        .clicked()
                    {
                        self.raw_table_mode = true;
                        self.focus_editor = true;
                        interacted = true;
                    }
                });
            });

            let avail_w = ui.available_width().max(240.0);
            let cur_cols = tbl.headers.len().max(1);
            let (col_w, total_w) = table_col_width(cur_cols, avail_w);
            let cell_size = body * 0.92;
            let cell_margin = Margin::symmetric(12, 8);
            let inner_w = (col_w - cell_margin.sum().x).max(56.0);

            egui::ScrollArea::horizontal()
                .id_salt(("tbl-scroll", block_id))
                .show(ui, |ui| {
                    let grid_left = ui.cursor().min.x;
                    let grid_top = ui.cursor().min.y;
                    let mut row_shapes: Vec<(egui::layers::ShapeIdx, f32, f32)> =
                        Vec::with_capacity(row_count);
                    let grid_resp = egui::Grid::new(("tbl-grid", block_id))
                        .spacing(vec2(0.0, 0.0))
                        .min_col_width(col_w)
                        .max_col_width(col_w)
                        .show(ui, |ui| {
                            let bg_idx = ui.painter().add(egui::Shape::Noop);
                            let mut row_top = f32::INFINITY;
                            let mut row_bot = f32::NEG_INFINITY;
                            for (c_i, hdr) in tbl.headers.iter_mut().enumerate() {
                                let cid = cell_id(0, c_i);
                                let align2 = table_align2(
                                    tbl.aligns.get(c_i).copied().unwrap_or(TableAlign::None),
                                );
                                let cell_out =
                                    Frame::new().inner_margin(cell_margin).show(ui, |ui| {
                                        ui.set_width(inner_w);
                                        let r = ui.add(
                                            egui::TextEdit::singleline(hdr)
                                                .id(cid)
                                                .frame(Frame::NONE)
                                                .margin(Margin::ZERO)
                                                .desired_width(inner_w)
                                                .align(align2)
                                                .font(FontId::new(cell_size, theme::bold()))
                                                .text_color(p.text)
                                                .lock_focus(true)
                                                .hint_text(
                                                    RichText::new("제목")
                                                        .size(cell_size)
                                                        .color(p.faint),
                                                ),
                                        );
                                        if next_focus == Some((0, c_i)) {
                                            r.request_focus();
                                        }
                                        if r.changed() {
                                            table_changed = true;
                                        }
                                        if r.clicked() || r.has_focus() {
                                            interacted = true;
                                        }
                                    });
                                row_top = row_top.min(cell_out.response.rect.top());
                                row_bot = row_bot.max(cell_out.response.rect.bottom());
                                if cell_out.response.clicked() {
                                    ctx.memory_mut(|m| m.request_focus(cid));
                                    interacted = true;
                                }
                            }
                            row_shapes.push((bg_idx, row_top, row_bot));
                            ui.end_row();

                            for (r_off, row) in tbl.rows.iter_mut().enumerate() {
                                let r_i = r_off + 1;
                                let bg_idx = ui.painter().add(egui::Shape::Noop);
                                let mut row_top = f32::INFINITY;
                                let mut row_bot = f32::NEG_INFINITY;
                                for (c_i, cell) in row.iter_mut().enumerate() {
                                    let cid = cell_id(r_i, c_i);
                                    let align2 = table_align2(
                                        tbl.aligns.get(c_i).copied().unwrap_or(TableAlign::None),
                                    );
                                    let cell_out =
                                        Frame::new().inner_margin(cell_margin).show(ui, |ui| {
                                            ui.set_width(inner_w);
                                            let r = ui.add(
                                                egui::TextEdit::singleline(cell)
                                                    .id(cid)
                                                    .frame(Frame::NONE)
                                                    .margin(Margin::ZERO)
                                                    .desired_width(inner_w)
                                                    .align(align2)
                                                    .font(FontId::proportional(cell_size))
                                                    .text_color(p.text)
                                                    .lock_focus(true)
                                                    .hint_text(
                                                        RichText::new("내용")
                                                            .size(cell_size)
                                                            .color(p.faint),
                                                    ),
                                            );
                                            if next_focus == Some((r_i, c_i)) {
                                                r.request_focus();
                                            }
                                            if r.changed() {
                                                table_changed = true;
                                            }
                                            if r.clicked() || r.has_focus() {
                                                interacted = true;
                                            }
                                        });
                                    row_top = row_top.min(cell_out.response.rect.top());
                                    row_bot = row_bot.max(cell_out.response.rect.bottom());
                                    if cell_out.response.clicked() {
                                        ctx.memory_mut(|m| m.request_focus(cid));
                                        interacted = true;
                                    }
                                }
                                row_shapes.push((bg_idx, row_top, row_bot));
                                ui.end_row();
                            }
                        });
                    let last_bot = row_shapes
                        .last()
                        .map_or(grid_resp.response.rect.bottom(), |x| x.2);
                    let full_rect = Rect::from_min_max(
                        pos2(grid_left, grid_top),
                        pos2(grid_left + total_w, last_bot),
                    );
                    paint_table_chrome(
                        ui,
                        p,
                        full_rect,
                        col_w,
                        cur_cols,
                        &row_shapes,
                        p.border_hover,
                        cur_cell,
                    );
                });
        });

        if table_changed {
            *buf = serialize_md_table(&tbl);
        }
        if next_focus.is_some() {
            ctx.request_repaint();
        }
        interacted |= ui.rect_contains_pointer(wrap_out.response.rect);
        (
            wrap_out.response.rect,
            table_changed,
            interacted,
            deactivate,
            delete_table,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn render_table_view(
        &mut self,
        ui: &mut egui::Ui,
        p: &Pal,
        tbl: &MdTable,
        block_idx: usize,
        body: f32,
        width: f32,
        scheme: &str,
    ) {
        let cols = tbl.headers.len().max(1);
        let row_count = tbl.rows.len() + 1;
        let (col_w, total_w) = table_col_width(cols, width);
        let cell_size = body * 0.92;
        let cell_margin = Margin::symmetric(12, 8);
        let inner_w = (col_w - cell_margin.sum().x).max(56.0);
        let dark = self.dark;

        ui.add_space(4.0);
        egui::ScrollArea::horizontal()
            .id_salt(("tbl-view-scroll", block_idx))
            .show(ui, |ui| {
                let grid_left = ui.cursor().min.x;
                let grid_top = ui.cursor().min.y;
                let mut row_shapes: Vec<(egui::layers::ShapeIdx, f32, f32)> =
                    Vec::with_capacity(row_count);
                let grid_resp = egui::Grid::new(("tbl-view-grid", block_idx))
                    .spacing(vec2(0.0, 0.0))
                    .min_col_width(col_w)
                    .max_col_width(col_w)
                    .show(ui, |ui| {
                        let bg_idx = ui.painter().add(egui::Shape::Noop);
                        let mut row_top = f32::INFINITY;
                        let mut row_bot = f32::NEG_INFINITY;
                        for (c_i, hdr) in tbl.headers.iter().enumerate() {
                            let align = table_egui_align(
                                tbl.aligns.get(c_i).copied().unwrap_or(TableAlign::None),
                            );
                            let cell_out = Frame::new().inner_margin(cell_margin).show(ui, |ui| {
                                ui.set_width(inner_w);
                                ui.with_layout(egui::Layout::top_down(align), |ui| {
                                    render_inline_md_cached(
                                        ui,
                                        &mut self.inline_galley_cache,
                                        hdr,
                                        cell_size,
                                        inner_w,
                                        p,
                                        dark,
                                        true,
                                        scheme,
                                    );
                                });
                            });
                            row_top = row_top.min(cell_out.response.rect.top());
                            row_bot = row_bot.max(cell_out.response.rect.bottom());
                        }
                        row_shapes.push((bg_idx, row_top, row_bot));
                        ui.end_row();

                        for row in &tbl.rows {
                            let bg_idx = ui.painter().add(egui::Shape::Noop);
                            let mut row_top = f32::INFINITY;
                            let mut row_bot = f32::NEG_INFINITY;
                            for (c_i, cell) in row.iter().enumerate() {
                                let align = table_egui_align(
                                    tbl.aligns.get(c_i).copied().unwrap_or(TableAlign::None),
                                );
                                let cell_out =
                                    Frame::new().inner_margin(cell_margin).show(ui, |ui| {
                                        ui.set_width(inner_w);
                                        ui.with_layout(egui::Layout::top_down(align), |ui| {
                                            render_inline_md_cached(
                                                ui,
                                                &mut self.inline_galley_cache,
                                                cell,
                                                cell_size,
                                                inner_w,
                                                p,
                                                dark,
                                                false,
                                                scheme,
                                            );
                                        });
                                    });
                                row_top = row_top.min(cell_out.response.rect.top());
                                row_bot = row_bot.max(cell_out.response.rect.bottom());
                            }
                            row_shapes.push((bg_idx, row_top, row_bot));
                            ui.end_row();
                        }
                    });
                let last_bot = row_shapes
                    .last()
                    .map_or(grid_resp.response.rect.bottom(), |x| x.2);
                let full_rect = Rect::from_min_max(
                    pos2(grid_left, grid_top),
                    pos2(grid_left + total_w, last_bot),
                );
                paint_table_chrome(ui, p, full_rect, col_w, cols, &row_shapes, p.border, None);
            });
        ui.add_space(4.0);
    }

    #[allow(clippy::too_many_arguments)]
    fn render_block(
        &mut self,
        ui: &mut egui::Ui,
        p: &Pal,
        text: &str,
        r: Range<usize>,
        block_idx: usize,
        body: f32,
        width: f32,
        scheme: &str,
    ) -> BlockOut {
        let src = &text[r.clone()];
        let t = src.trim_start();
        let mut changed = None;
        let mut consumed = false;
        let dark = self.dark;
        let resp = ui
            .push_id(("blk", block_idx), |ui| {
                ui.vertical(|ui| {
                    if let Some(level) = heading_level(t) {
                        ui.add_space(HEADING_TOP[level - 1]);
                        let title = t[level..].trim().trim_end_matches('#').trim();
                        let h_size = body * HEADING_SCALE[level - 1];
                        if title
                            .bytes()
                            .any(|b| matches!(b, b'*' | b'_' | b'`' | b'~' | b'='))
                        {
                            render_inline_md_cached(
                                ui,
                                &mut self.inline_galley_cache,
                                title,
                                h_size,
                                width,
                                p,
                                dark,
                                true,
                                scheme,
                            );
                        } else {
                            ui.label(
                                RichText::new(title)
                                    .font(FontId::new(h_size, theme::bold()))
                                    .color(p.text),
                            );
                        }
                    } else if let Some(rest) = t.strip_prefix(":::toggle") {
                        let first = rest.find('\n').unwrap_or(rest.len());
                        let title = rest[..first].trim();
                        let body_start = (src.len() - t.len()) + ":::toggle".len() + first + 1;
                        let body_end = src
                            .trim_end()
                            .strip_suffix(":::")
                            .map_or(src.len(), str::len);
                        let inner = src.get(body_start.min(body_end)..body_end).unwrap_or("");
                        let open_id = ui.make_persistent_id("toggle-open");
                        let mut open = ui.data(|d| d.get_temp(open_id).unwrap_or(false));
                        Frame::new()
                            .fill(p.surface)
                            .stroke(Stroke::new(1.0, p.border))
                            .corner_radius(8)
                            .inner_margin(Margin::symmetric(12, 9))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 7.0;
                                    let arrow = ui
                                        .add(
                                            egui::Button::new(
                                                RichText::new(if open { "▼" } else { "▶" })
                                                    .size(10.5)
                                                    .color(if open { p.accent } else { p.muted }),
                                            )
                                            .fill(if open {
                                                p.accent.gamma_multiply(0.10)
                                            } else {
                                                Color32::TRANSPARENT
                                            })
                                            .stroke(Stroke::NONE)
                                            .corner_radius(5)
                                            .min_size(vec2(22.0, 22.0)),
                                        )
                                        .on_hover_cursor(CursorIcon::PointingHand)
                                        .on_hover_text("클릭하여 펼치기/접기");
                                    if arrow.clicked() {
                                        open = !open;
                                        consumed = true;
                                        ui.data_mut(|d| d.insert_temp(open_id, open));
                                    }
                                    ui.label(
                                        RichText::new(title)
                                            .font(FontId::new(body, theme::bold()))
                                            .color(p.text),
                                    );
                                });
                                if open {
                                    ui.add_space(6.0);
                                    let inner_resp = ui.horizontal(|ui| {
                                        ui.add_space(22.0);
                                        ui.vertical(|ui| {
                                            self.markdown(
                                                ui,
                                                p,
                                                body,
                                                inner,
                                                width - 64.0,
                                                scheme,
                                                dark,
                                            )
                                        })
                                        .inner
                                    });
                                    let rr = inner_resp.response.rect;
                                    ui.painter().vline(
                                        rr.left() + 10.0,
                                        rr.y_range(),
                                        Stroke::new(2.0, p.accent.gamma_multiply(0.35)),
                                    );
                                    if let Some(new_inner) = inner_resp.inner {
                                        changed = Some(format!(
                                            "{}{}{}",
                                            &src[..body_start],
                                            new_inner,
                                            &src[body_end..]
                                        ));
                                    }
                                }
                            });
                    } else if t.starts_with("$$") {
                        let inner = t
                            .trim_start_matches('$')
                            .trim_end()
                            .trim_end_matches('$')
                            .trim();
                        let _ = card(ui, p, None, false, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new(inner)
                                        .family(FontFamily::Monospace)
                                        .size(body)
                                        .color(p.text),
                                );
                            });
                        });
                    } else if t.starts_with("```") || t.starts_with("~~~") {
                        let fence = &t[..3];
                        let first_nl = t.find('\n').unwrap_or(t.len());
                        let lang = t[3..first_nl].trim();
                        let body_part = if first_nl < t.len() {
                            &t[first_nl + 1..]
                        } else {
                            ""
                        };
                        let code = body_part
                            .trim_end()
                            .strip_suffix(fence)
                            .unwrap_or(body_part)
                            .trim_end_matches(['\n', '\r']);
                        if lang.eq_ignore_ascii_case("mermaid") {
                            if card(ui, p, Some("Mermaid 다이어그램"), true, |ui| {
                                if !crate::mermaid::render(
                                    ui,
                                    p,
                                    code,
                                    body,
                                    (width - 44.0).max(160.0),
                                ) {
                                    ui.label(
                                        RichText::new(code)
                                            .family(FontFamily::Monospace)
                                            .size(body * 0.84)
                                            .color(p.text),
                                    );
                                }
                            }) {
                                ui.ctx().copy_text(code.to_string());
                                self.status = "코드를 복사했습니다".into();
                                consumed = true;
                            }
                        } else {
                            let line_count = if code.is_empty() {
                                1
                            } else {
                                code.split('\n').count()
                            };
                            let mut copied = false;
                            ui.add_space(6.0);
                            Frame::new()
                                .fill(p.code_bg)
                                .stroke(Stroke::new(1.0, p.border))
                                .corner_radius(8)
                                .inner_margin(Margin::ZERO)
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    Frame::new()
                                        .fill(p.surface)
                                        .corner_radius(egui::CornerRadius {
                                            nw: 8,
                                            ne: 8,
                                            sw: 0,
                                            se: 0,
                                        })
                                        .inner_margin(Margin::symmetric(14, 7))
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                let lang_badge =
                                                    if lang.is_empty() { "CODE" } else { lang };
                                                Frame::new()
                                                    .fill(p.elevated)
                                                    .stroke(Stroke::new(1.0, p.border))
                                                    .corner_radius(5)
                                                    .inner_margin(Margin::symmetric(6, 2))
                                                    .show(ui, |ui| {
                                                        ui.label(
                                                            RichText::new(lang_badge)
                                                                .family(FontFamily::Monospace)
                                                                .size(10.5)
                                                                .strong()
                                                                .color(if lang.is_empty() {
                                                                    p.muted
                                                                } else {
                                                                    p.accent
                                                                }),
                                                        );
                                                    });
                                                ui.label(
                                                    RichText::new(format!("{line_count}줄"))
                                                        .size(11.0)
                                                        .color(p.faint),
                                                );
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(Align::Center),
                                                    |ui| {
                                                        let btn = ui
                                                            .add(
                                                                egui::Button::image_and_text(
                                                                    img(theme::COPY, 11.5, p.muted),
                                                                    RichText::new("복사")
                                                                        .size(11.0)
                                                                        .color(p.muted),
                                                                )
                                                                .fill(p.elevated)
                                                                .stroke(Stroke::new(1.0, p.border))
                                                                .corner_radius(6)
                                                                .min_size(vec2(52.0, 22.0)),
                                                            )
                                                            .on_hover_cursor(
                                                                CursorIcon::PointingHand,
                                                            )
                                                            .on_hover_text("코드 원문 복사");
                                                        if btn.clicked() {
                                                            copied = true;
                                                        }
                                                    },
                                                );
                                            });
                                        });
                                    hline(ui, p);
                                    Frame::new().inner_margin(Margin::symmetric(12, 10)).show(
                                        ui,
                                        |ui| {
                                            use std::fmt::Write;
                                            let avail_w = ui.available_width().max(160.0);
                                            let digits = line_count
                                                .checked_ilog10()
                                                .map_or(1, |d| d as usize + 1)
                                                .max(2);
                                            let code_size = body * 0.84;
                                            let code_lh = code_size * 1.5;
                                            let gutter_w =
                                                (digits as f32) * (code_size * 0.62) + 12.0;
                                            let text_gap = 12.0;
                                            let code_w = (avail_w - gutter_w - text_gap).max(100.0);
                                            let ppp_bits = ui.ctx().pixels_per_point().to_bits();
                                            let c_key = cache_hash((
                                                code,
                                                lang,
                                                code_w.to_bits(),
                                                code_size.to_bits(),
                                                dark,
                                                ppp_bits,
                                            ));
                                            let cached = self
                                                .code_galley_cache
                                                .get(&c_key)
                                                .filter(|(g, _)| g.text() == code)
                                                .map(|(g, ys)| {
                                                    (std::sync::Arc::clone(g), ys.clone())
                                                });
                                            let (galley, line_ys) = match cached {
                                                Some(hit) => hit,
                                                None => {
                                                    let job = build_code_layout_job(
                                                        ui.ctx(),
                                                        code,
                                                        lang,
                                                        code_size,
                                                        code_w,
                                                        code_lh,
                                                        p,
                                                        dark,
                                                    );
                                                    let g =
                                                        ui.ctx().fonts_mut(|f| f.layout_job(job));
                                                    let mut ys = Vec::with_capacity(line_count);
                                                    let mut at_line_start = true;
                                                    for row in &g.rows {
                                                        if at_line_start {
                                                            ys.push(row.rect().center().y);
                                                        }
                                                        at_line_start = row.ends_with_newline;
                                                    }
                                                    if self.code_galley_cache.len() >= 128 {
                                                        self.code_galley_cache.clear();
                                                    }
                                                    self.code_galley_cache.insert(
                                                        c_key,
                                                        (std::sync::Arc::clone(&g), ys.clone()),
                                                    );
                                                    (g, ys)
                                                }
                                            };
                                            let total_h = galley.size().y.max(code_lh);
                                            let (rect, code_resp) = ui.allocate_exact_size(
                                                vec2(avail_w, total_h),
                                                Sense::hover(),
                                            );
                                            let div_x = rect.left() + gutter_w;
                                            ui.painter().vline(
                                                div_x,
                                                rect.y_range(),
                                                Stroke::new(1.0, p.border),
                                            );
                                            let num_font = FontId::monospace(code_size * 0.88);
                                            for (idx, &y_off) in line_ys.iter().enumerate() {
                                                self.line_num_buf.clear();
                                                let _ = write!(self.line_num_buf, "{}", idx + 1);
                                                ui.painter().text(
                                                    pos2(div_x - 8.0, rect.top() + y_off),
                                                    Align2::RIGHT_CENTER,
                                                    &self.line_num_buf,
                                                    num_font.clone(),
                                                    p.faint,
                                                );
                                            }
                                            ui.painter().galley(
                                                pos2(div_x + text_gap, rect.top()),
                                                galley,
                                                p.text,
                                            );
                                            code_resp.widget_info(|| {
                                                egui::WidgetInfo::labeled(
                                                    egui::Role::Label,
                                                    ui.is_enabled(),
                                                    code,
                                                )
                                            });
                                        },
                                    );
                                });
                            ui.add_space(6.0);
                            if copied {
                                ui.ctx().copy_text(code.to_string());
                                self.status = "코드를 복사했습니다".into();
                                consumed = true;
                            }
                        }
                    } else if let Some(tbl) = parse_md_table_cached(&mut self.table_cache, t) {
                        self.render_table_view(ui, p, &tbl, block_idx, body, width, scheme);
                    } else {
                        let t_end = t.trim_end();
                        let plain_para = !t_end.is_empty()
                            && !t_end.contains('\n')
                            && src.len() - t.len() < 4
                            && list_marker_bytes(t).is_none()
                            && !matches!(
                                t_end.as_bytes().first(),
                                Some(b'>' | b'|' | b'-' | b'+' | b'#')
                            )
                            && !t_end.bytes().any(|b| {
                                matches!(
                                    b,
                                    b'*' | b'_'
                                        | b'`'
                                        | b'~'
                                        | b'['
                                        | b'='
                                        | b'<'
                                        | b'!'
                                        | b'$'
                                        | b'\\'
                                        | b'&'
                                )
                            })
                            && !t_end.contains("://");
                        if plain_para {
                            ui.label(
                                RichText::new(t_end)
                                    .size(body)
                                    .line_height(Some(body * 1.6))
                                    .color(p.text),
                            );
                        } else {
                            changed = self.markdown(ui, p, body, src, width, scheme, dark);
                        }
                    }
                })
                .response
            })
            .inner;
        BlockOut {
            rect: resp.rect,
            changed,
            consumed,
        }
    }

    // Markdown 조각 렌더링. 체크박스를 누르면 바뀐 원문을 돌려준다.
    #[allow(clippy::too_many_arguments)]
    fn markdown(
        &mut self,
        ui: &mut egui::Ui,
        p: &Pal,
        body: f32,
        src: &str,
        width: f32,
        scheme: &str,
        dark: bool,
    ) -> Option<String> {
        let before = preprocess(src);
        if before.contains("![") {
            let math = |ui: &mut egui::Ui, s: &str, _inline: bool| {
                ui.code(s.trim());
            };
            let mut viewer = CommonMarkViewer::new()
                .default_implicit_uri_scheme(scheme)
                .max_image_width(Some(width as usize))
                .render_math_fn(Some(&math));
            if src.contains("[!") {
                viewer = viewer.alerts(alerts(dark));
            }
            let has_checkbox =
                before.contains("[ ]") || before.contains("[x]") || before.contains("[X]");
            if !has_checkbox {
                viewer.show(ui, &mut self.md_cache, &before);
                return None;
            }
            let mut shown = before.clone().into_owned();
            viewer.show_mut(ui, &mut self.md_cache, &mut shown);
            if shown == *before {
                return None;
            }
            let mut s = src.to_string();
            return apply_preview_change(&mut s, &before, &shown).then_some(s);
        }
        let mut rctx = MdRenderCtx {
            p,
            raw: src,
            body,
            scheme,
            dark,
            cache: &mut self.inline_galley_cache,
        };
        let toggled = render_markdown_native(ui, &mut rctx, &before, width);
        let (span, new_checked) = toggled?;
        let mut shown = before.clone().into_owned();
        let inner = shown.get(span.clone())?;
        let off = inner.find([' ', 'x', 'X'])?;
        let byte_idx = span.start + off;
        // ' ', 'x', 'X' 모두 1바이트 ASCII이므로 바이트 길이·경계가 유지된다
        unsafe {
            shown.as_bytes_mut()[byte_idx] = if new_checked { b'x' } else { b' ' };
        }
        let mut s = src.to_string();
        apply_preview_change(&mut s, &before, &shown).then_some(s)
    }

    // ---------- 떠 있는 찾기·바꾸기 막대 (오른쪽 위) ----------
    fn find_bar(&mut self, ctx: &egui::Context, p: &Pal, area: Rect) {
        if !self.find.open {
            return;
        }
        let sel = self.cursor(ctx);
        let matches = self
            .current
            .as_ref()
            .and_then(|n| self.docs.get(n))
            .map(|d| self.find.matches(d).to_vec())
            .unwrap_or_default();
        let mut action: Option<u8> = None; // 0 이전, 1 다음, 2 바꾸기, 3 모두 바꾸기
        egui::Area::new(egui::Id::unique("find-bar"))
            .order(Order::Foreground)
            .pivot(Align2::RIGHT_TOP)
            .fixed_pos(area.right_top() + vec2(-16.0, 12.0))
            .show(ctx, |ui| {
                Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(10)
                    .shadow(theme::shadow(p))
                    .inner_margin(Margin::symmetric(8, 7))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = vec2(4.0, 6.0);
                        ui.horizontal(|ui| {
                            let id = egui::Id::unique("find-input");
                            let focused = ui.memory(|m| m.has_focus(id));
                            let r = Frame::new()
                                .fill(p.elevated)
                                .stroke(Stroke::new(1.0, if focused { p.accent } else { p.border }))
                                .corner_radius(6)
                                .inner_margin(Margin::symmetric(8, 4))
                                .show(ui, |ui| {
                                    ui.add(img(theme::SEARCH, 14.0, p.faint));
                                    let r = ui.add(
                                        egui::TextEdit::singleline(&mut self.find.query)
                                            .id(id)
                                            .frame(Frame::NONE)
                                            .margin(Margin::ZERO)
                                            .hint_text(
                                                RichText::new("찾기... (Enter / Shift+Enter)")
                                                    .color(p.faint),
                                            )
                                            .desired_width(170.0),
                                    );
                                    let idx =
                                        matches.iter().position(|&m| m == sel).map_or(0, |i| i + 1);
                                    ui.label(
                                        RichText::new(format!("{idx}/{}", matches.len()))
                                            .family(FontFamily::Monospace)
                                            .size(11.0)
                                            .color(p.faint),
                                    );
                                    r
                                })
                                .inner;
                            if std::mem::take(&mut self.find.focus) {
                                r.request_focus();
                            }
                            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                                action = Some(if ui.input(|i| i.modifiers.shift) {
                                    0
                                } else {
                                    1
                                });
                                r.request_focus();
                            }
                            let case_color = if self.find.case { p.accent } else { p.muted };
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("Aa").size(12.0).color(case_color),
                                    )
                                    .min_size(vec2(28.0, 26.0)),
                                )
                                .on_hover_text("대소문자 구분 (Case Sensitive)")
                                .clicked()
                            {
                                self.find.case = !self.find.case;
                            }
                            if ui
                                .add(
                                    egui::Button::new(RichText::new("▲").size(11.0).color(p.muted))
                                        .min_size(vec2(26.0, 26.0)),
                                )
                                .on_hover_text("이전 일치 항목 (Shift+Enter)")
                                .clicked()
                            {
                                action = Some(0);
                            }
                            if ui
                                .add(
                                    egui::Button::new(RichText::new("▼").size(11.0).color(p.muted))
                                        .min_size(vec2(26.0, 26.0)),
                                )
                                .on_hover_text("다음 일치 항목 (Enter)")
                                .clicked()
                            {
                                action = Some(1);
                            }
                            let rc = if self.find.replace { p.accent } else { p.muted };
                            if small_icon_btn(
                                ui,
                                p,
                                theme::REPLACE,
                                rc,
                                "바꾸기 펼치기/접기 (Ctrl+H)",
                            )
                            .clicked()
                            {
                                self.find.replace = !self.find.replace;
                            }
                            if small_icon_btn(ui, p, theme::X, p.muted, "닫기 (Esc)").clicked() {
                                self.find.open = false;
                                if self.active.is_some() {
                                    self.focus_editor = true;
                                }
                            }
                        });
                        if self.find.replace {
                            ui.horizontal(|ui| {
                                let rw = Frame::new()
                                    .fill(p.elevated)
                                    .stroke(Stroke::new(1.0, p.border))
                                    .corner_radius(6)
                                    .inner_margin(Margin::symmetric(8, 4))
                                    .show(ui, |ui| {
                                        ui.add(
                                            egui::TextEdit::singleline(&mut self.find.with)
                                                .frame(Frame::NONE)
                                                .margin(Margin::ZERO)
                                                .hint_text(
                                                    RichText::new("바꿀 내용...").color(p.faint),
                                                )
                                                .desired_width(208.0),
                                        )
                                    })
                                    .inner;
                                if rw.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                                    action = Some(2);
                                    rw.request_focus();
                                }
                                if ui.button(RichText::new("바꾸기").size(11.5)).clicked() {
                                    action = Some(2);
                                }
                                if ui.button(RichText::new("모두 바꾸기").size(11.5)).clicked()
                                {
                                    action = Some(3);
                                }
                            });
                        }
                    });
            });
        match action {
            Some(0) => self.find_step(ctx, false),
            Some(1) => self.find_step(ctx, true),
            Some(2) => self.replace_one(ctx),
            Some(3) => self.replace_everything(ctx),
            _ => {}
        }
    }

    // ---------- 모달 ----------
    fn dialogs(&mut self, ctx: &egui::Context, p: &Pal) {
        let Some(mut dialog) = self.dialog.take() else {
            return;
        };
        let (mut close, mut cancel) = (false, false);
        let mut error: Option<String> = None;
        let mut next: Option<Dialog> = None;
        let (width, fill) = match dialog {
            Dialog::Files { .. } | Dialog::Help => (660.0, p.surface),
            Dialog::Image { .. } => (480.0, p.elevated),
            _ => (440.0, p.elevated),
        };
        let kind = match dialog {
            Dialog::Files { .. } => 0,
            Dialog::NewFile(_) => 1,
            Dialog::SaveScratch(_) => 2,
            Dialog::Rename { .. } => 3,
            Dialog::Delete(_) => 4,
            Dialog::Unsaved(_) => 5,
            Dialog::Image { .. } => 6,
            Dialog::Help => 7,
        };
        // 종류마다 다른 id: 같은 id면 이전 창 크기를 기억해 레이아웃이 늘어난다
        let modal = egui::Modal::new(egui::Id::unique(("dialog", kind)))
            .backdrop_color(p.overlay)
            .frame(
                Frame::new()
                    .fill(fill)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(14)
                    .shadow(theme::shadow(p)),
            )
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing.y = 0.0;
                match &mut dialog {
                    Dialog::Files { filter, sel } => {
                        self.files_modal(ui, p, filter, sel, &mut close, &mut next);
                    }
                    Dialog::NewFile(new_name) | Dialog::SaveScratch(new_name) => {
                        let save_mode = matches!(kind, 2);
                        let (icon, title, btn, input_id) = if save_mode {
                            (theme::SAVE, "파일로 저장", "저장", "save-scratch")
                        } else {
                            (theme::PLUS, "새 파일", "만들기", "new-file")
                        };
                        cancel |= modal_header(ui, p, icon, p.accent, title);
                        let mut submit = false;
                        Frame::new()
                            .inner_margin(Margin::symmetric(20, 18))
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 8.0;
                                ui.label(
                                    RichText::new("파일 이름 (하위 폴더 가능, 예: notes/할일.md)")
                                        .size(12.0)
                                        .strong()
                                        .color(p.muted),
                                );
                                let r = input(
                                    ui,
                                    p,
                                    egui::Id::unique(input_id),
                                    new_name,
                                    "새문서.md",
                                    f32::INFINITY,
                                );
                                r.request_focus();
                                submit = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                            });
                        footer(ui, p, None, |ui| {
                            submit |= primary(ui, p, btn).clicked();
                            cancel |= ghost(ui, p, "취소", None).clicked();
                        });
                        if submit {
                            let res = if save_mode {
                                self.save_scratch_as(new_name)
                            } else {
                                self.create_file(new_name)
                            };
                            match res {
                                Ok(()) => close = true,
                                Err(e) => error = Some(e),
                            }
                        }
                    }
                    Dialog::Rename { from, to } => {
                        cancel |= modal_header(ui, p, theme::FILE, p.accent, "파일 이름 변경");
                        let mut submit = false;
                        Frame::new()
                            .inner_margin(Margin::symmetric(20, 18))
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 8.0;
                                ui.label(
                                    RichText::new(format!("현재: {from}"))
                                        .size(12.0)
                                        .color(p.muted),
                                );
                                let id = egui::Id::unique("rename");
                                let opening = !ui.memory(|m| m.has_focus(id));
                                let r = input(ui, p, id, to, "", f32::INFINITY);
                                r.request_focus();
                                if opening {
                                    // 폴더·확장자는 두고 이름만 선택해 둔다 → 바로 새 이름을 치면 된다
                                    let from_c = to.rfind('/').map_or(0, |i| char_len(&to[..=i]));
                                    let to_c =
                                        to.rfind('.').map_or(char_len(to), |i| char_len(&to[..i]));
                                    let mut st = egui::TextEdit::load_state(ui.ctx(), id)
                                        .unwrap_or_default();
                                    st.cursor.set_char_range(Some(CCursorRange::two(
                                        CCursor::new(from_c),
                                        CCursor::new(to_c.max(from_c)),
                                    )));
                                    egui::TextEdit::store_state(ui.ctx(), id, st);
                                }
                                submit = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                            });
                        footer(ui, p, None, |ui| {
                            submit |= primary(ui, p, "변경").clicked();
                            cancel |= ghost(ui, p, "취소", None).clicked();
                        });
                        if submit {
                            let (f, t) = (from.clone(), to.clone());
                            match self.rename(&f, &t) {
                                Ok(()) => close = true,
                                Err(e) => error = Some(e),
                            }
                        }
                    }
                    Dialog::Delete(name) => {
                        dialog_title(ui, p, RED, "파일 삭제");
                        message(
                            ui,
                            p,
                            &format!("'{name}' 파일을 영구 삭제할까요? 되돌릴 수 없습니다."),
                        );
                        footer(ui, p, Some(p.surface), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("삭제")
                                            .size(13.0)
                                            .color(Color32::WHITE)
                                            .strong(),
                                    )
                                    .fill(RED)
                                    .corner_radius(6)
                                    .min_size(vec2(0.0, 30.0)),
                                )
                                .clicked()
                            {
                                let n = name.clone();
                                self.delete(&n);
                                close = true;
                            }
                            cancel |= ghost(ui, p, "취소", None).clicked();
                        });
                    }
                    Dialog::Unsaved(after) => {
                        dialog_title(ui, p, AMBER, "저장되지 않은 변경사항");
                        let names: Vec<String> = match &after {
                            After::CloseTab(target) => self
                                .docs
                                .get(target)
                                .filter(|d| !d.scratch && d.dirty())
                                .map(|_| vec![target.clone()])
                                .unwrap_or_default(),
                            _ => self
                                .docs
                                .iter()
                                .filter(|(_, d)| !d.scratch && d.dirty())
                                .map(|(n, _)| n.clone())
                                .collect(),
                        };
                        let what = match &after {
                            After::Close => "종료하기",
                            After::CloseTab(_) => "탭을 닫기",
                            After::Switch(..) => "폴더를 바꾸기",
                        };
                        message(
                            ui,
                            p,
                            &format!("{} 전에 저장할까요?\n{}", what, names.join(", ")),
                        );
                        footer(ui, p, Some(p.surface), |ui| {
                            if primary(ui, p, "저장 후 계속").clicked()
                                || ui.input(|i| i.key_pressed(Key::Enter))
                            {
                                let ok = match &after {
                                    After::CloseTab(target) => self.save_quiet(target),
                                    _ => self.save_all(),
                                };
                                if ok {
                                    close = true;
                                } else {
                                    error = Some(self.status.clone());
                                }
                            }
                            if ghost(ui, p, "저장 안 함", Some(RED)).clicked() {
                                match &after {
                                    After::CloseTab(target) => {
                                        if let Some(d) = self.docs.get_mut(target) {
                                            d.text = d.saved.clone();
                                            d.edited_at = None;
                                            d.dirty_flag = false;
                                            d.refresh_meta();
                                        }
                                    }
                                    _ => {
                                        for d in self.docs.values_mut() {
                                            d.text = d.saved.clone();
                                            d.edited_at = None;
                                            d.dirty_flag = false;
                                            d.refresh_meta();
                                        }
                                    }
                                }
                                close = true;
                            }
                            cancel |= ghost(ui, p, "취소", None).clicked();
                        });
                    }
                    Dialog::Image { url_tab, url, alt } => {
                        cancel |= modal_header(ui, p, theme::IMAGE, p.accent, "이미지 삽입");
                        self.image_modal(ui, ctx, p, url_tab, url, alt, &mut close, &mut cancel);
                    }
                    Dialog::Help => {
                        cancel |=
                            modal_header(ui, p, theme::HELP, p.text, "단축키 및 마크다운 가이드");
                        help_body(ui, p);
                        footer(ui, p, None, |ui| {
                            close |= primary(ui, p, "확인").clicked();
                        });
                    }
                }
                if let Some(e) = &error {
                    Frame::new()
                        .inner_margin(Margin::symmetric(20, 8))
                        .show(ui, |ui| {
                            ui.colored_label(RED, e);
                        });
                }
            });
        cancel |= modal.should_close();
        if let Some(n) = next {
            self.dialog = Some(n);
        } else if let Some(e) = error {
            self.status = e;
            self.dialog = Some(dialog);
        } else if !close && !cancel {
            self.dialog = Some(dialog);
        } else if let (false, Dialog::Unsaved(after)) = (cancel, dialog) {
            self.finish(after); // 취소면 닫기/폴더 전환을 하지 않고 대화상자만 닫는다
        }
    }

    fn files_modal(
        &mut self,
        ui: &mut egui::Ui,
        p: &Pal,
        filter: &mut String,
        sel: &mut usize,
        close: &mut bool,
        next: &mut Option<Dialog>,
    ) {
        *close |= modal_header(ui, p, theme::FOLDER, p.text, "작업 폴더 파일 탐색기");
        let needle = filter.to_lowercase();
        let shown: Vec<String> = self
            .files
            .iter()
            .filter(|f| f.to_lowercase().contains(&needle))
            .cloned()
            .collect();
        let mut open: Option<String> = None;
        let mut moved = false; // 방향키로 선택을 옮겼으면 그 행이 보이게 스크롤한다
        Frame::new()
            .inner_margin(Margin {
                left: 20,
                right: 20,
                top: 12,
                bottom: 8,
            })
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.add(
                    egui::Label::new(
                        RichText::new(self.root.display().to_string())
                            .family(FontFamily::Monospace)
                            .size(11.5)
                            .color(p.faint),
                    )
                    .truncate(),
                );
                let r = input(
                    ui,
                    p,
                    egui::Id::unique("file-filter"),
                    filter,
                    "파일명으로 필터링... (↑↓ 이동, Enter 열기)",
                    f32::INFINITY,
                );
                r.request_focus();
                if r.changed() {
                    *sel = 0;
                }
                let n = shown.len().max(1);
                if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowDown)) {
                    *sel = (*sel + 1) % n;
                    moved = true;
                }
                if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp)) {
                    *sel = (*sel + n - 1) % n;
                    moved = true;
                }
                if ui.input(|i| i.key_pressed(Key::Enter)) {
                    open = shown.get(*sel).cloned();
                }
                if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F2)) {
                    if let Some(target) = shown.get(*sel).cloned() {
                        *next = Some(Dialog::Rename {
                            from: target.clone(),
                            to: target,
                        });
                    }
                }
                if (filter.is_empty()
                    && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Delete)))
                    || ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Delete))
                {
                    if let Some(target) = shown.get(*sel).cloned() {
                        *next = Some(Dialog::Delete(target));
                    }
                }
            });
        hline(ui, p);
        let pointer_pos = ui.ctx().input(|i| i.pointer.interact_pos());
        Frame::new()
            .inner_margin(Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                if shown.is_empty() {
                    ui.label(
                        RichText::new("마크다운 파일이 없습니다")
                            .size(13.0)
                            .color(p.faint),
                    );
                } else {
                    egui::ScrollArea::vertical().max_height(364.0).show_rows(
                        ui,
                        32.0,
                        shown.len(),
                        |ui, range| {
                            if moved {
                                let row_y = ui.min_rect().top() + (*sel as f32) * 34.0;
                                ui.scroll_to_rect(
                                    Rect::from_min_size(
                                        pos2(ui.min_rect().left(), row_y),
                                        vec2(10.0, 32.0),
                                    ),
                                    None,
                                );
                            }
                            for i in range {
                                let name = &shown[i];
                                let current = self.current.as_deref() == Some(name.as_str());
                                let (rect, resp) = ui.allocate_exact_size(
                                    vec2(ui.available_width(), 32.0),
                                    Sense::click(),
                                );
                                let hot = resp.hovered() || i == *sel;
                                let bg = if current {
                                    Some(p.accent.gamma_multiply(0.08))
                                } else {
                                    hot.then_some(p.hover)
                                };
                                if let Some(c) = bg {
                                    ui.painter().rect_filled(rect, 6.0, c);
                                }
                                let color = if current { p.accent } else { p.text };
                                img(theme::FILE, 14.0, if current { p.accent } else { p.faint })
                                    .paint_at(
                                        ui,
                                        Rect::from_min_size(
                                            pos2(rect.left() + 10.0, rect.center().y - 7.0),
                                            vec2(14.0, 14.0),
                                        ),
                                    );
                                let (dir, file) = name
                                    .rsplit_once('/')
                                    .map_or(("", name.as_str()), |(d, f)| (d, f));
                                let mut job = egui::text::LayoutJob::default();
                                if !dir.is_empty() {
                                    job.append(
                                        &format!("{dir}/"),
                                        0.0,
                                        egui::TextFormat::simple(
                                            FontId::proportional(11.0),
                                            p.faint,
                                        ),
                                    );
                                }
                                let font = if current {
                                    FontId::new(13.0, theme::bold())
                                } else {
                                    FontId::proportional(13.0)
                                };
                                job.append(file, 0.0, egui::TextFormat::simple(font, color));
                                let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
                                ui.painter().galley(
                                    pos2(
                                        rect.left() + 32.0,
                                        rect.center().y - galley.size().y / 2.0,
                                    ),
                                    galley,
                                    color,
                                );
                                let mut deleting = false;
                                if hot {
                                    let del = Rect::from_center_size(
                                        pos2(rect.right() - 18.0, rect.center().y),
                                        vec2(24.0, 24.0),
                                    );
                                    let dr = ui.interact(
                                        del,
                                        ui.make_persistent_id(("del", i)),
                                        Sense::click(),
                                    );
                                    if dr.hovered() {
                                        ui.painter().rect_filled(
                                            del,
                                            4.0,
                                            RED.gamma_multiply(0.14),
                                        );
                                    }
                                    img(
                                        theme::TRASH,
                                        14.0,
                                        if dr.hovered() { RED } else { p.faint },
                                    )
                                    .paint_at(
                                        ui,
                                        Rect::from_center_size(del.center(), vec2(14.0, 14.0)),
                                    );
                                    if dr.on_hover_text("파일 삭제 (Delete)").clicked() {
                                        deleting = true;
                                        *next = Some(Dialog::Delete(name.clone()));
                                    }
                                }
                                if resp.secondary_clicked() {
                                    *sel = i;
                                    let pos = pointer_pos.unwrap_or_else(|| rect.left_bottom());
                                    self.context_menu =
                                        Some(ContextMenu::FileItem(name.clone(), pos));
                                } else if resp.clicked() && !deleting && self.context_menu.is_none()
                                {
                                    open = Some(name.clone());
                                }
                            }
                        },
                    );
                }
            });
        footer(ui, p, None, |ui| {
            if primary(ui, p, "새 파일 (+)").clicked() {
                *next = Some(Dialog::NewFile(String::new()));
            }
            if ghost(ui, p, "OS 탐색기에서 열기", None).clicked() {
                *close = true;
                self.open_local_file();
            }
            if ghost(ui, p, "폴더 변경", None).clicked() {
                *close = true;
                self.pick_folder();
            }
            ui.with_layout(egui::Layout::left_to_right(Align::Center), |ui| {
                ui.label(
                    RichText::new("↑↓ 이동 · Enter 열기 · F2 이름변경 · 우클릭 메뉴")
                        .size(11.5)
                        .color(p.faint),
                );
            });
        });
        if let Some(name) = open {
            self.context_menu = None;
            self.open_file(&name);
            *close = true;
        }
    }

    // ---------- 우클릭 컨텍스트 메뉴 (캔버스 · 탭 · 파일 탐색기) ----------
    fn context_menus(&mut self, ctx: &egui::Context, p: &Pal) {
        if self.dialog.is_some() && !matches!(self.context_menu, Some(ContextMenu::FileItem(..))) {
            self.context_menu = None;
            self.canvas_sub = None;
            return;
        }
        if self.dialog.is_none() && matches!(self.context_menu, Some(ContextMenu::FileItem(..))) {
            self.context_menu = None;
            self.canvas_sub = None;
            return;
        }
        let Some(menu) = self.context_menu.clone() else {
            self.canvas_sub = None;
            return;
        };
        let screen = ctx.content_rect();
        let (raw_pos, menu_w, est_h) = match &menu {
            ContextMenu::Canvas(pos) => (*pos, 238.0, 290.0),
            ContextMenu::Tab(_, pos) => (*pos, 224.0, 250.0),
            ContextMenu::FileItem(_, pos) => (*pos, 210.0, 150.0),
        };
        let x = raw_pos.x.clamp(
            screen.left() + 8.0,
            (screen.right() - menu_w - 8.0).max(screen.left() + 8.0),
        );
        let y = raw_pos.y.clamp(
            screen.top() + 8.0,
            (screen.bottom() - est_h - 8.0).max(screen.top() + 8.0),
        );
        let mut close_menu = false;
        let mut sub_anchor_y = None;
        let area_resp = egui::Area::new(egui::Id::unique("context-menu"))
            .order(Order::Tooltip)
            .fixed_pos(pos2(x, y))
            .show(ctx, |ui| {
                Frame::new()
                    .fill(p.elevated)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(10)
                    .shadow(theme::shadow(p))
                    .inner_margin(Margin::symmetric(6, 6))
                    .show(ui, |ui| {
                        ui.set_width(menu_w);
                        ui.spacing_mut().item_spacing.y = 1.0;
                        match &menu {
                            ContextMenu::Canvas(_) => {
                                let (done, anchor) = self.canvas_context_menu(ui, ctx, p);
                                close_menu |= done;
                                sub_anchor_y = anchor;
                            }
                            ContextMenu::Tab(key, _) => {
                                close_menu |= self.tab_context_menu(ui, ctx, p, key);
                            }
                            ContextMenu::FileItem(name, _) => {
                                close_menu |= self.file_context_menu(ui, ctx, p, name);
                            }
                        }
                    });
            });
        let menu_rect = area_resp.response.rect;
        let mut sub_rect: Option<Rect> = None;
        if matches!(menu, ContextMenu::Canvas(_)) {
            if let Some(sub) = self.canvas_sub {
                let (sub_w, sub_est_h) = match sub {
                    CanvasSub::TurnInto => (248.0, 225.0),
                    CanvasSub::Insert => (232.0, 165.0),
                };
                let sub_outer_w = sub_w + 14.0;
                let sub_x = if menu_rect.right() + 2.0 + sub_outer_w <= screen.right() - 8.0 {
                    menu_rect.right() + 2.0
                } else {
                    (menu_rect.left() - sub_outer_w - 2.0).max(screen.left() + 8.0)
                };
                let sub_y = sub_anchor_y.unwrap_or(y).clamp(
                    screen.top() + 8.0,
                    (screen.bottom() - sub_est_h - 8.0).max(screen.top() + 8.0),
                );
                let sub_resp = egui::Area::new(egui::Id::unique(("context-sub", sub as u8)))
                    .order(Order::Tooltip)
                    .fixed_pos(pos2(sub_x, sub_y))
                    .show(ctx, |ui| {
                        Frame::new()
                            .fill(p.elevated)
                            .stroke(Stroke::new(1.0, p.border))
                            .corner_radius(10)
                            .shadow(theme::shadow(p))
                            .inner_margin(Margin::symmetric(6, 6))
                            .show(ui, |ui| {
                                ui.set_width(sub_w);
                                ui.spacing_mut().item_spacing.y = 1.0;
                                close_menu |= self.canvas_submenu(ui, ctx, p, sub);
                            });
                    });
                sub_rect = Some(sub_resp.response.rect);
            }
        }
        let clicked_outside = ctx.input(|i| {
            i.pointer.primary_pressed()
                && i.pointer.interact_pos().is_some_and(|pos| {
                    !menu_rect.contains(pos) && !sub_rect.is_some_and(|r| r.contains(pos))
                })
        });
        if close_menu || clicked_outside {
            self.context_menu = None;
            self.canvas_sub = None;
            if self.active.is_some() && self.dialog.is_none() {
                self.focus_editor = true;
            }
        }
    }

    fn canvas_context_menu(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        p: &Pal,
    ) -> (bool, Option<f32>) {
        let ic = |i: Icon| Some((i, p.muted));
        let mut done = false;
        let mut sub_anchor_y = None;

        // 1. 상단 인라인 글자 서식 빠른 버튼 줄
        let fmt_resp = ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.spacing_mut().item_spacing.x = 2.0;
            let m = p.muted;
            let mut pick = None;
            for (icon, color, tip, f) in [
                (
                    theme::BOLD,
                    m,
                    "굵게 (Ctrl+B)",
                    Fmt::Wrap("**", "**", "굵게"),
                ),
                (
                    theme::ITALIC,
                    m,
                    "기울임 (Ctrl+I)",
                    Fmt::Wrap("*", "*", "기울임"),
                ),
                (
                    theme::STRIKE,
                    m,
                    "취소선 (Ctrl+Shift+S)",
                    Fmt::Wrap("~~", "~~", "취소선"),
                ),
                (
                    theme::HIGHLIGHT,
                    Color32::from_rgb(0xea, 0xb3, 0x08),
                    "형광펜 강조 (Ctrl+Shift+H)",
                    Fmt::Wrap("==", "==", "강조"),
                ),
                (
                    theme::CODE,
                    m,
                    "인라인 코드 (Ctrl+E)",
                    Fmt::Wrap("`", "`", "code"),
                ),
            ] {
                if small_icon_btn(ui, p, icon, color, tip).clicked() {
                    pick = Some(f);
                }
            }
            if ui
                .add(
                    egui::Button::new(RichText::new("K").monospace().size(11.0).color(m))
                        .min_size(vec2(26.0, 26.0))
                        .corner_radius(5),
                )
                .on_hover_text("키보드 키 — [[키]] (Ctrl+Shift+K)")
                .clicked()
            {
                pick = Some(Fmt::Wrap("[[", "]]", "Ctrl"));
            }
            if small_icon_btn(ui, p, theme::LINK, m, "링크 삽입 (Ctrl+K)").clicked() {
                pick = Some(Fmt::Wrap("[", "](https://)", "링크"));
            }
            if small_icon_btn(ui, p, theme::X, m, "서식 지우기 (Ctrl+\\)").clicked() {
                pick = Some(Fmt::Clear);
            }
            if let Some(f) = pick {
                self.format(ctx, f);
                done = true;
            }
        });
        if fmt_resp.response.hovered() {
            self.canvas_sub = None;
        }

        // 2. 핵심 편집 (잘라내기 · 복사 · 붙여넣기 · 전체 선택)
        ui.add_space(3.0);
        hline(ui, p);
        ui.add_space(3.0);
        let r = menu_row(ui, p, ic(theme::SCISSORS), "잘라내기", "Ctrl+X", p.text);
        if r.hovered() {
            self.canvas_sub = None;
        }
        if r.clicked() {
            self.cut_selection(ctx);
            done = true;
        }
        let r = menu_row(ui, p, ic(theme::COPY), "복사", "Ctrl+C", p.text);
        if r.hovered() {
            self.canvas_sub = None;
        }
        if r.clicked() {
            self.copy_selection(ctx);
            done = true;
        }
        let r = menu_row(ui, p, ic(theme::CLIPBOARD), "붙여넣기", "Ctrl+V", p.text);
        if r.hovered() {
            self.canvas_sub = None;
        }
        if r.clicked() {
            self.paste_from_clipboard(ctx);
            done = true;
        }
        let r = menu_row(ui, p, ic(theme::MAXIMIZE), "전체 선택", "Ctrl+A", p.text);
        if r.hovered() {
            self.canvas_sub = None;
        }
        if r.clicked() {
            self.select_all_doc();
            done = true;
        }

        // 3. 하위 메뉴 (블록 전환 ▸ · 삽입 ▸)
        ui.add_space(3.0);
        hline(ui, p);
        ui.add_space(3.0);
        let r = submenu_row(
            ui,
            p,
            ic(theme::TYPE),
            "블록 전환",
            self.canvas_sub == Some(CanvasSub::TurnInto),
        );
        if r.hovered() || r.clicked() {
            self.canvas_sub = Some(CanvasSub::TurnInto);
        }
        if self.canvas_sub == Some(CanvasSub::TurnInto) {
            sub_anchor_y = Some(r.rect.top() - 6.0);
        }
        let r = submenu_row(
            ui,
            p,
            ic(theme::PLUS),
            "삽입",
            self.canvas_sub == Some(CanvasSub::Insert),
        );
        if r.hovered() || r.clicked() {
            self.canvas_sub = Some(CanvasSub::Insert);
        }
        if self.canvas_sub == Some(CanvasSub::Insert) {
            sub_anchor_y = Some(r.rect.top() - 6.0);
        }

        // 4. 블록 핵심 조작 (복제 · 삭제)
        ui.add_space(3.0);
        hline(ui, p);
        ui.add_space(3.0);
        let r = menu_row(ui, p, ic(theme::COPY), "블록 복제", "Ctrl+D", p.text);
        if r.hovered() {
            self.canvas_sub = None;
        }
        if r.clicked() {
            self.duplicate_blocks();
            done = true;
        }
        let r = menu_row(
            ui,
            p,
            Some((theme::TRASH, RED)),
            "블록 삭제",
            "Ctrl+Shift+D",
            RED,
        );
        if r.hovered() {
            self.canvas_sub = None;
        }
        if r.clicked() {
            self.delete_current_blocks();
            done = true;
        }

        (done, sub_anchor_y)
    }

    fn canvas_submenu(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        p: &Pal,
        sub: CanvasSub,
    ) -> bool {
        let ic = |i: Icon| Some((i, p.muted));
        let mut done = false;
        match sub {
            CanvasSub::TurnInto => {
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for (badge, rgb, tip, kind) in [
                        (
                            "¶",
                            [100, 116, 139],
                            "일반 텍스트 (Ctrl+Alt+0)",
                            BlockKind::Paragraph,
                        ),
                        (
                            "H1",
                            [35, 131, 226],
                            "제목 1 (Ctrl+Alt+1)",
                            BlockKind::Heading(1),
                        ),
                        (
                            "H2",
                            [35, 131, 226],
                            "제목 2 (Ctrl+Alt+2)",
                            BlockKind::Heading(2),
                        ),
                        (
                            "H3",
                            [35, 131, 226],
                            "제목 3 (Ctrl+Alt+3)",
                            BlockKind::Heading(3),
                        ),
                        (
                            "▣",
                            [16, 185, 129],
                            "할 일 목록 (Ctrl+Alt+4)",
                            BlockKind::Todo,
                        ),
                        (
                            "•",
                            [245, 158, 11],
                            "글머리 목록 (Ctrl+Alt+5)",
                            BlockKind::Bullet,
                        ),
                        (
                            "1.",
                            [245, 158, 11],
                            "번호 목록 (Ctrl+Alt+6)",
                            BlockKind::Numbered,
                        ),
                        ("”", [139, 92, 246], "인용구 (Ctrl+Alt+7)", BlockKind::Quote),
                    ] {
                        if block_chip(ui, p, badge, rgb, tip).clicked() {
                            self.convert_block(ctx, kind);
                            done = true;
                        }
                    }
                });
                ui.add_space(3.0);
                hline(ui, p);
                ui.add_space(3.0);
                if menu_row(
                    ui,
                    p,
                    Some((theme::CHECK_SQUARE, GREEN)),
                    "할 일 완료 토글",
                    "Ctrl+Enter",
                    p.text,
                )
                .clicked()
                {
                    self.toggle_todo(ctx);
                    done = true;
                }
                if menu_row(
                    ui,
                    p,
                    ic(theme::CODE),
                    "코드 블록으로 전환",
                    "Ctrl+Alt+8",
                    p.text,
                )
                .clicked()
                {
                    self.convert_block(ctx, BlockKind::Code);
                    done = true;
                }
                if menu_row(
                    ui,
                    p,
                    Some((theme::ALERT, AMBER)),
                    "콜아웃 박스로 전환",
                    "Ctrl+Alt+9",
                    p.text,
                )
                .clicked()
                {
                    self.convert_block(ctx, BlockKind::Callout);
                    done = true;
                }
                if menu_row(
                    ui,
                    p,
                    ic(theme::LIST),
                    "접기(토글) 블록으로 전환",
                    "Ctrl+Alt+G",
                    p.text,
                )
                .clicked()
                {
                    self.convert_block(ctx, BlockKind::Toggle);
                    done = true;
                }
                ui.add_space(3.0);
                hline(ui, p);
                ui.add_space(3.0);
                if menu_row(
                    ui,
                    p,
                    ic(theme::ARROW_UP),
                    "블록 위로 이동",
                    "Alt+↑",
                    p.text,
                )
                .clicked()
                {
                    self.move_blocks(ctx, true);
                    done = true;
                }
                if menu_row(
                    ui,
                    p,
                    ic(theme::ARROW_DOWN),
                    "블록 아래로 이동",
                    "Alt+↓",
                    p.text,
                )
                .clicked()
                {
                    self.move_blocks(ctx, false);
                    done = true;
                }
            }
            CanvasSub::Insert => {
                if menu_row(
                    ui,
                    p,
                    ic(theme::PLUS),
                    "아래에 빈 블록 추가",
                    "Ctrl+Shift+Enter",
                    p.text,
                )
                .clicked()
                {
                    self.insert_block_below();
                    done = true;
                }
                if menu_row(
                    ui,
                    p,
                    ic(theme::TABLE),
                    "표 (Table) 삽입",
                    "Ctrl+Alt+T",
                    p.text,
                )
                .clicked()
                {
                    self.insert_block_template("| 제목 | 내용 |\n| --- | --- |\n|  |  |", 2);
                    done = true;
                }
                if menu_row(
                    ui,
                    p,
                    ic(theme::CODE),
                    "Mermaid 다이어그램 삽입",
                    "Ctrl+Alt+M",
                    p.text,
                )
                .clicked()
                {
                    self.insert_block_template("```mermaid\ngraph TD\n  A --> B\n```", 11);
                    done = true;
                }
                if menu_row(
                    ui,
                    p,
                    ic(theme::MINUS),
                    "구분선 (---) 삽입",
                    "Ctrl+Alt+-",
                    p.text,
                )
                .clicked()
                {
                    self.insert_block_template("---", 3);
                    done = true;
                }
                if menu_row(
                    ui,
                    p,
                    ic(theme::IMAGE),
                    "이미지 삽입...",
                    "Ctrl+Shift+I",
                    p.text,
                )
                .clicked()
                {
                    self.dialog = Some(Dialog::Image {
                        url_tab: false,
                        url: String::new(),
                        alt: String::new(),
                    });
                    done = true;
                }
            }
        }
        done
    }

    fn tab_context_menu(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        p: &Pal,
        key: &str,
    ) -> bool {
        let ic = |i: Icon| Some((i, p.muted));
        let is_scratch = is_scratch_key(key);
        let mut done = false;

        if menu_row(ui, p, ic(theme::PLUS), "새 탭 열기", "Ctrl+N", p.text).clicked() {
            self.new_tab();
            done = true;
        }
        if menu_row(
            ui,
            p,
            ic(theme::COPY),
            "탭 내용 복제하여 새 탭으로",
            "",
            p.text,
        )
        .clicked()
        {
            self.duplicate_tab(key);
            done = true;
        }
        if is_scratch {
            if menu_row(ui, p, ic(theme::SAVE), "파일로 저장...", "Ctrl+S", p.text).clicked() {
                self.switch_to_tab(key);
                self.dialog = Some(Dialog::SaveScratch(String::new()));
                done = true;
            }
        } else {
            if menu_row(ui, p, ic(theme::PENCIL), "파일 이름 변경...", "F2", p.text).clicked()
            {
                self.ask_rename(key);
                done = true;
            }
            if menu_row(ui, p, ic(theme::FILE), "파일 전체 경로 복사", "", p.text).clicked()
            {
                let full = self.root.join(key).display().to_string();
                ctx.copy_text(full);
                self.status = "파일 경로를 복사했습니다".into();
                done = true;
            }
        }
        ui.add_space(3.0);
        hline(ui, p);
        ui.add_space(3.0);
        if menu_row(ui, p, ic(theme::X), "탭 닫기", "Ctrl+W", p.text).clicked() {
            self.close_tab(key);
            done = true;
        }
        if self.tabs.len() > 1
            && menu_row(ui, p, ic(theme::X), "다른 탭 모두 닫기", "", p.text).clicked()
        {
            self.close_other_tabs(key);
            done = true;
        }
        let has_right = self
            .tabs
            .iter()
            .position(|t| t == key)
            .is_some_and(|i| i + 1 < self.tabs.len());
        if has_right && menu_row(ui, p, ic(theme::X), "오른쪽 탭 모두 닫기", "", p.text).clicked()
        {
            self.close_tabs_to_right(key);
            done = true;
        }
        if !is_scratch {
            ui.add_space(3.0);
            hline(ui, p);
            ui.add_space(3.0);
            if menu_row(
                ui,
                p,
                Some((theme::TRASH, RED)),
                "파일 영구 삭제...",
                "",
                RED,
            )
            .clicked()
            {
                self.dialog = Some(Dialog::Delete(key.to_string()));
                done = true;
            }
        }
        done
    }

    fn file_context_menu(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        p: &Pal,
        name: &str,
    ) -> bool {
        let ic = |i: Icon| Some((i, p.muted));
        let mut done = false;
        if menu_row(ui, p, ic(theme::FILE), "파일 열기", "Enter", p.text).clicked() {
            self.open_file(name);
            self.dialog = None;
            done = true;
        }
        if menu_row(ui, p, ic(theme::PENCIL), "파일 이름 변경...", "F2", p.text).clicked() {
            self.dialog = Some(Dialog::Rename {
                from: name.to_string(),
                to: name.to_string(),
            });
            done = true;
        }
        if menu_row(ui, p, ic(theme::COPY), "파일 전체 경로 복사", "", p.text).clicked() {
            ctx.copy_text(self.root.join(name).display().to_string());
            self.status = "파일 경로를 복사했습니다".into();
            done = true;
        }
        ui.add_space(3.0);
        hline(ui, p);
        ui.add_space(3.0);
        if menu_row(
            ui,
            p,
            Some((theme::TRASH, RED)),
            "파일 삭제...",
            "Delete",
            RED,
        )
        .clicked()
        {
            self.dialog = Some(Dialog::Delete(name.to_string()));
            done = true;
        }
        done
    }

    #[allow(clippy::too_many_arguments)]
    fn image_modal(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        p: &Pal,
        url_tab: &mut bool,
        url: &mut String,
        alt: &mut String,
        close: &mut bool,
        cancel: &mut bool,
    ) {
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            for (is_url, label) in [(false, "파일 업로드"), (true, "웹 링크 (URL)")] {
                let on = *url_tab == is_url;
                let r = ui.add(
                    egui::Button::new(RichText::new(label).size(13.0).color(if on {
                        p.accent
                    } else {
                        p.muted
                    }))
                    .fill(Color32::TRANSPARENT)
                    .stroke(Stroke::NONE)
                    .min_size(vec2(0.0, 36.0)),
                );
                if on {
                    ui.painter().hline(
                        r.rect.x_range(),
                        r.rect.bottom(),
                        Stroke::new(2.0, p.accent),
                    );
                }
                if r.clicked() {
                    *url_tab = is_url;
                }
            }
        });
        hline(ui, p);
        Frame::new().inner_margin(Margin::same(20)).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            if !*url_tab {
                let zone = Frame::new()
                    .stroke(Stroke::new(1.5, p.border_hover))
                    .corner_radius(10)
                    .inner_margin(Margin::symmetric(20, 28))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.vertical_centered(|ui| {
                            ui.spacing_mut().item_spacing.y = 8.0;
                            ui.add(img(theme::UPLOAD, 36.0, p.muted));
                            ui.label(
                                RichText::new("이미지 파일을 이곳에 끌어다 놓거나 클릭하세요")
                                    .size(13.5)
                                    .strong()
                                    .color(p.text),
                            );
                            ui.label(
                                RichText::new("PNG, JPG, GIF, WebP, BMP 지원")
                                    .size(12.0)
                                    .color(p.faint),
                            );
                            ui.spacing_mut().button_padding = vec2(14.0, 6.0);
                            primary(ui, p, "컴퓨터에서 파일 선택").clicked()
                        })
                        .inner
                    });
                let pick = zone.inner || zone.response.interact(Sense::click()).clicked();
                if pick {
                    if let Some(f) = rfd::FileDialog::new()
                        .add_filter("이미지", &["png", "jpg", "jpeg", "gif", "webp", "bmp"])
                        .pick_file()
                    {
                        self.insert_image_file(ctx, &f);
                        *close = true;
                    }
                }
            } else {
                ui.label(
                    RichText::new("이미지 URL")
                        .size(12.0)
                        .strong()
                        .color(p.muted),
                );
                input(
                    ui,
                    p,
                    egui::Id::unique("img-url"),
                    url,
                    "https://example.com/image.png",
                    f32::INFINITY,
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new("대체 텍스트 (설명, 선택 사항)")
                        .size(12.0)
                        .strong()
                        .color(p.muted),
                );
                input(
                    ui,
                    p,
                    egui::Id::unique("img-alt"),
                    alt,
                    "이미지 설명",
                    f32::INFINITY,
                );
                ui.add_space(10.0);
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().button_padding = vec2(14.0, 6.0);
                    if primary(ui, p, "이미지 삽입").clicked() && !url.trim().is_empty() {
                        let a = if alt.trim().is_empty() {
                            "image"
                        } else {
                            alt.trim()
                        };
                        let md = format!("![{a}]({})", url.trim());
                        self.insert(ctx, &md);
                        *close = true;
                    }
                    *cancel |= ghost(ui, p, "취소", None).clicked();
                });
            }
        });
    }
}

struct MdRenderCtx<'a> {
    p: &'a Pal,
    raw: &'a str, // 전처리 전 원문 (길이 동일) — ==강조==와 **굵게** 구분용
    body: f32,
    scheme: &'a str,
    dark: bool,
    cache: &'a mut InlineGalleyCache,
}

struct MdInlineBuf<'a> {
    p: &'a Pal,
    size: f32,
    base_color: Color32,
    base_bold: bool,
    job: egui::text::LayoutJob,
    links: Vec<(Range<usize>, String)>,
    char_count: usize,
    strong: u16,
    mark: u16, // ==강조== (전처리로 **가 된 구간)
    emphasis: u16,
    strike: u16,
    link_url: Option<String>,
    pending: String,
}

impl<'a> MdInlineBuf<'a> {
    fn new(p: &'a Pal, size: f32, base_color: Color32, base_bold: bool) -> Self {
        Self {
            p,
            size,
            base_color,
            base_bold,
            job: egui::text::LayoutJob::default(),
            links: Vec::new(),
            char_count: 0,
            strong: 0,
            mark: 0,
            emphasis: 0,
            strike: 0,
            link_url: None,
            pending: String::new(),
        }
    }

    fn push_styled(&mut self, txt: &str, force_bold: bool) {
        if txt.is_empty() {
            return;
        }
        let is_bold = self.base_bold || self.strong > 0 || force_bold;
        let family = if is_bold {
            theme::bold()
        } else {
            FontFamily::Proportional
        };
        let color = if self.link_url.is_some() {
            self.p.accent
        } else if is_bold && self.base_color == self.p.muted {
            self.p.text
        } else {
            self.base_color
        };
        let fmt = egui::text::TextFormat {
            font_id: FontId::new(self.size, family),
            color,
            line_height: Some(self.size * 1.6),
            italics: self.emphasis > 0,
            underline: if self.link_url.is_some() {
                Stroke::new(1.0, self.p.accent)
            } else {
                Stroke::NONE
            },
            strikethrough: if self.strike > 0 {
                Stroke::new(1.0, color)
            } else {
                Stroke::NONE
            },
            background: if self.mark > 0 {
                mark_bg()
            } else {
                Color32::TRANSPARENT
            },
            ..Default::default()
        };
        let c0 = self.char_count;
        self.char_count += txt.chars().count();
        self.job.append(txt, 0.0, fmt);
        if let Some(url) = &self.link_url {
            if let Some((prev_r, prev_u)) = self.links.last_mut() {
                if prev_r.end == c0 && prev_u == url {
                    prev_r.end = self.char_count;
                    return;
                }
            }
            self.links.push((c0..self.char_count, url.clone()));
        }
    }

    fn push_styled_with_footnotes(&mut self, txt: &str, force_bold: bool) {
        if txt.is_empty() {
            return;
        }
        if !txt.contains("[^") {
            self.push_styled(txt, force_bold);
            return;
        }
        let mut cur = 0usize;
        while let Some(rel) = txt[cur..].find("[^") {
            let open = cur + rel;
            let rest = &txt[open + 2..];
            if let Some(close) = rest.find(']') {
                let name = &rest[..close];
                if !name.is_empty() && !name.contains(char::is_whitespace) && !name.contains('[') {
                    self.push_styled(&txt[cur..open], force_bold);
                    self.append_footnote_badge(name);
                    cur = open + 2 + close + 1;
                    continue;
                }
            }
            self.push_styled(&txt[cur..open + 2], force_bold);
            cur = open + 2;
        }
        if cur < txt.len() {
            self.push_styled(&txt[cur..], force_bold);
        }
    }

    // 한국어 조사("굵게!"입니다, "따옴표"는 등)가 붙어 CommonMark 후행 규칙을 통과하지 못한 잔여 **...** 쌍도 굵게 처리
    fn flush_pending(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let text = std::mem::take(&mut self.pending);
        if self.strong == 0 && text.contains("**") {
            let mut cur = 0usize;
            while cur < text.len() {
                let Some(rel_open) = text[cur..].find("**") else {
                    break;
                };
                let open = cur + rel_open;
                let inner = &text[open + 2..];
                if !inner.starts_with(char::is_whitespace) {
                    if let Some(rel_close) = inner.find("**") {
                        if rel_close > 0 && !inner[..rel_close].ends_with(char::is_whitespace) {
                            self.push_styled_with_footnotes(&text[cur..open], false);
                            self.push_styled_with_footnotes(&inner[..rel_close], true);
                            cur = open + 2 + rel_close + 2;
                            continue;
                        }
                    }
                }
                self.push_styled_with_footnotes(&text[cur..open + 2], false);
                cur = open + 2;
            }
            if cur < text.len() {
                self.push_styled_with_footnotes(&text[cur..], false);
            }
        } else {
            self.push_styled_with_footnotes(&text, false);
        }
    }

    fn append_footnote_badge(&mut self, name: &str) {
        let badge = format!("[{name}]");
        let fmt = egui::text::TextFormat {
            font_id: FontId::new(self.size * 0.78, FontFamily::Monospace),
            color: self.p.accent,
            valign: Align::TOP,
            line_height: Some(self.size * 1.6),
            ..Default::default()
        };
        self.char_count += badge.chars().count();
        self.job.append(&badge, 0.0, fmt);
    }

    fn push_text(&mut self, txt: &str, escaped: bool) {
        if escaped {
            self.flush_pending();
            self.push_styled(txt, false);
        } else {
            self.pending.push_str(txt);
        }
    }

    fn push_code(&mut self, txt: &str) {
        self.flush_pending();
        if txt.is_empty() {
            return;
        }
        let code_sz = self.size * 0.88;
        let color = if self.link_url.is_some() {
            self.p.accent
        } else {
            self.p.text
        };
        let fmt = egui::text::TextFormat {
            font_id: FontId::new(code_sz, FontFamily::Monospace),
            color,
            background: self.p.code_bg,
            line_height: Some(self.size * 1.6),
            ..Default::default()
        };
        let c0 = self.char_count;
        self.char_count += txt.chars().count();
        self.job.append(txt, 0.0, fmt);
        if let Some(url) = &self.link_url {
            self.links.push((c0..self.char_count, url.clone()));
        }
    }

    fn push_footnote_ref(&mut self, name: &str) {
        self.flush_pending();
        self.append_footnote_badge(name);
    }

    fn handle_inline_event(
        &mut self,
        ev: &pulldown_cmark::Event<'_>,
        src: &str,
        span: Range<usize>,
    ) -> bool {
        use pulldown_cmark::{Event, Tag, TagEnd};
        match ev {
            Event::Text(t) => {
                let raw = src.get(span.clone()).unwrap_or("");
                let escaped = raw.starts_with('\\')
                    || (span.start > 0 && src.as_bytes()[span.start - 1] == b'\\');
                self.push_text(t, escaped);
                true
            }
            Event::Code(t) | Event::InlineMath(t) => {
                self.push_code(t);
                true
            }
            Event::DisplayMath(t) => {
                self.push_code(t.trim());
                true
            }
            Event::FootnoteReference(name) => {
                self.push_footnote_ref(name);
                true
            }
            Event::SoftBreak => {
                self.push_text(" ", false);
                true
            }
            Event::HardBreak => {
                self.flush_pending();
                self.push_styled("\n", false);
                true
            }
            Event::Html(t) | Event::InlineHtml(t) => {
                self.push_text(t, true);
                true
            }
            Event::Start(Tag::Strong) => {
                self.flush_pending();
                self.strong += 1;
                self.mark += u16::from(src.get(span.start..span.start + 2) == Some("=="));
                true
            }
            Event::End(TagEnd::Strong) => {
                self.flush_pending();
                self.strong = self.strong.saturating_sub(1);
                if src.get(span.start..span.start + 2) == Some("==") {
                    self.mark = self.mark.saturating_sub(1);
                }
                true
            }
            Event::Start(Tag::Emphasis) => {
                self.flush_pending();
                self.emphasis += 1;
                true
            }
            Event::End(TagEnd::Emphasis) => {
                self.flush_pending();
                self.emphasis = self.emphasis.saturating_sub(1);
                true
            }
            Event::Start(Tag::Strikethrough) => {
                self.flush_pending();
                self.strike += 1;
                true
            }
            Event::End(TagEnd::Strikethrough) => {
                self.flush_pending();
                self.strike = self.strike.saturating_sub(1);
                true
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                self.flush_pending();
                self.link_url = Some(dest_url.to_string());
                true
            }
            Event::End(TagEnd::Link) => {
                self.flush_pending();
                self.link_url = None;
                true
            }
            _ => false,
        }
    }

    fn finish_cached(
        mut self,
        ui: &mut egui::Ui,
        cache: &mut InlineGalleyCache,
        width: f32,
        dark: bool,
        scheme: &str,
    ) {
        self.flush_pending();
        if self.job.text.is_empty() {
            return;
        }
        let max_w = width.max(40.0);
        self.job.wrap.max_width = max_w;
        let ppp_bits = ui.ctx().pixels_per_point().to_bits();
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            self.job.text.hash(&mut h);
            self.size.to_bits().hash(&mut h);
            max_w.to_bits().hash(&mut h);
            self.base_color.to_array().hash(&mut h);
            self.base_bold.hash(&mut h);
            dark.hash(&mut h);
            ppp_bits.hash(&mut h);
            self.job.sections.len().hash(&mut h);
            for sec in &self.job.sections {
                sec.byte_range.start.0.hash(&mut h);
                sec.byte_range.end.0.hash(&mut h);
                sec.format.color.to_array().hash(&mut h);
                sec.format.italics.hash(&mut h);
                sec.format.background.to_array().hash(&mut h);
                sec.format.font_id.size.to_bits().hash(&mut h);
                (sec.format.font_id.family == theme::bold()).hash(&mut h);
                (sec.format.font_id.family == FontFamily::Monospace).hash(&mut h);
            }
            self.links.len().hash(&mut h);
            h.finish()
        };
        let (galley, links) = if let Some((g, l)) = cache
            .get(&key)
            .filter(|(g, l)| g.text() == self.job.text && l.len() == self.links.len())
        {
            (std::sync::Arc::clone(g), l.as_slice())
        } else {
            let g = ui.ctx().fonts_mut(|f| f.layout_job(self.job));
            if cache.len() >= 256 {
                cache.clear();
            }
            cache.insert(key, (std::sync::Arc::clone(&g), self.links));
            let (g_ref, l_ref) = &cache[&key];
            (std::sync::Arc::clone(g_ref), l_ref.as_slice())
        };
        paint_galley_with_links(ui, galley, links, self.p.text, scheme);
    }
}

fn paint_galley_with_links(
    ui: &mut egui::Ui,
    galley: std::sync::Arc<egui::Galley>,
    links: &[(Range<usize>, String)],
    text_color: Color32,
    scheme: &str,
) {
    let (rect, resp) = ui.allocate_exact_size(galley.size(), Sense::hover());
    ui.painter()
        .galley(rect.min, std::sync::Arc::clone(&galley), text_color);
    let galley_info = std::sync::Arc::clone(&galley);
    let enabled = ui.is_enabled();
    resp.widget_info(move || {
        egui::WidgetInfo::labeled(egui::Role::Label, enabled, galley_info.text())
    });
    if links.is_empty() {
        return;
    }
    let (ptr, clicked) = ui.input(|i| (i.pointer.interact_pos(), i.pointer.primary_clicked()));
    if let Some(pos_ptr) = ptr.filter(|pos_ptr| resp.rect.contains(*pos_ptr)) {
        let c_idx: usize = galley.cursor_from_pos(pos_ptr - rect.min).index.into();
        if let Some((_, url)) = links.iter().find(|(r, _)| r.contains(&c_idx)) {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            if clicked {
                let path_part = url.split('#').next().unwrap_or("");
                let full = if url.starts_with('#')
                    || url.contains("://")
                    || url.starts_with("mailto:")
                    || is_markdown_name(path_part)
                {
                    url.clone()
                } else {
                    format!("{scheme}{url}")
                };
                ui.ctx().open_url(egui::OpenUrl::new_tab(full));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_inline_md_cached(
    ui: &mut egui::Ui,
    cache: &mut InlineGalleyCache,
    src: &str,
    size: f32,
    width: f32,
    p: &Pal,
    dark: bool,
    base_bold: bool,
    scheme: &str,
) {
    use pulldown_cmark::{Options, Parser};
    let max_w = width.max(40.0);
    let ppp_bits = ui.ctx().pixels_per_point().to_bits();
    let key = cache_hash((
        "inline",
        src,
        size.to_bits(),
        max_w.to_bits(),
        dark,
        base_bold,
        ppp_bits,
    ));
    if let Some((galley, links)) = cache.get(&key) {
        paint_galley_with_links(ui, std::sync::Arc::clone(galley), links, p.text, scheme);
        return;
    }
    let clean = preprocess(src);
    let opts = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_FOOTNOTES | Options::ENABLE_MATH;
    let mut buf = MdInlineBuf::new(p, size, p.text, base_bold);
    for (ev, span) in Parser::new_ext(&clean, opts).into_offset_iter() {
        buf.handle_inline_event(&ev, src, span);
    }
    buf.flush_pending();
    if buf.job.text.is_empty() {
        return;
    }
    buf.job.wrap.max_width = max_w;
    let galley = ui.ctx().fonts_mut(|f| f.layout_job(buf.job));
    if cache.len() >= 256 {
        cache.clear();
    }
    cache.insert(key, (std::sync::Arc::clone(&galley), buf.links));
    let (g_ref, l_ref) = &cache[&key];
    paint_galley_with_links(ui, std::sync::Arc::clone(g_ref), l_ref, p.text, scheme);
}

fn parse_md_table_cached(cache: &mut TableParseCache, t: &str) -> Option<std::sync::Arc<MdTable>> {
    if !t.contains('|') || !t.contains('\n') {
        return None;
    }
    let key = cache_hash(t);
    if let Some((raw, parsed)) = cache.get(&key) {
        if raw == t {
            return Some(std::sync::Arc::clone(parsed));
        }
    }
    let parsed = parse_md_table(t)?;
    let arc = std::sync::Arc::new(parsed);
    if cache.len() >= 64 {
        cache.clear();
    }
    cache.insert(key, (t.to_string(), std::sync::Arc::clone(&arc)));
    Some(arc)
}

#[cfg(test)]
pub(crate) fn inline_md_view_job(
    src: &str,
    size: f32,
    p: &Pal,
    base_bold: bool,
) -> egui::text::LayoutJob {
    use pulldown_cmark::{Options, Parser};
    let clean = preprocess(src);
    let opts = Options::ENABLE_STRIKETHROUGH;
    let mut buf = MdInlineBuf::new(p, size, p.text, base_bold);
    for (ev, span) in Parser::new_ext(&clean, opts).into_offset_iter() {
        buf.handle_inline_event(&ev, src, span);
    }
    buf.flush_pending();
    buf.job
}

fn md_alert_meta(kind: pulldown_cmark::BlockQuoteKind, dark: bool) -> (Color32, &'static str) {
    use pulldown_cmark::BlockQuoteKind;
    let c = |d: (u8, u8, u8), l: (u8, u8, u8)| {
        let (r, g, b) = if dark { d } else { l };
        Color32::from_rgb(r, g, b)
    };
    match kind {
        BlockQuoteKind::Note => (c((0x38, 0xbd, 0xf8), (0x02, 0x84, 0xc7)), "● Note"),
        BlockQuoteKind::Tip => (c((0x10, 0xb9, 0x81), (0x05, 0x96, 0x69)), "★ Tip"),
        BlockQuoteKind::Important => (c((0x8b, 0x5c, 0xf6), (0x7c, 0x3a, 0xed)), "◆ Important"),
        BlockQuoteKind::Warning => (c((0xf5, 0x9e, 0x0b), (0xd9, 0x77, 0x06)), "▲ Warning"),
        BlockQuoteKind::Caution => (c((0xef, 0x44, 0x44), (0xdc, 0x26, 0x26)), "■ Caution"),
    }
}

fn render_markdown_native(
    ui: &mut egui::Ui,
    ctx: &mut MdRenderCtx<'_>,
    src: &str,
    width: f32,
) -> Option<(Range<usize>, bool)> {
    use pulldown_cmark::{Options, Parser};
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_MATH
        | Options::ENABLE_GFM;
    let mut iter = Parser::new_ext(src, opts).into_offset_iter().peekable();
    let mut toggled = None;
    render_md_stream(
        ui,
        ctx,
        src,
        width,
        &mut iter,
        None,
        ctx.p.text,
        0,
        &mut toggled,
    );
    toggled
}

#[allow(clippy::too_many_arguments)]
fn render_md_stream<'a, I>(
    ui: &mut egui::Ui,
    ctx: &mut MdRenderCtx<'_>,
    src: &'a str,
    width: f32,
    iter: &mut std::iter::Peekable<I>,
    stop_tag: Option<pulldown_cmark::TagEnd>,
    base_color: Color32,
    list_depth: usize,
    toggled: &mut Option<(Range<usize>, bool)>,
) where
    I: Iterator<Item = (pulldown_cmark::Event<'a>, Range<usize>)>,
{
    use pulldown_cmark::{Event, Tag, TagEnd};
    let p = ctx.p;
    while let Some((ev, span)) = iter.next() {
        match ev {
            Event::End(end_tag) if Some(end_tag) == stop_tag => break,
            Event::Start(Tag::Paragraph) => {
                let mut buf = MdInlineBuf::new(p, ctx.body, base_color, false);
                for (iev, ispan) in iter.by_ref() {
                    if matches!(iev, Event::End(TagEnd::Paragraph)) {
                        break;
                    }
                    buf.handle_inline_event(&iev, ctx.raw, ispan);
                }
                buf.finish_cached(ui, ctx.cache, width, ctx.dark, ctx.scheme);
            }
            Event::Start(Tag::Heading { level, .. }) => {
                let idx = (level as usize)
                    .saturating_sub(1)
                    .min(HEADING_SCALE.len() - 1);
                let sz = ctx.body * HEADING_SCALE[idx];
                ui.add_space(HEADING_TOP[idx] * 0.5);
                let mut buf = MdInlineBuf::new(p, sz, p.text, true);
                for (iev, ispan) in iter.by_ref() {
                    if matches!(iev, Event::End(TagEnd::Heading(_))) {
                        break;
                    }
                    buf.handle_inline_event(&iev, ctx.raw, ispan);
                }
                buf.finish_cached(ui, ctx.cache, width, ctx.dark, ctx.scheme);
            }
            Event::Start(Tag::FootnoteDefinition(label)) => {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label(
                        RichText::new(format!("[{label}]:"))
                            .family(FontFamily::Monospace)
                            .size(ctx.body * 0.85)
                            .color(p.accent),
                    );
                    ui.vertical(|ui| {
                        let fw = (width - 44.0).max(80.0);
                        ui.set_width(fw);
                        render_md_stream(
                            ui,
                            ctx,
                            src,
                            fw,
                            iter,
                            Some(TagEnd::FootnoteDefinition),
                            p.muted,
                            list_depth,
                            toggled,
                        );
                    });
                });
            }
            Event::Start(Tag::BlockQuote(kind)) => {
                if let Some(k) = kind {
                    let (accent, title) = md_alert_meta(k, ctx.dark);
                    let resp = Frame::new()
                        .fill(accent.gamma_multiply(0.08))
                        .corner_radius(6)
                        .inner_margin(Margin {
                            left: 16,
                            right: 14,
                            top: 10,
                            bottom: 10,
                        })
                        .show(ui, |ui| {
                            ui.set_width((width - 30.0).max(80.0));
                            ui.label(
                                RichText::new(title)
                                    .font(FontId::new(ctx.body * 0.94, theme::bold()))
                                    .color(accent),
                            );
                            render_md_stream(
                                ui,
                                ctx,
                                src,
                                (width - 30.0).max(80.0),
                                iter,
                                Some(TagEnd::BlockQuote(kind)),
                                p.text,
                                list_depth,
                                toggled,
                            );
                        })
                        .response;
                    let r = resp.rect;
                    ui.painter().rect_filled(
                        Rect::from_min_max(r.left_top(), pos2(r.left() + 3.0, r.bottom())),
                        2.0,
                        accent,
                    );
                } else {
                    let resp = ui
                        .horizontal(|ui| {
                            ui.add_space(14.0);
                            ui.vertical(|ui| {
                                let qw = (width - 14.0).max(80.0);
                                ui.set_width(qw);
                                render_md_stream(
                                    ui,
                                    ctx,
                                    src,
                                    qw,
                                    iter,
                                    Some(TagEnd::BlockQuote(kind)),
                                    p.muted,
                                    list_depth,
                                    toggled,
                                );
                            });
                        })
                        .response;
                    ui.painter().vline(
                        resp.rect.left() + 4.0,
                        resp.rect.y_range(),
                        Stroke::new(3.0, p.border_hover),
                    );
                }
            }
            Event::Start(Tag::List(first)) => {
                let mut num = first;
                while let Some((lev, _)) = iter.next() {
                    match lev {
                        Event::End(TagEnd::List(_)) => break,
                        Event::Start(Tag::Item) => {
                            let mut task: Option<(Range<usize>, bool)> = None;
                            let mut leading_para = false;
                            if let Some((Event::TaskListMarker(chk), mspan)) = iter.peek() {
                                task = Some((mspan.clone(), *chk));
                                iter.next();
                            } else if matches!(iter.peek(), Some((Event::Start(Tag::Paragraph), _)))
                            {
                                iter.next();
                                leading_para = true;
                                if let Some((Event::TaskListMarker(chk), mspan)) = iter.peek() {
                                    task = Some((mspan.clone(), *chk));
                                    iter.next();
                                }
                            }
                            // 편집 화면(md_job·paint_list_markers)과 같은 배치 상수를 쓴다
                            let indent = LIST_PAD;
                            let marker_w = LIST_MARKER_W;
                            let item_w = (width - LIST_STEP).max(80.0);
                            ui.horizontal_top(|ui| {
                                ui.spacing_mut().item_spacing.x = LIST_GAP;
                                ui.add_space(indent);
                                if let Some((mspan, chk)) = task {
                                    // 체크박스도 기호 칸 안에 두어 글머리 항목과 내용 열을 맞춘다
                                    let mut val = chk;
                                    let (rect, resp) = ui.allocate_exact_size(
                                        vec2(marker_w, ctx.body * 1.5),
                                        Sense::click(),
                                    );
                                    paint_checkbox(ui.painter(), rect.center(), val);
                                    resp.widget_info(|| {
                                        egui::WidgetInfo::selected(
                                            egui::accesskit::Role::CheckBox,
                                            true,
                                            val,
                                            "",
                                        )
                                    });
                                    if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                                        val = !val;
                                        *toggled = Some((mspan, val));
                                    }
                                } else {
                                    let label = if let Some(n) = &mut num {
                                        let s = format!("{n}.");
                                        *n += 1;
                                        s
                                    } else {
                                        match list_depth % 3 {
                                            0 => "•",
                                            1 => "◦",
                                            _ => "▪",
                                        }
                                        .to_string()
                                    };
                                    ui.add_sized(
                                        vec2(marker_w, ctx.body * 1.5),
                                        egui::Label::new(
                                            RichText::new(label).size(ctx.body).color(p.muted),
                                        ),
                                    );
                                }
                                ui.vertical(|ui| {
                                    ui.set_width(item_w);
                                    if leading_para {
                                        let mut buf =
                                            MdInlineBuf::new(p, ctx.body, base_color, false);
                                        for (iev, ispan) in iter.by_ref() {
                                            if matches!(iev, Event::End(TagEnd::Paragraph)) {
                                                break;
                                            }
                                            buf.handle_inline_event(&iev, ctx.raw, ispan);
                                        }
                                        buf.finish_cached(
                                            ui, ctx.cache, item_w, ctx.dark, ctx.scheme,
                                        );
                                    } else {
                                        let mut buf =
                                            MdInlineBuf::new(p, ctx.body, base_color, false);
                                        while let Some((iev, _)) = iter.peek() {
                                            if matches!(
                                                iev,
                                                Event::End(TagEnd::Item)
                                                    | Event::Start(Tag::List(_))
                                                    | Event::Start(Tag::Paragraph)
                                                    | Event::Start(Tag::BlockQuote(_))
                                                    | Event::Start(Tag::CodeBlock(_))
                                            ) {
                                                break;
                                            }
                                            let (iev, ispan) = iter.next().unwrap();
                                            buf.handle_inline_event(&iev, ctx.raw, ispan);
                                        }
                                        buf.finish_cached(
                                            ui, ctx.cache, item_w, ctx.dark, ctx.scheme,
                                        );
                                    }
                                    render_md_stream(
                                        ui,
                                        ctx,
                                        src,
                                        item_w,
                                        iter,
                                        Some(TagEnd::Item),
                                        base_color,
                                        list_depth + 1,
                                        toggled,
                                    );
                                });
                            });
                        }
                        _ => {}
                    }
                }
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                let mut code = String::new();
                for (cev, _) in iter.by_ref() {
                    if matches!(cev, Event::End(TagEnd::CodeBlock)) {
                        break;
                    }
                    if let Event::Text(t) = cev {
                        code.push_str(&t);
                    }
                }
                let lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(l) => l.to_string(),
                    pulldown_cmark::CodeBlockKind::Indented => String::new(),
                };
                Frame::new()
                    .fill(p.code_bg)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(6)
                    .inner_margin(Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.set_width((width - 24.0).max(80.0));
                        let code_sz = ctx.body * 0.85;
                        let job = build_code_layout_job(
                            ui.ctx(),
                            code.trim_end_matches('\n'),
                            &lang,
                            code_sz,
                            (width - 28.0).max(80.0),
                            code_sz * 1.5,
                            p,
                            ctx.dark,
                        );
                        ui.add(egui::Label::new(job));
                    });
            }
            Event::Start(Tag::Table(_aligns)) => {
                let mut headers: Vec<Vec<egui::text::LayoutJob>> = Vec::new();
                let mut rows: Vec<Vec<egui::text::LayoutJob>> = Vec::new();
                let mut cur_row: Vec<egui::text::LayoutJob> = Vec::new();
                let mut in_head = false;
                while let Some((tev, tspan)) = iter.next() {
                    match tev {
                        Event::End(TagEnd::Table) => break,
                        Event::Start(Tag::TableHead) => in_head = true,
                        Event::End(TagEnd::TableHead) => {
                            if !cur_row.is_empty() {
                                headers.push(std::mem::take(&mut cur_row));
                            }
                            in_head = false;
                        }
                        Event::Start(Tag::TableRow) => cur_row.clear(),
                        Event::End(TagEnd::TableRow) => {
                            if !cur_row.is_empty() {
                                rows.push(std::mem::take(&mut cur_row));
                            }
                        }
                        Event::Start(Tag::TableCell) => {
                            let mut buf = MdInlineBuf::new(p, ctx.body * 0.92, p.text, in_head);
                            for (cev, cspan) in iter.by_ref() {
                                if matches!(cev, Event::End(TagEnd::TableCell)) {
                                    break;
                                }
                                buf.handle_inline_event(&cev, ctx.raw, cspan);
                            }
                            buf.flush_pending();
                            cur_row.push(buf.job);
                        }
                        _ => {
                            let _ = tspan;
                        }
                    }
                }
                egui::Grid::new(("nested-tbl", span.start))
                    .striped(true)
                    .spacing(vec2(12.0, 6.0))
                    .show(ui, |ui| {
                        for h_row in headers {
                            for job in h_row {
                                ui.add(egui::Label::new(job).wrap());
                            }
                            ui.end_row();
                        }
                        for r in rows {
                            for job in r {
                                ui.add(egui::Label::new(job).wrap());
                            }
                            ui.end_row();
                        }
                    });
            }
            Event::DisplayMath(t) => {
                let _ = card(ui, p, None, false, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new(t.trim())
                                .family(FontFamily::Monospace)
                                .size(ctx.body)
                                .color(p.text),
                        );
                    });
                });
            }
            Event::Rule => {
                ui.add_space(4.0);
                hline(ui, p);
                ui.add_space(4.0);
            }
            Event::TaskListMarker(chk) => {
                let mut val = chk;
                if ui.checkbox(&mut val, "").changed() {
                    *toggled = Some((span, val));
                }
            }
            _ => {}
        }
    }
}

fn canvas_style(ui: &mut egui::Ui, body: f32, p: &Pal) {
    use egui::TextStyle;
    let s = ui.style_mut();
    s.interaction.selectable_labels = false;
    s.text_styles
        .insert(TextStyle::Body, FontId::proportional(body));
    s.text_styles
        .insert(TextStyle::Heading, FontId::new(body * 1.95, theme::bold()));
    s.text_styles
        .insert(TextStyle::Monospace, FontId::monospace(body * 0.84));
    s.text_styles
        .insert(TextStyle::Small, FontId::proportional(body * 0.8));
    s.text_styles
        .insert(TextStyle::Button, FontId::proportional(body * 0.85));
    s.spacing.item_spacing.y = 6.0;
    s.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
}

fn table_delete_btn(ui: &mut egui::Ui) -> egui::Response {
    ui.add(
        egui::Button::image_and_text(
            img(theme::TRASH, 11.0, RED),
            RichText::new("표 삭제").size(11.0).color(RED),
        )
        .fill(RED.gamma_multiply(0.08))
        .stroke(Stroke::new(1.0, RED.gamma_multiply(0.32)))
        .corner_radius(6)
        .min_size(vec2(0.0, 23.0)),
    )
    .on_hover_cursor(CursorIcon::PointingHand)
    .on_hover_text("표 전체 삭제 (Ctrl+Shift+D)")
}

fn table_col_width(cols: usize, avail_w: f32) -> (f32, f32) {
    let c = cols.max(1) as f32;
    let col_w = (avail_w.max(240.0) / c).max(116.0);
    (col_w, col_w * c)
}

fn table_egui_align(a: TableAlign) -> Align {
    match a {
        TableAlign::Center => Align::Center,
        TableAlign::Right => Align::Max,
        TableAlign::None | TableAlign::Left => Align::Min,
    }
}

fn table_align2(a: TableAlign) -> Align2 {
    match a {
        TableAlign::Center => Align2::CENTER_CENTER,
        TableAlign::Right => Align2::RIGHT_CENTER,
        TableAlign::None | TableAlign::Left => Align2::LEFT_CENTER,
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_table_chrome(
    ui: &egui::Ui,
    p: &Pal,
    full_rect: Rect,
    col_w: f32,
    cols: usize,
    row_shapes: &[(egui::layers::ShapeIdx, f32, f32)],
    outer_border: Color32,
    focus_cell: Option<(usize, usize)>,
) {
    let total_rows = row_shapes.len();
    for (r_i, &(shape_idx, y_top, y_bot)) in row_shapes.iter().enumerate() {
        let rr = Rect::from_min_max(
            pos2(full_rect.left(), y_top),
            pos2(full_rect.right(), y_bot),
        );
        let is_first = r_i == 0;
        let is_last = r_i + 1 == total_rows;
        let cr = egui::CornerRadius {
            nw: if is_first { 8 } else { 0 },
            ne: if is_first { 8 } else { 0 },
            sw: if is_last { 8 } else { 0 },
            se: if is_last { 8 } else { 0 },
        };
        let fill = if r_i == 0 {
            p.surface
        } else if r_i % 2 == 0 {
            p.surface.gamma_multiply(0.45)
        } else {
            p.bg
        };
        ui.painter()
            .set(shape_idx, egui::Shape::rect_filled(rr, cr, fill));
    }

    for c_i in 1..cols {
        let x = full_rect.left() + col_w * (c_i as f32);
        ui.painter()
            .vline(x, full_rect.y_range(), Stroke::new(1.0, p.border));
    }
    for (r_i, &(_, _, y_bot)) in row_shapes
        .iter()
        .take(total_rows.saturating_sub(1))
        .enumerate()
    {
        let stroke = if r_i == 0 {
            Stroke::new(1.0, p.border_hover)
        } else {
            Stroke::new(1.0, p.border)
        };
        ui.painter().hline(full_rect.x_range(), y_bot, stroke);
    }

    if let Some((fr, fc)) = focus_cell {
        if let Some(&(_, y_top, y_bot)) = row_shapes.get(fr) {
            if fc < cols {
                let x0 = full_rect.left() + col_w * (fc as f32);
                let cell_rect = Rect::from_min_max(pos2(x0, y_top), pos2(x0 + col_w, y_bot));
                let focus_cr = egui::CornerRadius {
                    nw: if fr == 0 && fc == 0 { 8 } else { 0 },
                    ne: if fr == 0 && fc + 1 == cols { 8 } else { 0 },
                    sw: if fr + 1 == total_rows && fc == 0 {
                        8
                    } else {
                        0
                    },
                    se: if fr + 1 == total_rows && fc + 1 == cols {
                        8
                    } else {
                        0
                    },
                };
                ui.painter()
                    .rect_filled(cell_rect, focus_cr, p.accent.gamma_multiply(0.06));
                ui.painter().rect_stroke(
                    cell_rect,
                    focus_cr,
                    Stroke::new(1.5, p.accent),
                    egui::StrokeKind::Inside,
                );
            }
        }
    }

    ui.painter().rect_stroke(
        full_rect,
        8.0,
        Stroke::new(1.0, outer_border),
        egui::StrokeKind::Inside,
    );
}

fn card(
    ui: &mut egui::Ui,
    p: &Pal,
    label: Option<&str>,
    copyable: bool,
    add: impl FnOnce(&mut egui::Ui),
) -> bool {
    let mut copied = false;
    ui.add_space(6.0);
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if label.is_some() || copyable {
                ui.horizontal(|ui| {
                    if let Some(l) = label {
                        ui.label(RichText::new(l).size(11.5).strong().color(p.muted));
                    }
                    if copyable {
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            let btn = ui
                                .add(
                                    egui::Button::image_and_text(
                                        img(theme::COPY, 11.5, p.muted),
                                        RichText::new("복사").size(11.0).color(p.muted),
                                    )
                                    .fill(p.elevated)
                                    .stroke(Stroke::new(1.0, p.border))
                                    .corner_radius(6)
                                    .min_size(vec2(52.0, 22.0)),
                                )
                                .on_hover_cursor(CursorIcon::PointingHand)
                                .on_hover_text("코드 원문 복사");
                            if btn.clicked() {
                                copied = true;
                            }
                        });
                    }
                });
                ui.add_space(6.0);
            }
            add(ui);
        });
    ui.add_space(6.0);
    copied
}

fn message(ui: &mut egui::Ui, p: &Pal, text: &str) {
    Frame::new()
        .inner_margin(Margin {
            left: 20,
            right: 20,
            top: 6,
            bottom: 18,
        })
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(13.5).color(p.muted));
        });
}

fn slash_row(ui: &mut egui::Ui, p: &Pal, it: &SlashItem, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
    let hot = selected || resp.hovered();
    if hot {
        ui.painter().rect_filled(rect, 6.0, p.hover);
    }
    let [r, g, b] = it.color;
    let c = Color32::from_rgb(r, g, b);
    let ib = Rect::from_min_size(
        pos2(rect.left() + 8.0, rect.center().y - 12.0),
        vec2(24.0, 24.0),
    );
    ui.painter().rect_filled(ib, 6.0, c.gamma_multiply(0.14));
    if hot {
        ui.painter().rect_stroke(
            ib,
            6.0,
            Stroke::new(1.0, p.accent),
            egui::StrokeKind::Inside,
        );
    }
    ui.painter().text(
        ib.center(),
        Align2::CENTER_CENTER,
        it.icon,
        FontId::monospace(11.0),
        c,
    );
    let x = ib.right() + 10.0;
    ui.painter().text(
        pos2(x, rect.center().y - 7.0),
        Align2::LEFT_CENTER,
        it.title,
        FontId::proportional(12.5),
        if hot { p.accent } else { p.text },
    );
    ui.painter().text(
        pos2(x, rect.center().y + 8.0),
        Align2::LEFT_CENTER,
        it.desc,
        FontId::proportional(11.0),
        p.faint,
    );
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

fn help_body(ui: &mut egui::Ui, p: &Pal) {
    egui::ScrollArea::vertical()
        .max_height(460.0)
        .show(ui, |ui| {
            Frame::new()
                .inner_margin(Margin::symmetric(24, 16))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    let table = |ui: &mut egui::Ui,
                                 id: &str,
                                 head: (&str, &str),
                                 rows: &[(&str, &str)]| {
                        egui::Grid::new(id)
                            .striped(true)
                            .num_columns(2)
                            .min_col_width(180.0)
                            .spacing(vec2(16.0, 8.0))
                            .show(ui, |ui| {
                                ui.label(RichText::new(head.0).size(12.5).strong().color(p.text));
                                ui.label(RichText::new(head.1).size(12.5).strong().color(p.text));
                                ui.end_row();
                                for (k, v) in rows {
                                    ui.label(
                                        RichText::new(*k)
                                            .family(FontFamily::Monospace)
                                            .size(12.0)
                                            .color(p.text),
                                    );
                                    ui.label(RichText::new(*v).size(12.5).color(p.muted));
                                    ui.end_row();
                                }
                            });
                    };
                    ui.label(
                        RichText::new("키보드 단축키")
                            .size(13.0)
                            .strong()
                            .color(p.muted),
                    );
                    table(
                        ui,
                        "help-keys",
                        ("단축키", "동작 설명"),
                        &[
                            ("Ctrl + S", "현재 파일 원자적 저장 (입력 2초 후 자동 저장)"),
                            ("Ctrl + N / T", "새 편집 탭 열기"),
                            ("Ctrl + Shift + N", "작업 폴더에 새 마크다운 파일 만들기"),
                            ("Ctrl + W", "현재 탭 닫기"),
                            ("Ctrl + Tab / 1~9", "다음 탭 · 번호별 탭으로 전환"),
                            ("Ctrl + O", "작업 폴더 파일 탐색기 열기"),
                            ("Ctrl + Alt + O", "OS 파일 탐색기로 로컬 .md 파일 열기"),
                            ("Ctrl + Shift + O", "작업 폴더 바꾸기"),
                            (
                                "Ctrl + Z / Ctrl + Y",
                                "실행 취소 / 다시 실행 (블록 편집 포함)",
                            ),
                            ("Ctrl + F / H", "본문 찾기 / 찾기 및 바꾸기"),
                            ("F3 / Shift + F3", "다음 / 이전 일치 항목"),
                            (
                                "Ctrl + B / I / K",
                                "굵게 / 기울임 / 링크 (다시 누르면 해제)",
                            ),
                            (
                                "Ctrl + Shift + S / H",
                                "취소선 (~~) / 형광펜 강조 (==) 토글",
                            ),
                            (
                                "Ctrl + E / Shift + K",
                                "인라인 코드 (`) / 키보드 키캡 ([[키]]) 토글",
                            ),
                            ("Ctrl + \\", "선택 영역의 인라인 마크다운 서식 지우기"),
                            (
                                "Ctrl + Alt + 0 ~ 3",
                                "일반 텍스트(0) / 제목 1(1) · 2(2) · 3(3) 블록 전환",
                            ),
                            (
                                "Ctrl + Alt + 4 ~ 7",
                                "할 일(4) / 글머리(5) / 번호(6) / 인용구(7) 전환",
                            ),
                            (
                                "Ctrl + Alt + 8 / 9 / G",
                                "코드 블록(8) / 콜아웃(9) / 접기 토글(G) 전환",
                            ),
                            ("Ctrl + Enter", "할 일 체크박스 완료([x])/미완료([ ]) 토글"),
                            (
                                "Ctrl + Alt + T / M / -",
                                "표(T) · Mermaid 그래프(M) · 구분선(-) 삽입",
                            ),
                            ("Ctrl + Shift + I", "이미지 삽입 (파일 / 웹 링크)"),
                            ("Ctrl + V (이미지)", "스크린샷을 images/ 에 저장하고 삽입"),
                            ("Ctrl + Shift + L", "왼쪽 문서 목차(TOC) 서랍 열기 / 닫기"),
                            ("Ctrl + Shift + E", "현재 문서를 HTML 파일로 내보내기"),
                            (
                                "Ctrl + Shift + C / Alt + C",
                                "마크다운 원본 전체 복사 / HTML 코드 복사",
                            ),
                            (
                                "Ctrl + Alt + D / W / F",
                                "다크·라이트 테마 / 가운데↔전체 너비 / 글꼴 크기 전환",
                            ),
                            (
                                "F11 / Ctrl+Shift+F",
                                "집중 크게 보기 (상·하단 바 숨기기 토글)",
                            ),
                            (
                                "``` + Space / Enter",
                                "코드 블록 즉시 열기 (```lang 지원 · 닫는 줄 Enter로 탈출)",
                            ),
                            ("Ctrl + P", "인쇄 (PDF로 저장 가능)"),
                            ("Ctrl + Q", "프로그램 안전 종료"),
                            ("/", "빈 블록에서 블록 삽입 명령어 메뉴 열기"),
                            ("마우스 우클릭", "캔버스·탭·파일 탐색기 컨텍스트 메뉴 열기"),
                            (
                                "마우스 드래그 / Shift+클릭",
                                "선택 영역 위 서식 툴바 표시 · 범위 선택",
                            ),
                            ("Shift + 방향키", "블록 경계를 넘어 연속 선택"),
                            ("Alt + ↑ / ↓", "현재 블록 또는 선택된 블록 위·아래로 이동"),
                            ("Ctrl + D / Shift + D", "현재·선택 블록 복제 / 블록 삭제"),
                            ("Ctrl + Shift + Enter", "현재 블록 아래에 새 빈 블록 추가"),
                            ("Ctrl + Home / End", "문서 맨 처음 / 맨 끝 블록으로 이동"),
                            ("Enter / Backspace", "빈 줄·블록 나누기 / 앞 블록과 합치기"),
                            ("Tab / Shift + Tab", "들여쓰기 / 내어쓰기 (다중 블록 지원)"),
                            ("Enter (목록 중)", "번호·체크박스 자동 이어쓰기"),
                            ("Esc", "우클릭 메뉴·찾기 막대·모달 닫기 / 편집 끝내기"),
                            ("F1 / F2", "도움말 / 파일 이름 변경"),
                        ],
                    );
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new("확장 마크다운 문법")
                            .size(13.0)
                            .strong()
                            .color(p.muted),
                    );
                    table(
                        ui,
                        "help-syntax",
                        ("문법", "코드 예시"),
                        &[
                            ("콜아웃 블록", "> [!NOTE]  > [!TIP]  > [!WARNING]"),
                            ("토글 접기", ":::toggle 제목 … :::"),
                            ("LaTeX 수식", "$E=mc^2$  또는  $$ … $$ (원문 표시)"),
                            (
                                "Mermaid 그래프",
                                "```mermaid … ``` (순서도·시퀀스·상태·파이 차트 렌더링)",
                            ),
                            ("형광펜 & 키캡", "==하이라이트==  [[Ctrl]]"),
                            ("로컬 이미지", "![설명](images/arch.png)"),
                            ("각주", "문장[^1]  +  [^1]: 설명"),
                        ],
                    );
                });
        });
}
