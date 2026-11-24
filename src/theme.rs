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
            border: a(255, 255, 255, 24),
            border_hover: a(255, 255, 255, 48),
            text: Color32::from_rgb(0xeb, 0xeb, 0xeb),
            muted: Color32::from_rgb(0x9b, 0x9b, 0x9b),
            faint: Color32::from_rgb(0x6e, 0x6e, 0x6e),
            accent: Color32::from_rgb(0x23, 0x83, 0xe2),
            code_bg: Color32::from_rgb(0x21, 0x22, 0x24),
            overlay: a(0, 0, 0, 173),
            shadow: a(0, 0, 0, 145),
        }
    } else {
        Pal {
            bg: Color32::WHITE,
            surface: Color32::from_rgb(0xf7, 0xf7, 0xf5),
            elevated: Color32::WHITE,
            hover: Color32::from_rgb(0xed, 0xed, 0xea),
            border: a(15, 15, 15, 24),
            border_hover: a(15, 15, 15, 48),
            text: Color32::from_rgb(0x37, 0x35, 0x2f),
            muted: Color32::from_rgb(0x78, 0x77, 0x74),
            faint: Color32::from_rgb(0x9b, 0x9a, 0x97),
            accent: Color32::from_rgb(0x23, 0x83, 0xe2),
            code_bg: Color32::from_rgb(0xf6, 0xf6, 0xf4),
            overlay: a(15, 15, 15, 107),
            shadow: a(15, 15, 15, 28),
        }
    }
}

pub fn shadow(p: &Pal) -> Shadow {
    Shadow {
        offset: [0, 12],
        blur: 28,
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
        s.corner_radius = CornerRadius::same(7);
        s.fg_stroke = Stroke::new(1.0, p.text);
        s.expansion = 0.0;
    }
    w.noninteractive.bg_fill = p.surface;
    w.noninteractive.weak_bg_fill = p.surface;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.inactive.weak_bg_fill = Color32::TRANSPARENT; // 고스트 버튼
    w.inactive.bg_fill = p.elevated;
    w.inactive.bg_stroke = Stroke::NONE;
    w.hovered.weak_bg_fill = p.hover;
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
    // 창 제목 표시줄도 앱 테마를 따르게 한다
    ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(if dark {
        egui::SystemTheme::Dark
    } else {
        egui::SystemTheme::Light
    }));
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
        s.spacing.button_padding = egui::vec2(9.0, 5.0);
        s.spacing.window_margin = Margin::same(0);
        s.spacing.menu_margin = Margin::same(6);
        s.spacing.interact_size.y = 26.0;
    });
}

pub fn bold() -> FontFamily {
    static BOLD: std::sync::OnceLock<FontFamily> = std::sync::OnceLock::new();
    BOLD.get_or_init(|| FontFamily::Name("bold".into())).clone()
}

#[cfg(windows)]
mod win_mem {
    use std::fs::File;
    use std::os::windows::io::AsRawHandle;
    use std::path::Path;
    use std::ptr::NonNull;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateFileMappingW(
            hFile: *mut core::ffi::c_void,
            lpAttributes: *const core::ffi::c_void,
            flProtect: u32,
            dwMaximumSizeHigh: u32,
            dwMaximumSizeLow: u32,
            lpName: *const u16,
        ) -> *mut core::ffi::c_void;
        fn MapViewOfFile(
            hFileMappingObject: *mut core::ffi::c_void,
            dwDesiredAccess: u32,
            dwFileOffsetHigh: u32,
            dwFileOffsetLow: u32,
            dwNumberOfBytesToMap: usize,
        ) -> *mut core::ffi::c_void;
        fn UnmapViewOfFile(lpBaseAddress: *const core::ffi::c_void) -> i32;
        fn CloseHandle(hObject: *mut core::ffi::c_void) -> i32;
        fn GlobalLock(hMem: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
        fn GlobalUnlock(hMem: *mut core::ffi::c_void) -> i32;
        fn GlobalSize(hMem: *mut core::ffi::c_void) -> usize;
    }

    #[link(name = "user32")]
    extern "system" {
        fn OpenClipboard(hWndNewOwner: *mut core::ffi::c_void) -> i32;
        fn CloseClipboard() -> i32;
        fn GetClipboardData(uFormat: u32) -> *mut core::ffi::c_void;
    }

