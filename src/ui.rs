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
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            self.save_all();
            if self.any_dirty() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.dialog = Some(Dialog::Unsaved(After::Close));
            }
        }
        self.handle_drops(&ctx);
        let pasted: Vec<_> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::PasteImage(img) => Some(img.clone()),
                    _ => None,
                })
                .collect()
        });
        for img in pasted {
            self.insert_pasted_image(&ctx, &img);
        }
        self.handle_shortcuts(&ctx);
        self.auto_save_tick(&ctx);
        self.title_bar(&ctx);

        let p = theme::pal(self.dark);
        egui::Panel::top("header")
            .frame(
                Frame::new()
                    .fill(p.surface)
                    .inner_margin(Margin::symmetric(16, 8)),
            )
            .show(ui, |ui| self.header(ui, &p));
        egui::Panel::bottom("status")
            .frame(
                Frame::new()
                    .fill(p.surface)
                    .inner_margin(Margin::symmetric(16, 4)),
            )
            .show(ui, |ui| self.status_bar(ui, &p));
        if self.toc_open {
            egui::Panel::right("toc")
                .exact_size(280.0)
                .resizable(false)
                .frame(Frame::new().fill(p.surface))
                .show(ui, |ui| self.toc(ui, &p));
        }
        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.bg))
            .show(ui, |ui| self.canvas(ui, &p));

        self.show_toast(&ctx, &p);
        self.dialogs(&ctx, &p);
        self.handle_links(&ctx);
        if self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string("root", self.root.to_string_lossy().into_owned());
        storage.set_string("file", self.current.clone().unwrap_or_default());
        storage.set_string("theme", if self.dark { "dark" } else { "light" }.into());
        storage.set_string("full_width", self.full_width.to_string());
        storage.set_string("small_text", self.small_text.to_string());
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

fn primary(ui: &mut egui::Ui, p: &Pal, text: &str) -> egui::Response {
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

fn ghost(ui: &mut egui::Ui, p: &Pal, text: &str, color: Option<Color32>) -> egui::Response {
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

// 편집 중인 블록의 원문 서식: 제목은 크기·굵기, 코드는 고정폭, 기호는 흐리게
fn md_job(text: &str, body: f32, p: &Pal) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, TextFormat};
    let fmt = |size: f32, family: FontFamily, color: Color32| TextFormat {
        font_id: FontId::new(size, family),
        color,
        line_height: Some(size * 1.6),
        ..Default::default()
    };
    let mut job = LayoutJob::default();
    let mut fence = false;
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
            job.append(&line[..m], 0.0, fmt(size, theme::bold(), p.faint));
            job.append(&line[m..], 0.0, fmt(size, theme::bold(), p.text));
            continue;
        }
        let bare = line.trim_end_matches(['\n', '\r']);
        let marker = if let Some((indent, _, len, _)) = list_marker(bare) {
            indent.len() + len
        } else if let Some(rest) = t.strip_prefix('>') {
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
        job.append(
            &line[marker..],
            0.0,
            fmt(body, FontFamily::Proportional, p.text),
        );
    }
    job
}

struct BlockOut {
    rect: Rect,
    changed: Option<String>,
    consumed: bool,
}

struct EditorOut {
    rect: Rect,
    changed: Option<String>,
    fmt: Option<Fmt>,
    slash: Option<usize>,
}

