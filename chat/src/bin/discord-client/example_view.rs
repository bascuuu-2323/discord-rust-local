use super::appearance::{ACCENT, BORDER, INPUT, SIDEBAR, avatar, muted, surface};
use eframe::egui::{self, Color32, RichText, Stroke};

pub const COMMAND: &str = "/test";
const RESPONSE_BG: Color32 = Color32::from_rgb(54, 57, 77);

pub fn show_suggestion(ui: &mut egui::Ui) -> bool {
    let mut chosen = false;
    surface(SIDEBAR, 10., 7.)
        .rounding(8.)
        .stroke(Stroke::new(1_f32, BORDER))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(muted("COMANDOS").size(11.).strong());
            surface(INPUT, 7., 3.).rounding(5.).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    chosen = ui.button(RichText::new(COMMAND).strong()).clicked();
                    ui.label(muted("Aplicación de ejemplo · Envía un saludo"));
                });
            });
        });
    chosen
}

pub fn welcome(ui: &mut egui::Ui, channel: &str) {
    let (badge, _) = ui.allocate_exact_size(egui::Vec2::splat(48.), egui::Sense::hover());
    ui.painter().circle_filled(badge.center(), 24., INPUT);
    ui.painter().text(
        badge.center(),
        egui::Align2::CENTER_CENTER,
        "#",
        egui::FontId::proportional(32.),
        Color32::WHITE,
    );
    ui.label(
        RichText::new(format!("Te damos la bienvenida a #{channel}"))
            .strong()
            .size(23.),
    );
    ui.label(muted(format!("Este es el inicio del canal #{channel}.")));
    ui.add_space(12.);
}

pub fn show_reply(ui: &mut egui::Ui, name: &str, reply: &str) -> bool {
    let mut dismiss = false;
    surface(RESPONSE_BG, 12., 8.).rounding(6.).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal_top(|ui| {
            avatar(ui, "E", "0", 38.);
            ui.vertical(|ui| {
                ui.label(muted(format!("{name} usó {COMMAND}")).size(12.));
                ui.horizontal_wrapped(|ui| {
                    ui.strong("Aplicación de ejemplo");
                    surface(ACCENT, 4., 1.).rounding(3.).show(ui, |ui| {
                        ui.label(RichText::new("APP").size(10.).strong());
                    });
                });
                ui.label(reply);
                ui.horizontal_wrapped(|ui| {
                    ui.label(muted("Solo tú puedes ver esto").size(12.));
                    dismiss = ui.link("Descartar").clicked();
                });
            });
        });
    });
    dismiss
}