    // 시스템 폰트 파일을 힙에 복사(Vec<u8>, ~50MB)하지 않고 OS 파일 캐시에 읽기 전용으로 매핑한다 (PrivateBytes 0)
    pub struct MappedFile {
        ptr: NonNull<u8>,
        len: usize,
    }

    unsafe impl Send for MappedFile {}
    unsafe impl Sync for MappedFile {}

    impl MappedFile {
        pub fn open(path: &Path) -> Option<Self> {
            let file = File::open(path).ok()?;
            let len = usize::try_from(file.metadata().ok()?.len()).ok()?;
            if len == 0 {
                return None;
            }
            unsafe {
                let mapping = CreateFileMappingW(
                    file.as_raw_handle() as *mut _,
                    core::ptr::null(),
                    0x02, // PAGE_READONLY
                    0,
                    0,
                    core::ptr::null(),
                );
                if mapping.is_null() {
                    return None;
                }
                let view = MapViewOfFile(mapping, 0x04, 0, 0, len); // FILE_MAP_READ
                CloseHandle(mapping);
                let ptr = NonNull::new(view.cast::<u8>())?;
                Some(Self { ptr, len })
            }
        }
    }

    impl AsRef<[u8]> for MappedFile {
        fn as_ref(&self) -> &[u8] {
            unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
        }
    }

    impl Drop for MappedFile {
        fn drop(&mut self) {
            unsafe {
                UnmapViewOfFile(self.ptr.as_ptr().cast());
            }
        }
    }

    pub fn clipboard_text() -> Option<String> {
        const CF_UNICODETEXT: u32 = 13;
        unsafe {
            if OpenClipboard(core::ptr::null_mut()) == 0 {
                return None;
            }
            let out = (|| {
                let handle = GetClipboardData(CF_UNICODETEXT);
                if handle.is_null() {
                    return None;
                }
                let max_u16 = GlobalSize(handle) / 2;
                if max_u16 == 0 {
                    return None;
                }
                let ptr = GlobalLock(handle).cast::<u16>();
                if ptr.is_null() {
                    return None;
                }
                let slice = core::slice::from_raw_parts(ptr, max_u16);
                let len = slice.iter().position(|&c| c == 0).unwrap_or(max_u16);
                let s = String::from_utf16_lossy(&slice[..len]).replace("\r\n", "\n");
                GlobalUnlock(handle);
                (!s.is_empty()).then_some(s)
            })();
            CloseClipboard();
            out
        }
    }
}

// macOS·Linux: 같은 이유로 시스템 폰트(AppleSDGothicNeo.ttc ~55MB)를 mmap으로 읽기 전용 매핑한다
#[cfg(unix)]
mod unix_mem {
    use std::os::unix::io::AsRawFd;
    use std::path::Path;
    use std::ptr::NonNull;

    extern "C" {
        fn mmap(
            addr: *mut core::ffi::c_void,
            len: usize,
            prot: i32,
            flags: i32,
            fd: i32,
            offset: i64,
        ) -> *mut core::ffi::c_void;
        fn munmap(addr: *mut core::ffi::c_void, len: usize) -> i32;
    }

    pub struct MappedFile {
        ptr: NonNull<u8>,
        len: usize,
    }

    unsafe impl Send for MappedFile {}
    unsafe impl Sync for MappedFile {}

    impl MappedFile {
        pub fn open(path: &Path) -> Option<Self> {
            let file = std::fs::File::open(path).ok()?;
            let len = usize::try_from(file.metadata().ok()?.len()).ok()?;
            if len == 0 {
                return None;
            }
            const PROT_READ: i32 = 1;
            const MAP_PRIVATE: i32 = 2;
            // 매핑은 fd를 닫아도 유지된다
            let p = unsafe {
                mmap(
                    core::ptr::null_mut(),
                    len,
                    PROT_READ,
                    MAP_PRIVATE,
                    file.as_raw_fd(),
                    0,
                )
            };
            if p as isize == -1 {
                return None; // MAP_FAILED
            }
            Some(Self {
                ptr: NonNull::new(p.cast())?,
                len,
            })
        }
    }

