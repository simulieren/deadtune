//! One dark style for every screen: a gaming utility that sits next to a dark game.

use eframe::egui::{self, Color32, CornerRadius, FontId, Stroke, TextStyle, Visuals};

pub const ACCENT: Color32 = Color32::from_rgb(242, 170, 60);
pub const GOOD: Color32 = Color32::from_rgb(96, 200, 120);
pub const WARN: Color32 = Color32::from_rgb(235, 190, 70);
pub const BAD: Color32 = Color32::from_rgb(235, 105, 100);

pub const BG: Color32 = Color32::from_rgb(18, 20, 24);
pub const BAR: Color32 = Color32::from_rgb(24, 27, 32);
pub const CARD: Color32 = Color32::from_rgb(30, 33, 40);
pub const CARD_HOVER: Color32 = Color32::from_rgb(40, 44, 53);
pub const LINE: Color32 = Color32::from_rgb(52, 57, 68);
pub const TEXT: Color32 = Color32::from_rgb(222, 225, 232);
pub const DIM: Color32 = Color32::from_rgb(140, 146, 160);

pub const RADIUS: u8 = 6;

pub fn install(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(12.0)),
            (TextStyle::Body, FontId::proportional(15.0)),
            (TextStyle::Button, FontId::proportional(15.0)),
            (TextStyle::Heading, FontId::proportional(22.0)),
            (TextStyle::Monospace, FontId::monospace(13.5)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(12.0, 5.0);
        style.spacing.interact_size = egui::vec2(40.0, 26.0);
        style.spacing.slider_width = 240.0;
        style.spacing.indent = 14.0;
        style.spacing.combo_width = 220.0;
        style.visuals = visuals();
    });
}

fn visuals() -> Visuals {
    let mut v = Visuals::dark();
    let radius = CornerRadius::same(RADIUS);
    v.panel_fill = BG;
    v.window_fill = CARD;
    v.window_stroke = Stroke::new(1.0, LINE);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = radius;
    v.extreme_bg_color = Color32::from_rgb(12, 13, 16);
    v.faint_bg_color = Color32::from_rgb(26, 29, 35);
    v.code_bg_color = Color32::from_rgb(40, 44, 53);
    v.hyperlink_color = ACCENT;
    v.warn_fg_color = WARN;
    v.error_fg_color = BAD;
    v.override_text_color = None;
    v.weak_text_alpha = 0.55;
    v.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;
    v.collapsing_header_frame = false;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = BAR;
    w.noninteractive.weak_bg_fill = BAR;
    w.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.noninteractive.corner_radius = radius;
    w.inactive.bg_fill = Color32::from_rgb(46, 50, 60);
    w.inactive.weak_bg_fill = Color32::from_rgb(40, 44, 53);
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.inactive.corner_radius = radius;
    w.hovered.bg_fill = Color32::from_rgb(60, 65, 78);
    w.hovered.weak_bg_fill = Color32::from_rgb(54, 59, 71);
    w.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(90, 96, 112));
    w.hovered.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    w.hovered.corner_radius = radius;
    w.active.bg_fill = Color32::from_rgb(70, 76, 92);
    w.active.weak_bg_fill = Color32::from_rgb(70, 76, 92);
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(2.0, Color32::WHITE);
    w.active.corner_radius = radius;
    w.open = w.hovered;
    v
}
