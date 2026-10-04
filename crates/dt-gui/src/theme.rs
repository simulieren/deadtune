//! DeadTune's look: a warm dark theme with one amber accent, applied to every screen.

use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle,
    Theme, Visuals,
};

pub const RAIL: Color32 = Color32::from_rgb(14, 15, 18);
pub const BG: Color32 = Color32::from_rgb(21, 23, 27);
pub const CARD: Color32 = Color32::from_rgb(29, 32, 38);
pub const CARD_HOVER: Color32 = Color32::from_rgb(35, 39, 46);
pub const BORDER: Color32 = Color32::from_rgb(43, 47, 55);
pub const TEXT: Color32 = Color32::from_rgb(236, 232, 225);
pub const WEAK: Color32 = Color32::from_rgb(144, 149, 160);
pub const ACCENT: Color32 = Color32::from_rgb(240, 179, 65);
pub const ON_ACCENT: Color32 = Color32::from_rgb(26, 20, 6);
pub const GOOD: Color32 = Color32::from_rgb(95, 203, 140);
pub const WARN: Color32 = Color32::from_rgb(242, 163, 58);
pub const BAD: Color32 = Color32::from_rgb(239, 107, 107);

pub const RADIUS: u8 = 8;

pub fn title() -> TextStyle {
    TextStyle::Name("title".into())
}

/// Inter SemiBold, for headings, page titles and the selected rail entry.
pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

/// Inter (subset by `scripts/subset-inter.sh`) ahead of egui's defaults, which stay as
/// fallbacks for scripts and emoji the subset lacks; Hack stays the monospace font.
/// Only our own subsets ship (eframe's `default_fonts` is off): Inter for UI text, Hack for
/// monospace, each falling back to the other.
pub fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::empty();
    for (name, bytes) in [
        (
            "Inter-Regular",
            &include_bytes!("../assets/fonts/Inter-Regular.ttf")[..],
        ),
        (
            "Inter-SemiBold",
            &include_bytes!("../assets/fonts/Inter-SemiBold.ttf")[..],
        ),
        (
            "Hack-Regular",
            &include_bytes!("../assets/fonts/Hack-Regular.ttf")[..],
        ),
    ] {
        fonts
            .font_data
            .insert(name.into(), Arc::new(FontData::from_static(bytes)));
    }
    for (family, list) in [
        (
            FontFamily::Proportional,
            &["Inter-Regular", "Hack-Regular"][..],
        ),
        (
            semibold(),
            &["Inter-SemiBold", "Inter-Regular", "Hack-Regular"],
        ),
        (FontFamily::Monospace, &["Hack-Regular", "Inter-Regular"]),
    ] {
        fonts
            .families
            .insert(family, list.iter().map(|n| n.to_string()).collect());
    }
    fonts
}

pub fn install(ctx: &egui::Context) {
    ctx.set_theme(Theme::Dark);
    ctx.set_fonts(fonts());
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (
                TextStyle::Small,
                FontId::new(11.0, FontFamily::Proportional),
            ),
            (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(13.0, FontFamily::Proportional),
            ),
            (TextStyle::Heading, FontId::new(15.5, semibold())),
            (
                TextStyle::Monospace,
                FontId::new(12.5, FontFamily::Monospace),
            ),
            (title(), FontId::new(20.0, semibold())),
        ]
        .into();
        let s = &mut style.spacing;
        s.item_spacing = egui::vec2(6.0, 6.0);
        s.button_padding = egui::vec2(10.0, 4.0);
        s.interact_size.y = 24.0;
        s.slider_rail_height = 5.0;
        s.combo_height = 300.0;
        style.visuals = visuals();
    });
}

fn visuals() -> Visuals {
    let mut v = Visuals::dark();
    v.override_text_color = None;
    v.panel_fill = BG;
    v.window_fill = CARD;
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.extreme_bg_color = RAIL;
    v.faint_bg_color = CARD_HOVER;
    v.code_bg_color = RAIL;
    v.hyperlink_color = ACCENT;
    v.warn_fg_color = WARN;
    v.error_fg_color = BAD;
    v.window_corner_radius = CornerRadius::same(RADIUS);
    v.menu_corner_radius = CornerRadius::same(RADIUS);
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0, ON_ACCENT);
    v.slider_trailing_fill = true;
    let radius = CornerRadius::same(4);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = CARD;
    w.noninteractive.weak_bg_fill = CARD;
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.inactive.bg_fill = Color32::from_rgb(46, 50, 58);
    w.inactive.weak_bg_fill = Color32::from_rgb(40, 44, 51);
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.hovered.bg_fill = Color32::from_rgb(58, 63, 73);
    w.hovered.weak_bg_fill = Color32::from_rgb(52, 57, 66);
    w.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(78, 84, 96));
    w.hovered.fg_stroke = Stroke::new(1.5, TEXT);
    w.hovered.expansion = 0.0;
    w.active.bg_fill = ACCENT;
    w.active.weak_bg_fill = Color32::from_rgb(62, 67, 78);
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(1.5, TEXT);
    w.active.expansion = 0.0;
    w.open.weak_bg_fill = Color32::from_rgb(46, 50, 58);
    for widget in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        widget.corner_radius = radius;
    }
    v
}

pub fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(RADIUS))
        .inner_margin(egui::Margin::symmetric(14, 12))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use super::*;

    fn non_ascii_chars(dir: &Path, out: &mut BTreeSet<char>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                non_ascii_chars(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                out.extend(text.chars().filter(|c| !c.is_ascii()));
            }
        }
    }

    /// The Inter subset is cut to a codepoint list, and a dropped glyph silently falls back
    /// to another font or draws as a box (an ellipsis once did). Every non-ASCII char in
    /// the GUI and core sources (where the labels live) must be in the primary font itself;
    /// egui's `has_glyphs` can't tell, since any fallback face satisfies it.
    #[test]
    fn every_glyph_renders_in_inter() {
        use skrifa::MetadataProvider;
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut chars = BTreeSet::new();
        non_ascii_chars(&root.join("src"), &mut chars);
        non_ascii_chars(&root.join("../dt-core/src"), &mut chars);
        assert!(chars.contains(&'·'), "source scan found nothing");
        let defs = fonts();
        for family in [FontFamily::Proportional, semibold()] {
            let name = &defs.families[&family][0];
            assert!(name.starts_with("Inter-"), "{family:?} starts with {name}");
            let font = skrifa::FontRef::new(&defs.font_data[name].font).unwrap();
            let charmap = font.charmap();
            for c in chars.iter().copied().chain('A'..='z') {
                assert!(
                    charmap.map(c).is_some(),
                    "{c:?} (U+{:04X}) is missing from {name}; add it to scripts/subset-inter.sh",
                    c as u32
                );
            }
        }
    }
}