    impl AsRef<[u8]> for MappedFile {
        fn as_ref(&self) -> &[u8] {
            unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
        }
    }

    impl Drop for MappedFile {
        fn drop(&mut self) {
            unsafe {
                munmap(self.ptr.as_ptr().cast(), self.len);
            }
        }
    }
}

pub fn clipboard_text() -> Option<String> {
    #[cfg(windows)]
    return win_mem::clipboard_text();
    #[cfg(not(windows))]
    {
        let mut cb = arboard::Clipboard::new().ok()?;
        let s = cb.get_text().ok()?.replace("\r\n", "\n");
        (!s.is_empty()).then_some(s)
    }
}

// index: .ttc 글꼴 모음 안의 글꼴 번호 (단일 .ttf/.otf는 0)
fn load_font_data(path: &std::path::Path, index: u32) -> Option<egui::FontData> {
    #[cfg(windows)]
    if let Some(mapped) = win_mem::MappedFile::open(path) {
        return Some(egui::FontData::from_blob(
            std::sync::Arc::new(mapped),
            index,
        ));
    }
    #[cfg(unix)]
    if let Some(mapped) = unix_mem::MappedFile::open(path) {
        return Some(egui::FontData::from_blob(
            std::sync::Arc::new(mapped),
            index,
        ));
    }
    let mut fd = egui::FontData::from_owned(std::fs::read(path).ok()?);
    fd.index = index;
    Some(fd)
}

// "AppleSDGothicNeo.ttc#6" → ("AppleSDGothicNeo.ttc", 6)
fn split_face(pre: &str) -> (&str, u32) {
    pre.rsplit_once('#')
        .and_then(|(name, i)| Some((name, i.parse().ok()?)))
        .unwrap_or((pre, 0))
}

pub struct FontPreset {
    pub id: &'static str,
    pub label: &'static str,
    pub regular: &'static [&'static str],
    pub bold: &'static [&'static str],
}

const FONT_PRESETS: &[FontPreset] = &[
    FontPreset {
        id: "malgun",
        label: "맑은 고딕",
        regular: &["malgun.ttf"],
        bold: &["malgunbd.ttf"],
    },
    FontPreset {
        id: "kopub",
        label: "KoPubWorld 돋움",
        regular: &["KoPubWorld Dotum Medium", "KoPubDotumMedium"],
        bold: &["KoPubWorld Dotum Bold", "KoPubDotumBold"],
    },
    FontPreset {
        id: "pretendard",
        label: "Pretendard",
        regular: &[
            "Pretendard-Regular",
            "Pretendard-Medium",
            "PretendardVariable",
            "Pretendard",
        ],
        bold: &["Pretendard-Bold", "Pretendard-SemiBold"],
    },
    FontPreset {
        id: "nanum",
        label: "나눔고딕",
        regular: &["NanumGothic.ttf", "NanumGothic", "NanumBarunGothic"],
        bold: &["NanumGothicBold", "NanumBarunGothicBold"],
    },
    FontPreset {
        id: "nanummyeongjo",
        label: "나눔명조 (명조체)",
        regular: &["NanumMyeongjo.ttf", "NanumMyeongjo"],
        bold: &["NanumMyeongjoBold"],
    },
    FontPreset {
        id: "notosanskr",
        label: "Noto Sans KR",
        regular: &["NotoSansKR-Regular", "NotoSansKR"],
        bold: &["NotoSansKR-Bold"],
    },
    FontPreset {
        id: "d2coding",
        label: "D2Coding (코딩체)",
        regular: &["D2Coding"],
        bold: &["D2CodingBold"],
    },
    FontPreset {
        id: "arial",
        label: "Arial + 맑은 고딕",
        regular: &["arial.ttf"],
        bold: &["arialbd.ttf", "Arial Bold.ttf"],
    },
    FontPreset {
        id: "georgia",
        label: "Georgia (세리프) + 맑은 고딕",
        regular: &["georgia.ttf"],
        bold: &["georgiab.ttf", "Georgia Bold.ttf"],
    },
    FontPreset {
        id: "consolas",
        label: "Consolas (고정폭)",
        regular: &["consola.ttf"],
        bold: &["consolab.ttf"],
    },
];