impl App {
    fn title_bar(&self, ctx: &egui::Context) {
        let title = match &self.current {
            Some(n) if self.docs.get(n).is_some_and(Doc::dirty) => format!("● {n} — MDEditor"),
            Some(n) => format!("{n} — MDEditor"),
            None => "MDEditor".into(),
        };
        // ponytail: 매 프레임 전송하지만 winit이 같은 제목은 무시한다
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
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
            let dirty = self.doc().is_some_and(Doc::dirty);
            let badge = Frame::new()
                .fill(p.elevated)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(8)
                .inner_margin(Margin::symmetric(10, 4))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 7.0;
                    ui.set_max_width(220.0);
                    ui.add(img(theme::FILE, 14.0, p.muted));
                    ui.add(
                        egui::Label::new(
                            RichText::new(name.as_deref().unwrap_or("untitled.md"))
                                .size(12.5)
                                .color(p.text),
                        )
                        .truncate(),
                    );
                    if dirty {
                        dot(ui, AMBER, 3.5, false);
                    }
                })
                .response
                .interact(Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_text("클릭하여 파일 이름 변경 (Rename)");
            if badge.clicked() {
                if let Some(n) = &name {
                    self.ask_rename(n);
                }
            }
            ui.add_space(7.0);
            divider(ui, p);
            ui.add_space(7.0);

            if icon_btn(ui, p, theme::FOLDER, "작업 폴더 파일 탐색기 (Ctrl+O)").clicked() {
                self.open_files();
            }
            if icon_btn(ui, p, theme::UPLOAD, "로컬 파일 열기 (OS 탐색기)").clicked() {
                self.open_local_file();
            }
            if icon_btn(ui, p, theme::PLUS, "새 파일 (Ctrl+N)").clicked() {
                self.dialog = Some(Dialog::NewFile(String::new()));
            }
            if icon_btn(ui, p, theme::SAVE, "문서 저장 (Ctrl+S)").clicked() {
                if let Some(n) = self.current.clone() {
                    self.save(&n);
                }
            }
            ui.add_space(12.0);
            if icon_btn(ui, p, theme::LIST, "문서 목차 (TOC) 열기/닫기").clicked() {
                self.toc_open = !self.toc_open;
            }
            if icon_btn(
                ui,
                p,
                theme::SEARCH,
                "본문 찾기 및 바꾸기 (Ctrl+F / Ctrl+H)",
            )
            .clicked()
            {
                self.open_find(false);
            }
            if icon_btn(ui, p, theme::IMAGE, "이미지 삽입 (Ctrl+Shift+I)").clicked() {
                self.dialog = Some(Dialog::Image {
                    url_tab: false,
                    url: String::new(),
                    alt: String::new(),
                });
            }
            ui.add_space(12.0);
            if icon_btn(ui, p, theme::PRINTER, "PDF 인쇄 (Ctrl+P)").clicked() {
                self.print(&ctx);
            }
            if icon_btn(ui, p, theme::DOWNLOAD, "HTML 파일로 내보내기").clicked() {
                self.export();
            }
            ui.add_space(7.0);
            divider(ui, p);
            ui.add_space(7.0);
            let settings = icon_btn(ui, p, theme::SETTINGS, "설정");
            egui::Popup::menu(&settings).show(|ui| self.settings_menu(ui, p, &ctx));
        });
    }

    fn settings_menu(&mut self, ui: &mut egui::Ui, p: &Pal, ctx: &egui::Context) {
        ui.set_width(200.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        let ic = |i: Icon| Some((i, p.muted));
        let theme_label = if self.dark {
            "라이트 모드로 전환"
        } else {
            "다크 모드로 전환"
        };
        if row(ui, p, ic(theme::SUN), theme_label, p.text, 32.0, None).clicked() {
            self.dark = !self.dark;
            theme::apply(ctx, self.dark);
        }
        let fw = format!("전체 너비: {}", if self.full_width { "켬" } else { "끔" });
        if row(ui, p, ic(theme::MAXIMIZE), &fw, p.text, 32.0, None).clicked() {
            self.full_width = !self.full_width;
        }
        let fs = format!(
            "글꼴 크기: {}",
            if self.small_text { "작게" } else { "보통" }
        );
        if row(ui, p, ic(theme::TYPE), &fs, p.text, 32.0, None).clicked() {
            self.small_text = !self.small_text;
        }
        if row(
            ui,
            p,
            ic(theme::COPY),
            "마크다운 원본 복사",
            p.text,
            32.0,
            None,
        )
        .clicked()
        {
            if let Some(d) = self.doc() {
                ctx.copy_text(d.text.clone());
                self.status = "마크다운 원본을 복사했습니다".into();
            }
            ui.close();
        }
        if row(ui, p, ic(theme::CODE), "HTML 코드 복사", p.text, 32.0, None).clicked() {
            if let Some(d) = self.doc() {
                ctx.copy_text(md_to_html(&d.text));
                self.status = "HTML 코드를 복사했습니다".into();
            }
            ui.close();
        }
        ui.add_space(4.0);
        hline(ui, p);
        ui.add_space(4.0);
        if row(ui, p, ic(theme::HELP), "도움말", p.text, 32.0, None).clicked() {
            self.dialog = Some(Dialog::Help);
            ui.close();
        }
        ui.add_space(4.0);
        hline(ui, p);
        ui.add_space(4.0);
        if row(ui, p, ic(theme::EXIT), "프로그램 종료", RED, 32.0, None).clicked() {
            self.guarded(After::Close);
            ui.close();
        }
    }

    // ---------- 하단 상태바 ----------
    fn status_bar(&self, ui: &mut egui::Ui, p: &Pal) {
        ui.horizontal(|ui| {
            let (label, color, ring) = match self.doc() {
                None => ("준비됨", GREEN, false),
                Some(d) if d.dirty() => ("저장되지 않음", AMBER, true),
                Some(_) => ("저장됨", GREEN, false),
            };
            Frame::new()
                .fill(p.elevated)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(255)
                .inner_margin(Margin::symmetric(9, 2))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    dot(ui, color, 3.0, ring);
                    ui.label(RichText::new(label).size(12.0).color(p.muted));
                });
            let Some(doc) = self.doc() else {
                return;
            };
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                let chars = char_len(&doc.text);
                let lines = doc.text.split('\n').count();
                ui.label(
                    RichText::new(format!(
                        "{chars}자 · {lines}줄 · ~{}분",
                        chars.div_ceil(500).max(1)
                    ))
                    .family(FontFamily::Monospace)
                    .size(11.5)
                    .color(p.faint),
                );
                let (a, b) = self.cursor(ui.ctx());
                if b > a && self.active.is_some() {
                    ui.label(
                        RichText::new(format!("{}자 선택됨", b - a))
                            .family(FontFamily::Monospace)
                            .size(11.5)
                            .color(p.faint),
                    );
                }
            });
        });
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
        let Some(doc) = self.doc() else {
            return;
        };
        let text = doc.text.clone();
        let items = outline(&text);
        egui::ScrollArea::vertical().show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(12)).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                if items.is_empty() {
                    ui.label(
                        RichText::new("제목(#)이 없습니다")
                            .size(13.0)
                            .color(p.faint),
                    );
                }
                for (level, title, _, ci) in items {
                    let b = byte_idx(&text, ci);
                    let active = self.toc_active == Some(b);
                    let indent = match level {
                        1 => 0.0,
                        2 => 14.0,
                        _ => 26.0,
                    };
                    let color = if active {
                        p.accent
                    } else if level == 1 {
                        p.text
                    } else {
                        p.muted
                    };
                    let bg = active.then(|| p.accent.gamma_multiply(0.08));
                    let r = ui
                        .horizontal(|ui| {
                            ui.add_space(indent);
                            row(ui, p, None, &title, color, 30.0, bg)
                        })
                        .inner;
                    if r.clicked() {
                        self.scroll_to = Some(b);
                    }
                }
            });
        });
    }

    // ---------- 노션 라이브 캔버스 ----------
    fn canvas(&mut self, ui: &mut egui::Ui, p: &Pal) {
        let ctx = ui.ctx().clone();
        let area = ui.max_rect();
        let Some(name) = self.current.clone() else {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new("폴더 아이콘(Ctrl+O)으로 문서를 열거나 + 로 새 파일을 만드세요.\n.md 파일이나 폴더를 창에 끌어다 놓아도 됩니다.")
                        .size(14.0)
                        .color(p.faint),
                );
            });
            return;
        };
        if let Some(sel) = self.pending_sel.take() {
            self.place_cursor(&ctx, &name, sel);
        }
        let text = self.docs[&name].text.clone();
        let blocks = split_blocks(&text);
        let active = self.active_block();
        if let Some(r) = &active {
            self.active = Some(r.start);
        }
        let mut items: Vec<(Range<usize>, bool)> = blocks
            .iter()
            .map(|b| (b.clone(), active.as_ref() == Some(b)))
            .collect();
        if let Some(r) = active.filter(|r| r.is_empty() && !blocks.contains(r)) {
            let at = items
                .iter()
                .position(|(b, _)| b.start > r.start)
                .unwrap_or(items.len());
            items.insert(at, (r, true));
        }
        let body = if self.small_text { 14.5 } else { 16.0 };
        let scheme = format!(
            "file:///{}/",
            to_slash(&self.doc_dir()).trim_start_matches('/')
        );
        let clicked = ctx.input(|i| i.pointer.primary_clicked());
        let cmds_before = ctx.output(|o| o.commands.len());
        let mut change: Option<(Range<usize>, String)> = None;
        let mut go_to: Option<(usize, usize)> = None;
        let mut consumed = false;
        let mut below = false;
        let mut fmt = None;
        let mut slash = None;
        let mut toc_active = None;

        egui::ScrollArea::vertical()
            .id_salt(("canvas", &name))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let full = ui.available_width();
                let w = if self.full_width {
                    (full - 72.0).max(200.0)
                } else {
                    (full - 64.0).clamp(200.0, 840.0)
                };
                let pad = ((full - w) / 2.0).max(16.0);
                ui.add_space(36.0);
                ui.horizontal_top(|ui| {
                    ui.add_space(pad);
                    ui.vertical(|ui| {
                        ui.set_width(w);
                        canvas_style(ui, body, p);
                        let view_top = ui.clip_rect().top();
                        for (r, is_active) in &items {
                            if *is_active {
                                let out =
                                    self.block_editor(ui, &ctx, p, &name, &text, r.clone(), body);
                                if out.changed.is_some() {
                                    change = out.changed.map(|s| (r.clone(), s));
                                }
                                fmt = fmt.or(out.fmt);
                                slash = slash.or(out.slash);
                                if clicked && ui.rect_contains_pointer(out.rect) {
                                    consumed = true;
                                }
                            } else {
                                let out =
                                    self.render_block(ui, p, &text, r.clone(), body, w, &scheme);
                                let full_row = Rect::from_x_y_ranges(
                                    ui.max_rect().x_range(),
                                    out.rect.y_range(),
                                );
                                if let Some(s) = out.changed {
                                    change = Some((r.clone(), s));
                                    consumed = true;
                                }
                                consumed |= out.consumed;
                                if clicked && !consumed && ui.rect_contains_pointer(full_row) {
                                    go_to = Some((r.start, r.end));
                                    consumed = true;
                                }
                                if ui.rect_contains_pointer(full_row) {
                                    ctx.set_cursor_icon(CursorIcon::Text);
                                }
                                if heading_level(text[r.clone()].trim_start()).is_some()
                                    && out.rect.top() <= view_top + 80.0
                                {
                                    toc_active = Some(r.start);
                                }
                                if self.scroll_to == Some(r.start) {
                                    ui.scroll_to_rect(out.rect, Some(Align::TOP));
                                    self.scroll_to = None;
                                }
                            }
                            ui.add_space(4.0);
                        }
                        // 마지막 블록 아래 빈 곳을 누르면 새 블록 (노션과 같음)
                        let rest = ui.available_height().max(140.0);
                        let (_, resp) = ui.allocate_exact_size(vec2(w, rest), Sense::click());
                        if resp.hovered() {
                            ctx.set_cursor_icon(CursorIcon::Text);
                        }
                        if resp.clicked() {
                            below = true;
                            consumed = true;
                        }
                    });
                });
            });

        // 링크·복사 버튼을 누른 클릭은 블록 편집으로 넘어가지 않는다
        if ctx.output(|o| o.commands.len()) > cmds_before {
            go_to = None;
        }
        if let Some((r, s)) = change.take() {
            if let Some(doc) = self.docs.get_mut(&name) {
                doc.text.replace_range(r, &s);
                doc.edited_at = Some(Instant::now());
            }
            go_to = None;
        }
        self.toc_active = toc_active;
        if let Some((s, c)) = go_to {
            self.go(s, c);
        } else if below {
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

    // 전체 문서 기준 선택(sel)을 블록 편집기 상태로 옮긴다
    fn place_cursor(&mut self, ctx: &egui::Context, name: &str, sel: Sel) {
        let text = &self.docs[name].text;
        let b = byte_idx(text, sel.0);
        let start = split_blocks(text)
            .into_iter()
            .find(|r| r.start <= b && b <= r.end)
            .map_or(b, |r| r.start);
        let base = char_len(&text[..start]);
        let id = Self::editor_id(name, start);
        let mut st = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        st.cursor.set_char_range(Some(CCursorRange::two(
            CCursor::new(sel.0.saturating_sub(base)),
            CCursor::new(sel.1.saturating_sub(base)),
        )));
        egui::TextEdit::store_state(ctx, id, st);
        self.active = Some(start);
        self.scroll_cursor = true;
    }

    #[allow(clippy::too_many_arguments)]
    fn block_editor(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        p: &Pal,
        name: &str,
        text: &str,
        r: Range<usize>,
        body: f32,
    ) -> EditorOut {
        let id = Self::editor_id(name, r.start);
        let mut buf = text[r.clone()].to_string();
        let pal = *p;
        let mut layouter = move |ui: &egui::Ui, b: &dyn egui::TextBuffer, wrap: f32| {
            let mut job = md_job(b.as_str(), body, &pal);
            job.wrap.max_width = wrap;
            ui.ctx().fonts_mut(|f| f.layout_job(job))
        };
        let out = egui::TextEdit::multiline(&mut buf)
            .id(id)
            .frame(Frame::NONE)
            .margin(Margin::ZERO)
            .desired_width(f32::INFINITY)
            .desired_rows(1)
            .lock_focus(true)
            .hint_text(RichText::new(PLACEHOLDER).size(body).color(p.faint))
            .layouter(&mut layouter)
            .show(ui);
        if std::mem::take(&mut self.focus_editor) {
            out.response.request_focus();
        }
        let cursor_of = |c: usize| {
            out.galley
                .pos_from_cursor(CCursor::new(c))
                .translate(out.galley_pos.to_vec2())
        };
        let sel = out.cursor_range.map(|c| c.as_sorted_char_range());
        if std::mem::take(&mut self.scroll_cursor) {
            let c = sel.as_ref().map_or(0, |s| s.end.0);
            ui.scroll_to_rect(cursor_of(c), Some(Align::Center));
        }
        let changed = (buf != text[r.clone()]).then(|| buf.clone());

        // 선택 영역 위 서식 막대
        let mut fmt = None;
        if let Some(s) = sel.filter(|s| s.start.0 != s.end.0) {
            let at = cursor_of(s.start.0).left_top() - vec2(0.0, 40.0);
            egui::Area::new(egui::Id::unique("format-bar"))
                .order(Order::Foreground)
                .fixed_pos(at)
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
                                    "취소선",
                                    Fmt::Wrap("~~", "~~", "취소선"),
                                );
                                divider(ui, p);
                                b(
                                    ui,
                                    theme::HIGHLIGHT,
                                    Color32::from_rgb(0xea, 0xb3, 0x08),
                                    "형광펜 강조",
                                    Fmt::Wrap("==", "==", "강조"),
                                );
                                b(
                                    ui,
                                    theme::CODE,
                                    m,
                                    "인라인 코드",
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
                                    .on_hover_text("키보드 키 ([[키]])")
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
                                b(ui, theme::X, m, "서식 지우기", Fmt::Clear);
                                fmt = fmt
                                    .or(pick)
                                    .or(kbd.then_some(Fmt::Wrap("[[", "]]", "Ctrl")));
                            });
                        });
                });
        }

        // 슬래시 메뉴
        let mut slash = None;
        if let Some(q) = buf
            .strip_prefix('/')
            .filter(|q| !q.contains(char::is_whitespace))
        {
            let matches = slash_matches(q);
            let sel_idx = self.slash_sel.min(matches.len().saturating_sub(1));
            egui::Area::new(egui::Id::unique("slash-menu"))
                .order(Order::Foreground)
                .fixed_pos(out.response.rect.left_bottom() + vec2(0.0, 4.0))
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
        }
        EditorOut {
            rect: out.response.rect,
            changed,
            fmt,
            slash,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_block(
        &mut self,
        ui: &mut egui::Ui,
        p: &Pal,
        text: &str,
        r: Range<usize>,
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
            .push_id(r.start, |ui| {
                ui.vertical(|ui| {
                    if let Some(level) = heading_level(t) {
                        ui.add_space(HEADING_TOP[level - 1]);
                        let title = t[level..].trim().trim_end_matches('#').trim();
                        ui.label(
                            RichText::new(title)
                                .font(FontId::new(body * HEADING_SCALE[level - 1], theme::bold()))
                                .color(p.text),
                        );
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
                            .inner_margin(Margin::symmetric(12, 8))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                let head = ui
                                    .horizontal(|ui| {
                                        ui.label(
                                            RichText::new(if open { "▼" } else { "▶" })
                                                .color(p.muted),
                                        );
                                        ui.label(
                                            RichText::new(title)
                                                .font(FontId::new(body, theme::bold()))
                                                .color(p.text),
                                        );
                                    })
                                    .response
                                    .interact(Sense::click())
                                    .on_hover_cursor(CursorIcon::PointingHand);
                                if head.clicked() {
                                    open = !open;
                                    consumed = true;
                                    ui.data_mut(|d| d.insert_temp(open_id, open));
                                }
                                if open {
                                    ui.add_space(4.0);
                                    let inner_resp = ui.horizontal(|ui| {
                                        ui.add_space(18.0);
                                        ui.vertical(|ui| {
                                            self.markdown(ui, inner, width - 60.0, scheme, dark)
                                        })
                                        .inner
                                    });
                                    let rr = inner_resp.response.rect;
                                    ui.painter().vline(
                                        rr.left() + 9.0,
                                        rr.y_range(),
                                        Stroke::new(2.0, p.border_hover),
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
                        card(ui, p, None, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new(inner)
                                        .family(FontFamily::Monospace)
                                        .size(body)
                                        .color(p.text),
                                );
                            });
                        });
                    } else if let Some(rest) = t.strip_prefix("```mermaid") {
                        let code = rest.trim_end().trim_end_matches('`').trim_matches('\n');
                        card(ui, p, Some("Mermaid 다이어그램"), |ui| {
                            ui.label(
                                RichText::new(code)
                                    .family(FontFamily::Monospace)
                                    .size(body * 0.84)
                                    .color(p.text),
                            );
                        });
                    } else {
                        changed = self.markdown(ui, src, width, scheme, dark);
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
    fn markdown(
        &mut self,
        ui: &mut egui::Ui,
        src: &str,
        width: f32,
        scheme: &str,
        dark: bool,
    ) -> Option<String> {
        let before = preprocess(src);
        let mut shown = before.clone();
        let math = |ui: &mut egui::Ui, s: &str, _inline: bool| {
            ui.code(s.trim());
        };
        CommonMarkViewer::new()
            .default_implicit_uri_scheme(scheme)
            .max_image_width(Some(width as usize))
            .render_math_fn(Some(&math))
            .alerts(alerts(dark))
            .show_mut(ui, &mut self.md_cache, &mut shown);
        if shown == before {
            return None;
        }
        let mut s = src.to_string();
        apply_preview_change(&mut s, &before, &shown).then_some(s)
    }

    // ---------- 떠 있는 찾기·바꾸기 막대 (오른쪽 위) ----------
    fn find_bar(&mut self, ctx: &egui::Context, p: &Pal, area: Rect) {
        if !self.find.open {
            return;
        }
        let matches = self
            .doc()
            .map(|d| find_all(&d.text, &self.find.query, self.find.case))
            .unwrap_or_default();
        let sel = self.cursor(ctx);
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
                            }
                        });
                        if self.find.replace {
                            ui.horizontal(|ui| {
                                Frame::new()
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
                                        );
                                    });
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
            Dialog::Rename { .. } => 2,
            Dialog::Delete(_) => 3,
            Dialog::Unsaved(_) => 4,
            Dialog::Image { .. } => 5,
            Dialog::Help => 6,
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
                    Dialog::NewFile(new_name) => {
                        cancel |= modal_header(ui, p, theme::PLUS, p.accent, "새 파일");
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
                                    egui::Id::unique("new-file"),
                                    new_name,
                                    "새문서.md",
                                    f32::INFINITY,
                                );
                                r.request_focus();
                                submit = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                            });
                        footer(ui, p, None, |ui| {
                            submit |= primary(ui, p, "만들기").clicked();
                            cancel |= ghost(ui, p, "취소", None).clicked();
                        });
                        if submit {
                            match self.create_file(new_name) {
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
                                let r =
                                    input(ui, p, egui::Id::unique("rename"), to, "", f32::INFINITY);
                                r.request_focus();
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
                        let names: Vec<String> = self
                            .docs
                            .iter()
                            .filter(|(_, d)| d.dirty())
                            .map(|(n, _)| n.clone())
                            .collect();
                        let what = if matches!(after, After::Close) {
                            "종료하기"
                        } else {
                            "폴더를 바꾸기"
                        };
                        message(
                            ui,
                            p,
                            &format!("{} 전에 저장할까요?\n{}", what, names.join(", ")),
                        );
                        footer(ui, p, Some(p.surface), |ui| {
                            if primary(ui, p, "저장 후 계속").clicked() {
                                if self.save_all() {
                                    close = true;
                                } else {
                                    error = Some(self.status.clone());
                                }
                            }
                            if ghost(ui, p, "저장 안 함", Some(RED)).clicked() {
                                for d in self.docs.values_mut() {
                                    d.text = d.saved.clone();
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
                }
                if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp)) {
                    *sel = (*sel + n - 1) % n;
                }
                if ui.input(|i| i.key_pressed(Key::Enter)) {
                    open = shown.get(*sel).cloned();
                }
            });
        hline(ui, p);
        egui::ScrollArea::vertical()
            .max_height(380.0)
            .show(ui, |ui| {
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
                        }
                        for (i, name) in shown.iter().enumerate() {
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
                                    egui::TextFormat::simple(FontId::proportional(11.0), p.faint),
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
                                pos2(rect.left() + 32.0, rect.center().y - galley.size().y / 2.0),
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
                                    ui.painter().rect_filled(del, 4.0, RED.gamma_multiply(0.14));
                                }
                                img(theme::TRASH, 14.0, if dr.hovered() { RED } else { p.faint })
                                    .paint_at(
                                        ui,
                                        Rect::from_center_size(del.center(), vec2(14.0, 14.0)),
                                    );
                                if dr.on_hover_text("파일 삭제").clicked() {
                                    deleting = true;
                                    *next = Some(Dialog::Delete(name.clone()));
                                }
                            }
                            if resp.clicked() && !deleting {
                                open = Some(name.clone());
                            }
                        }
                    });
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
                    RichText::new("↑↓ 이동  ·  Enter 열기  ·  Esc 닫기")
                        .size(11.5)
                        .color(p.faint),
                );
            });
        });
        if let Some(name) = open {
            self.open_file(&name);
            *close = true;
        }
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

