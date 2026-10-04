//! The simple view's look: a dark, in-game-menu palette with one amber accent, plus the few
//! custom widgets it needs (segmented levels, arrow selector, FPS meter).
//!
//! Dark, because players run DeadTune next to a dark game and a bright window beside it
//! stings. The accent is reserved for "this is yours": the selected level, changed rows, Apply.

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Response, Sense, Shape, Stroke,
    TextStyle, Ui, Vec2, vec2,
};

pub const ACCENT: Color32 = Color32::from_rgb(244, 168, 50);
pub const ACCENT_INK: Color32 = Color32::from_rgb(24, 16, 4);
pub const BG: Color32 = Color32::from_rgb(13, 15, 19);
pub const PANEL: Color32 = Color32::from_rgb(19, 22, 28);
pub const RAISED: Color32 = Color32::from_rgb(30, 34, 43);
pub const HOVER: Color32 = Color32::from_rgb(40, 45, 57);
pub const LINE: Color32 = Color32::from_rgb(44, 49, 60);
pub const TEXT: Color32 = Color32::from_rgb(230, 233, 239);
pub const DIM: Color32 = Color32::from_rgb(138, 146, 162);
pub const OK: Color32 = Color32::from_rgb(98, 200, 130);
pub const WARN: Color32 = Color32::from_rgb(244, 168, 50);
pub const BAD: Color32 = Color32::from_rgb(235, 98, 98);

const RADIUS: u8 = 3;

pub fn apply(style: &mut egui::Style) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.window_stroke = Stroke::new(1.0, LINE);
    v.extreme_bg_color = BG;
    v.faint_bg_color = PANEL;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = ACCENT.gamma_multiply(0.45);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT;
    v.window_corner_radius = CornerRadius::same(6);
    v.menu_corner_radius = CornerRadius::same(RADIUS);
    let w = &mut v.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    for (state, fill) in [
        (&mut w.inactive, RAISED),
        (&mut w.hovered, HOVER),
        (&mut w.active, HOVER),
        (&mut w.open, HOVER),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0, LINE);
        state.fg_stroke = Stroke::new(1.0, TEXT);
        state.corner_radius = CornerRadius::same(RADIUS);
    }
    w.hovered.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.7));
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.noninteractive.corner_radius = CornerRadius::same(RADIUS);
    style.visuals = v;
    style.spacing.item_spacing = vec2(10.0, 8.0);
    style.spacing.button_padding = vec2(14.0, 7.0);
    style.spacing.interact_size.y = 30.0;
    style.spacing.slider_rail_height = 6.0;
    style.spacing.menu_margin = 8.0.into();
    let font = |size| FontId::new(size, FontFamily::Proportional);
    style.text_styles.insert(TextStyle::Body, font(15.0));
    style.text_styles.insert(TextStyle::Button, font(15.0));
    style.text_styles.insert(TextStyle::Small, font(12.5));
    style.text_styles.insert(TextStyle::Heading, font(22.0));
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(14.0, FontFamily::Monospace),
    );
}

pub fn frame(fill: Color32, margin: impl Into<egui::Margin>) -> egui::Frame {
    egui::Frame::new().fill(fill).inner_margin(margin)
}

fn text_width(ui: &Ui, text: &str, font: &FontId) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_string(), font.clone(), TEXT)
        .size()
        .x
}

/// Buttons joined into one bar; the selected one is filled with the accent.
/// Returns the clicked index. `None` as `selected` shows no choice (unknown value).
pub fn segmented(ui: &mut Ui, options: &[&str], selected: Option<usize>) -> Option<usize> {
    let font = FontId::new(15.0, FontFamily::Proportional);
    let height = 32.0;
    let mut clicked = None;
    ui.spacing_mut().item_spacing.x = 2.0;
    ui.horizontal(|ui| {
        for (i, label) in options.iter().enumerate() {
            let width = text_width(ui, label, &font) + 28.0;
            let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
            let on = selected == Some(i);
            let fill = if on {
                ACCENT
            } else if response.hovered() {
                HOVER
            } else {
                RAISED
            };
            let ink = if on {
                ACCENT_INK
            } else if response.hovered() {
                TEXT
            } else {
                DIM
            };
            let radius = CornerRadius::same(RADIUS);
            ui.painter().rect_filled(rect, radius, fill);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                font.clone(),
                ink,
            );
            if response.clicked() {
                clicked = Some(i);
            }
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, *label)
            });
        }
    });
    clicked
}