#[cfg(not(target_os = "macos"))]
fn font_dirs() -> Vec<std::path::PathBuf> {
    let win = std::path::PathBuf::from(r"C:\Windows\Fonts");
    let user = std::env::var_os("LOCALAPPDATA")
        .map(|d| std::path::PathBuf::from(d).join(r"Microsoft\Windows\Fonts"));
    user.into_iter().chain([win]).collect()
}

#[cfg(target_os = "macos")]
fn font_dirs() -> Vec<std::path::PathBuf> {
    let user = std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join("Library/Fonts"));
    user.into_iter()
        .chain(
            [
                "/Library/Fonts",
                "/System/Library/Fonts",
                "/System/Library/Fonts/Supplemental",
            ]
            .map(std::path::PathBuf::from),
        )
        .collect()
}

fn scan_font_entries() -> &'static Vec<(String, std::path::PathBuf)> {
    static ENTRIES: std::sync::OnceLock<Vec<(String, std::path::PathBuf)>> =
        std::sync::OnceLock::new();
    ENTRIES.get_or_init(|| {
        font_dirs()
            .into_iter()
            .filter_map(|d| std::fs::read_dir(d).ok())
            .flatten()
            .flatten()
            .filter_map(|e| {
                let n = e.file_name().to_string_lossy().to_lowercase();
                (n.ends_with(".ttf") || n.ends_with(".otf") || n.ends_with(".ttc"))
                    .then(|| (n, e.path()))
            })
            .collect()
    })
}

fn find_font_path(prefixes: &[&str]) -> Option<(usize, std::path::PathBuf)> {
    let dirs = font_dirs();
    let entries = scan_font_entries();
    prefixes.iter().enumerate().find_map(|(idx, pre)| {
        let pre = split_face(pre).0;
        let lower = pre.to_lowercase();
        if lower.ends_with(".ttf") || lower.ends_with(".otf") || lower.ends_with(".ttc") {
            if let Some(p) = dirs.iter().map(|d| d.join(pre)).find(|p| p.is_file()) {
                return Some((idx, p));
            }
        }
        entries
            .iter()
            .find(|(n, _)| n.starts_with(&lower))
            .map(|(_, p)| (idx, p.clone()))
    })
}

pub fn available_fonts() -> &'static Vec<(String, String)> {
    static LIST: std::sync::OnceLock<Vec<(String, String)>> = std::sync::OnceLock::new();
    LIST.get_or_init(|| {
        let mut out = vec![(
            "auto".to_string(),
            "기본 자동 (Segoe UI · 맑은 고딕)".to_string(),
        )];
        for p in FONT_PRESETS {
            if find_font_path(p.regular).is_some() {
                out.push((p.id.to_string(), p.label.to_string()));
            }
        }
        out
    })
}

pub fn font_display_name(choice: &str) -> std::borrow::Cow<'static, str> {
    if choice.is_empty() || choice == "auto" {
        return std::borrow::Cow::Borrowed("기본");
    }
    if let Some(p) = FONT_PRESETS.iter().find(|p| p.id == choice) {
        return std::borrow::Cow::Borrowed(p.label);
    }
    let p = std::path::Path::new(choice);
    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
        return std::borrow::Cow::Owned(stem.to_string());
    }
    std::borrow::Cow::Borrowed("사용자 글꼴")
}