fn canvas_style(ui: &mut egui::Ui, body: f32, p: &Pal) {
    use egui::TextStyle;
    let s = ui.style_mut();
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

fn card(ui: &mut egui::Ui, p: &Pal, label: Option<&str>, add: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(6.0);
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(10)
        .inner_margin(Margin::symmetric(20, 16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if let Some(l) = label {
                ui.label(RichText::new(l).size(11.5).color(p.faint));
            }
            add(ui);
        });
    ui.add_space(6.0);
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
                            ("Ctrl + N", "새 파일 만들기"),
                            ("Ctrl + O", "작업 폴더 파일 탐색기 열기"),
                            ("Ctrl + Shift + O", "작업 폴더 바꾸기"),
                            ("Ctrl + F / H", "본문 찾기 / 찾기 및 바꾸기"),
                            ("Ctrl + B / I / K", "선택 영역 굵게 / 기울임 / 하이퍼링크"),
                            ("Ctrl + Shift + I", "이미지 삽입 (파일 / 웹 링크)"),
                            ("Ctrl + V (이미지)", "스크린샷을 images/ 에 저장하고 삽입"),
                            ("Ctrl + P", "인쇄 (PDF로 저장 가능)"),
                            ("Ctrl + Q", "프로그램 안전 종료"),
                            ("/", "빈 블록에서 블록 삽입 명령어 메뉴 열기"),
                            ("마우스 드래그", "선택 영역 위에 서식 툴바 표시"),
                            ("Enter / Backspace", "블록 나누기 / 앞 블록과 합치기"),
                            ("Tab / Shift + Tab", "들여쓰기 / 내어쓰기"),
                            ("Enter (목록 중)", "번호·체크박스 자동 이어쓰기"),
                            ("Esc", "편집 끝내기 · 모달 / 찾기 막대 닫기"),
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
                            ("Mermaid", "```mermaid … ``` (원문 표시)"),
                            ("형광펜 & 키캡", "==하이라이트==  [[Ctrl]]"),
                            ("로컬 이미지", "![설명](images/arch.png)"),
                            ("각주", "문장[^1]  +  [^1]: 설명"),
                        ],
                    );
                });
        });
}