pub fn segmented_width(ui: &Ui, options: &[&str]) -> f32 {
    let font = FontId::new(15.0, FontFamily::Proportional);
    options
        .iter()
        .map(|o| text_width(ui, o, &font) + 28.0 + 2.0)
        .sum()
}

fn triangle(ui: &Ui, rect: Rect, dir: f32, color: Color32) {
    let c = rect.center();
    let (w, h) = (5.0, 7.0);
    let pts = vec![
        Pos2::new(c.x + dir * w, c.y),
        Pos2::new(c.x - dir * w, c.y - h),
        Pos2::new(c.x - dir * w, c.y + h),
    ];
    ui.painter()
        .add(Shape::convex_polygon(pts, color, Stroke::NONE));
}

pub fn arrow_button(ui: &mut Ui, dir: f32, enabled: bool, size: Vec2) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let fill = if enabled && response.hovered() {
        HOVER
    } else {
        RAISED
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(RADIUS), fill);
    let ink = if !enabled {
        LINE
    } else if response.hovered() {
        ACCENT
    } else {
        DIM
    };
    triangle(ui, rect, dir, ink);
    response
}

/// `<  Medium  >` selector. Returns the new index when an arrow is clicked.
pub fn arrow_select(
    ui: &mut Ui,
    current: &str,
    index: Option<usize>,
    len: usize,
    width: f32,
) -> Option<usize> {
    let mut next = None;
    ui.spacing_mut().item_spacing.x = 2.0;
    ui.horizontal(|ui| {
        let i = index.unwrap_or(0);
        if arrow_button(ui, -1.0, i > 0, vec2(32.0, 32.0)).clicked() && i > 0 {
            next = Some(i - 1);
        }
        let (rect, _) = ui.allocate_exact_size(vec2(width - 68.0, 32.0), Sense::hover());
        ui.painter()
            .rect_filled(rect, CornerRadius::same(RADIUS), RAISED);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            current,
            FontId::new(15.0, FontFamily::Proportional),
            TEXT,
        );
        if arrow_button(ui, 1.0, i + 1 < len, vec2(32.0, 32.0)).clicked() && i + 1 < len {
            next = Some(i + 1);
        }
    });
    next
}

/// Three bars, filled according to how much a setting costs in FPS.
pub fn fps_meter(ui: &mut Ui, filled: usize) {
    let (rect, _) = ui.allocate_exact_size(vec2(44.0, 14.0), Sense::hover());
    for i in 0..3 {
        let x = rect.left() + i as f32 * 16.0;
        let bar = Rect::from_min_size(Pos2::new(x, rect.top()), vec2(12.0, 14.0));
        let color = if i < filled { ACCENT } else { LINE };
        ui.painter().rect_filled(bar, CornerRadius::same(2), color);
    }
}

pub fn primary_button(ui: &mut Ui, enabled: bool, text: &str) -> Response {
    let button = egui::Button::new(
        egui::RichText::new(text)
            .size(17.0)
            .strong()
            .color(if enabled { ACCENT_INK } else { DIM }),
    )
    .fill(if enabled { ACCENT } else { RAISED })
    .stroke(Stroke::NONE)
    .min_size(vec2(150.0, 42.0));
    ui.add_enabled(enabled, button)
}

pub fn ghost_button(ui: &mut Ui, text: &str) -> Response {
    ui.add(
        egui::Button::new(egui::RichText::new(text).color(DIM))
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::new(1.0, LINE)),
    )
}

pub fn pill(ui: &mut Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.18))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.6)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(9, 2))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).size(12.5).strong().color(color));
        });
}