fn build_fonts(choice: &str) -> egui::FontDefinitions {
    let find = |prefixes: &[&str]| -> Option<(usize, egui::FontData)> {
        let (idx, path) = find_font_path(prefixes)?;
        load_font_data(&path, split_face(prefixes[idx]).1).map(|fd| (idx, fd))
    };
    let mut fonts = egui::FontDefinitions::empty();
    let mut add = |key: &str, found: Option<(usize, egui::FontData)>| -> Option<(usize, String)> {
        let (idx, fd) = found?;
        fonts.font_data.insert(key.into(), fd.into());
        Some((idx, key.to_string()))
    };
    let custom_path =
        (!choice.is_empty() && choice != "auto" && FONT_PRESETS.iter().all(|p| p.id != choice))
            .then(|| std::path::Path::new(choice));
    let preset = FONT_PRESETS.iter().find(|p| p.id == choice);

    let reg_found = if let Some(path) = custom_path {
        add("regular", load_font_data(path, 0).map(|fd| (0, fd)))
    } else if let Some(p) = preset {
        add("regular", find(p.regular))
    } else {
        add(
            "regular",
            find(&[
                "KoPubWorld Dotum Medium",
                "segoeui.ttf",
                "AppleSDGothicNeo.ttc#0",
            ]),
        )
    };
    let bold_found = if custom_path.is_some() {
        None
    } else if let Some(p) = preset {
        add("bold", find(p.bold))
    } else {
        add(
            "bold",
            find(&[
                "KoPubWorld Dotum Bold",
                "segoeuib.ttf",
                "AppleSDGothicNeo.ttc#6",
            ]),
        )
    };

    let regular = reg_found.as_ref().map(|(_, k)| k.clone());
    let bold_f = bold_found.as_ref().map(|(_, k)| k.clone());
    let ko = add("malgun", find(&["malgun.ttf", "AppleSDGothicNeo.ttc#0"])).map(|(_, k)| k);
    let ko_bold = if (preset.is_none() && bold_found.as_ref().is_some_and(|(idx, _)| *idx == 0))
        || (choice == "kopub" && bold_f.is_some())
        || (choice == "malgun" && bold_f.is_some())
    {
        None
    } else {
        add(
            "malgunbd",
            find(&["malgunbd.ttf", "AppleSDGothicNeo.ttc#6"]),
        )
        .map(|(_, k)| k)
    };
    let mono = if choice == "d2coding" {
        add("mono", find(&["D2Coding", "consola.ttf", "Menlo.ttc#0"])).map(|(_, k)| k)
    } else {
        add("mono", find(&["consola.ttf", "Menlo.ttc#0"])).map(|(_, k)| k)
    };
    let sym = add("symbol", find(&["seguisym.ttf", "Apple Symbols.ttf"])).map(|(_, k)| k);
    let emoji = add("emoji", find(&["seguiemj.ttf"])).map(|(_, k)| k);
    let chain = |list: &[&Option<String>]| -> Vec<String> {
        list.iter().filter_map(|o| (*o).clone()).collect()
    };
    let prop_chain = chain(&[&regular, &ko, &sym, &emoji]);
    if prop_chain.is_empty() {
        return egui::FontDefinitions::default();
    }
    fonts.families.insert(FontFamily::Proportional, prop_chain);
    fonts
        .families
        .insert(FontFamily::Monospace, chain(&[&mono, &ko, &sym, &emoji]));
    fonts.families.insert(
        bold(),
        chain(&[&bold_f, &ko_bold, &regular, &ko, &sym, &emoji]),
    );
    fonts
}

// 웹과 같은 글꼴: KoPubWorld 돋움(설치돼 있으면) → Segoe UI → 맑은 고딕, 코드는 Consolas
#[cfg_attr(not(windows), allow(dead_code))] // 설치 마법사(Windows 전용)와 테스트에서만 쓴다
pub fn install_fonts(ctx: &egui::Context) {
    install_fonts_with(ctx, "auto");
}

