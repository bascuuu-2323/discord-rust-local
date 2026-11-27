use eframe::egui::{self, Color32, FontId, Frame, Margin, RichText, Rounding, Stroke};
use serde::Deserialize;

pub const CHAT: Color32 = Color32::from_rgb(48, 49, 54);
pub const SIDEBAR: Color32 = Color32::from_rgb(40, 42, 47);
pub const RAIL: Color32 = Color32::from_rgb(18, 18, 20);
pub const INPUT: Color32 = Color32::from_rgb(59, 62, 68);
pub const BORDER: Color32 = Color32::from_rgb(71, 74, 82);
pub const TEXT: Color32 = Color32::from_rgb(237, 237, 242);
pub const MUTED: Color32 = Color32::from_rgb(176, 176, 190);
pub const ACCENT: Color32 = Color32::from_rgb(92, 101, 225);

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "SCREAMING_SNAKE_CASE")]
pub struct Profile {
    background_color: String,
    window_bounds: Option<Bounds>,
    is_maximized: bool,
}

#[derive(Deserialize)]
struct Bounds {
    width: f32,
    height: f32,
}

impl Profile {
    pub fn load() -> Self {
        std::env::var_os("APPDATA")
            .and_then(|dir| {
                std::fs::read(std::path::Path::new(&dir).join("discord/settings.json")).ok()
            })
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_default()
    }

    pub fn background(&self) -> Color32 {
        self.background_color
            .strip_prefix('#')
            .filter(|hex| hex.len() == 6)
            .and_then(|hex| u32::from_str_radix(hex, 16).ok())
            .map(|rgb| Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8))
            .unwrap_or(RAIL)
    }

    pub fn viewport(&self) -> egui::ViewportBuilder {
        let size = self.window_bounds.as_ref().map_or([1180., 720.], |b| {
            [b.width.clamp(560., 1600.), b.height.clamp(420., 970.)]
        });
        let viewport = egui::ViewportBuilder::default()
            .with_inner_size(size)
            .with_min_inner_size([560., 420.])
            .with_maximized(self.is_maximized);
        match asset("window.rgba") {
            Some(rgba) if rgba.len() == 64 * 64 * 4 => viewport.with_icon(egui::IconData {
                rgba,
                width: 64,
                height: 64,
            }),
            _ => viewport,
        }
    }
}

fn asset(name: &str) -> Option<Vec<u8>> {
    let directory = std::env::var_os("DISCORD_LOCAL_ASSETS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets"));
    std::fs::read(directory.join(name)).ok()
}

pub fn apply_local_fonts(ctx: &egui::Context) {
    let (Some(normal), Some(medium)) = (asset("ggsans-400.ttf"), asset("ggsans-500.ttf")) else {
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    let heading = egui::FontFamily::Name("GG Sans Medium".into());
    fonts
        .font_data
        .insert("GG Sans".into(), egui::FontData::from_owned(normal));
    fonts
        .font_data
        .insert("GG Sans Medium".into(), egui::FontData::from_owned(medium));
    let family = fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default();
    family.insert(0, "GG Sans".into());
    let mut medium_family = family.clone();
    medium_family.insert(0, "GG Sans Medium".into());
    fonts.families.insert(heading.clone(), medium_family);
    ctx.set_fonts(fonts);
    ctx.style_mut(|style| {
        if let Some(font) = style.text_styles.get_mut(&egui::TextStyle::Heading) {
            font.family = heading;
        }
    });
}

pub fn surface(fill: Color32, x: f32, y: f32) -> Frame {
    Frame::none()
        .fill(fill)
        .inner_margin(Margin::symmetric(x, y))
}

pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.override_text_color = Some(TEXT);
    v.panel_fill = CHAT;
    v.window_fill = SIDEBAR;
    v.faint_bg_color = INPUT;
    v.extreme_bg_color = INPUT;
    v.selection.bg_fill = ACCENT;
    for (widget, fill) in [
        (&mut v.widgets.inactive, INPUT),
        (&mut v.widgets.hovered, BORDER),
        (&mut v.widgets.active, ACCENT),
        (&mut v.widgets.open, INPUT),
    ] {
        widget.bg_fill = fill;
        widget.weak_bg_fill = fill;
        widget.bg_stroke = Stroke::NONE;
        widget.fg_stroke = Stroke::new(1_f32, TEXT);
        widget.rounding = Rounding::same(6.);
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1_f32, BORDER);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1_f32, MUTED);
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(16.));
        s.text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(15.));
        s.spacing.item_spacing = [8., 6.].into();
    });
}

pub fn avatar(ui: &mut egui::Ui, name: &str, id: &str, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(size), egui::Sense::hover());
    let colors = [
        ACCENT,
        Color32::from_rgb(43, 118, 110),
        Color32::from_rgb(162, 73, 94),
    ];
    let index = id
        .bytes()
        .fold(0usize, |sum, byte| (sum + usize::from(byte)) % colors.len());
    ui.painter()
        .circle_filled(rect.center(), size / 2., colors[index]);
    let initial = name
        .chars()
        .next()
        .map_or_else(|| "?".to_owned(), |c| c.to_uppercase().collect());
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        initial,
        FontId::proportional(size * 0.47),
        TEXT,
    );
}

pub fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text).color(MUTED)
}