pub fn install_fonts_with(ctx: &egui::Context, choice: &str) {
    let key = if choice.is_empty() { "auto" } else { choice };
    if key == "auto" {
        static AUTO_FONTS: std::sync::OnceLock<egui::FontDefinitions> = std::sync::OnceLock::new();
        let fonts = AUTO_FONTS.get_or_init(|| build_fonts("auto")).clone();
        ctx.set_fonts(fonts);
    } else {
        ctx.set_fonts(build_fonts(key));
    }
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
    SAVE_AS,
    r#"<path d="M9.5 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v2"/><polyline points="7 3 7 8 15 8"/><polyline points="7 21 7 14 13 14"/><path d="M12.5 22l.8-2.9 5.9-5.9a1.5 1.5 0 0 1 2.12 2.12l-5.9 5.9Z"/>"#
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
icon!(MOON, r#"<path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/>"#);
icon!(
    MAXIMIZE,
    r#"<polyline points="15 3 21 3 21 9"/><polyline points="9 21 3 21 3 15"/><line x1="21" y1="3" x2="14" y2="10"/><line x1="3" y1="21" x2="10" y2="14"/>"#
);
icon!(
    WIDTH,
    r#"<polyline points="18 8 22 12 18 16"/><polyline points="6 8 2 12 6 16"/><line x1="2" y1="12" x2="22" y2="12"/>"#
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
    SCISSORS,
    r#"<circle cx="6" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><line x1="20" y1="4" x2="8.12" y2="15.88"/><line x1="14.47" y1="14.48" x2="20" y2="20"/><line x1="8.12" y1="8.12" x2="12" y2="12"/>"#
);
icon!(
    CLIPBOARD,
    r#"<path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/><rect x="8" y="2" width="8" height="4" rx="1" ry="1"/>"#
);
icon!(
    BRUSH,
    r#"<path d="m9.06 11.9 8.07-8.06a2.85 2.85 0 1 1 4.03 4.03l-8.06 8.08"/><path d="M7.07 14.94c-1.66 0-3 1.35-3 3.02 0 1.33-2.5 1.52-2 2.02 1.08 1.1 2.49 2.02 4 2.02 2.2 0 4-1.8 4-4.04a3.01 3.01 0 0 0-3-3.02z"/>"#
);
icon!(
    UNDO,
    r#"<path d="M3 7v6h6"/><path d="M21 17a9 9 0 0 0-9-9 9 9 0 0 0-6 2.3L3 13"/>"#
);
icon!(
    REDO,
    r#"<path d="M21 7v6h-6"/><path d="M3 17a9 9 0 0 1 9-9 9 9 0 0 1 6 2.3l3 2.7"/>"#
);
icon!(
    CHECK_SQUARE,
    r#"<polyline points="9 11 12 14 22 4"/><path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11"/>"#
);
icon!(
    ARROW_UP,
    r#"<line x1="12" y1="19" x2="12" y2="5"/><polyline points="5 12 12 5 19 12"/>"#
);
icon!(
    ARROW_DOWN,
    r#"<line x1="12" y1="5" x2="12" y2="19"/><polyline points="19 12 12 19 5 12"/>"#
);
icon!(
    PENCIL,
    r#"<path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z"/><path d="m15 5 4 4"/>"#
);
icon!(
    TABLE,
    r#"<rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><line x1="3" y1="9" x2="21" y2="9"/><line x1="3" y1="15" x2="21" y2="15"/><line x1="12" y1="3" x2="12" y2="21"/>"#
);
icon!(MINUS, r#"<line x1="5" y1="12" x2="19" y2="12"/>"#);
icon!(
    X,
    r#"<line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>"#
);
icon!(
    SPELL,
    r#"<path d="m6 16 6-12 6 12"/><path d="M8 12h8"/><path d="m16 20 2 2 4-4"/>"#
);
icon!(
    TOC,
    r#"<rect x="3" y="3" width="18" height="18" rx="2"/><line x1="9" y1="3" x2="9" y2="21"/><line x1="13" y1="8" x2="18" y2="8"/><line x1="13" y1="12" x2="18" y2="12"/><line x1="13" y1="16" x2="16" y2="16"/>"#
);
icon!(
    BULLET_LIST,
    r#"<circle cx="4.5" cy="6" r="1.9" fill="white" stroke="none"/><circle cx="4.5" cy="12" r="1.9" fill="white" stroke="none"/><circle cx="4.5" cy="18" r="1.9" fill="white" stroke="none"/><line x1="10" y1="6" x2="21" y2="6"/><line x1="10" y1="12" x2="21" y2="12"/><line x1="10" y1="18" x2="21" y2="18"/>"#
);
icon!(
    NUMBERED_LIST,
    r#"<line x1="10" y1="6" x2="21" y2="6"/><line x1="10" y1="12" x2="21" y2="12"/><line x1="10" y1="18" x2="21" y2="18"/><path d="M4 6h1v4"/><path d="M4 10h2"/><path d="M6 18H4c0-1 2-2 2-3s-1-1.5-2-1"/>"#
);

pub fn img(icon: Icon, size: f32, color: Color32) -> egui::Image<'static> {
    egui::Image::from_bytes(icon.0, icon.1.as_bytes())
        .fit_to_exact_size(egui::vec2(size, size))
        .tint(color)
}
