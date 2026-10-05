//! Health bar page: style presets, the controls behind them and a preview of the health
//! number at full, hurt and low health, drawn from the game's bar, frame and backer when
//! they load. Edits go out with the HUD addon on Apply.

use dt_core::hud::art;
use dt_core::hud::health_style::{HealthPreset, HealthStyle, NUMBER_SCALE_RANGE};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Layout, Rect, RichText, Sense, Stroke, Ui,
    pos2, vec2,
};

use crate::hud_art::Images;
use crate::minimap_view::{marked, percent_slider};
use crate::state::AppState;
use crate::theme::{self, ACCENT, BORDER, RAIL, TEXT, WARN, WEAK};
use crate::widgets;

/// Vanilla's number colours: off-white, and `#FF5656` at low health.
const HEALTHY: Color32 = Color32::from_rgb(0xFF, 0xEF, 0xD7);
const LOW: Color32 = Color32::from_rgb(0xFF, 0x56, 0x56);
const HURT: Color32 = Color32::from_rgb(0xFF, 0xB3, 0x47);
/// `vivaciousGreen`, the backer's wash, and the frame's `#142304`.
const BACKER: Color32 = Color32::from_rgb(0x2E, 0x6B, 0x3A);
const FRAME: Color32 = Color32::from_rgb(0x14, 0x23, 0x04);
const PREVIEW_WIDTH: f32 = 300.0;

pub fn page(ui: &mut Ui, state: &mut AppState) {
    crate::hud_view::hud_error(ui, state);
    crate::hud_view::show_layout_note(ui, state, dt_core::hud::elements::ElementId::HealthAndAmmo);
    let mut style = state.profile.hud.health.clone();
    let ctx = ui.ctx().clone();
    crate::hud_art::with(&ctx, state, |_, images| {
        if ui.available_width() >= 760.0 {
            ui.horizontal_top(|ui| {
                let gap = 12.0;
                let left = ui.available_width() - PREVIEW_WIDTH - gap;
                ui.vertical(|ui| {
                    ui.set_width(left);
                    controls(ui, &mut style);
                });
                ui.add_space(gap - ui.spacing().item_spacing.x);
                ui.vertical(|ui| {
                    ui.set_width(PREVIEW_WIDTH);
                    preview(ui, &style, images);
                });
            });
        } else {
            preview(ui, &style, images);
            controls(ui, &mut style);
        }
    });
    credits(ui);
    if style != state.profile.hud.health {
        state.set_health_style(style);
    }
}

fn controls(ui: &mut Ui, style: &mut HealthStyle) {
    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            widgets::caption(ui, "Style");
            widgets::badge(ui, "Experimental, untested in game", WARN);
        });
        widgets::hint(
            ui,
            "Goes into the HUD addon on Apply, next to your HUD layout.",
        );
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            let current = style.preset();
            for preset in HealthPreset::ALL {
                if ui
                    .selectable_label(current == Some(preset), preset.label())
                    .on_hover_text(preset.blurb())
                    .clicked()
                {
                    *style = preset.style();
                }
            }
            ui.add_space(6.0);
            let n = style.changed_count();
            ui.label(
                RichText::new(format!("Changed: {n}"))
                    .small()
                    .color(if n > 0 { ACCENT } else { WEAK }),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(n > 0, egui::Button::new("Reset health bar"))
                    .on_hover_text("Back to the game's own health bar")
                    .clicked()
                {
                    *style = HealthStyle::default();
                }
            });
        });
    });
    widgets::section(ui, "Health number", |ui| {
        if let Some(v) = percent_slider(
            ui,
            "Size",
            style.number_scale_pct,
            NUMBER_SCALE_RANGE,
            10,
            ui.available_width().min(420.0),
        ) {
            style.number_scale_pct = v;
        }
        toggle(
            ui,
            &mut style.color_by_health,
            "Colour by health",
            "Turns orange when you're hurt. The game already turns it red when you're low.",
        );
        toggle(
            ui,
            &mut style.clear_max_health,
            "Easy-to-read max health",
            "The game shows your max health at 20% opacity; this makes it clear.",
        );
    });
    widgets::section(ui, "Bar", |ui| {
        toggle(
            ui,
            &mut style.hide_backer,
            "Hide the green backer",
            "The shape behind the health number.",
        );
        toggle(
            ui,
            &mut style.no_shake,
            "No shaking at low health",
            "Stops the health bar and HUD from shaking and pulsing when you're hurt.",
        );
        toggle(
            ui,
            &mut style.hide_regen,
            "Hide health regen",
            "The small regeneration number beside the bar.",
        );
    });
}

fn toggle(ui: &mut Ui, value: &mut bool, label: &str, help: &str) {
    ui.horizontal(|ui| {
        if widgets::switch(ui, *value).clicked() {
            *value = !*value;
        }
        ui.vertical(|ui| {
            ui.label(marked(label, *value));
            ui.label(RichText::new(help).size(11.0).color(WEAK));
        });
    });
    ui.add_space(4.0);
}

/// The health block at three health levels with the style applied: the game's bar parts
/// where they load, painted shapes where they don't.
fn preview(ui: &mut Ui, style: &HealthStyle, images: &mut Images) {
    widgets::card(ui, |ui| {
        widgets::caption(ui, "Preview");
        let height = 190.0;
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(6), RAIL);
        let states: [(&str, f32, Color32, bool); 3] = [
            ("Full", 1.0, HEALTHY, false),
            (
                "Hurt",
                0.55,
                if style.color_by_health { HURT } else { HEALTHY },
                false,
            ),
            ("Low", 0.2, LOW, true),
        ];
        let column = rect.width() / 3.0;
        let mut real = false;
        for (i, (label, fill, color, low)) in states.into_iter().enumerate() {
            let x = rect.left() + column * (i as f32 + 0.5);
            let bar =
                Rect::from_center_size(pos2(x - 22.0, rect.center().y - 6.0), vec2(16.0, 110.0));
            let filled = Rect::from_min_max(
                pos2(bar.left(), bar.bottom() - bar.height() * fill),
                bar.max,
            );
            let fill_tint = if low { LOW } else { Color32::WHITE };
            let parts = images
                .get(art::HEALTH_FILL, bar.height())
                .zip(images.get(art::HEALTH_FRAME, bar.height()));
            if let Some((texture, _)) = parts {
                real = true;
                painter.rect_filled(
                    bar,
                    CornerRadius::same(3),
                    Color32::from_rgba_unmultiplied(0x33, 0x33, 0x33, 0xEA),
                );
                let uv = Rect::from_min_max(pos2(0.0, 1.0 - fill), pos2(1.0, 1.0));
                painter.image(texture.id(), filled, uv, fill_tint);
                let [w, h] = art::HEALTH_FRAME.size.map(f32::from);
                let [bw, bh] = art::HEALTH_FILL.size.map(f32::from);
                let frame = Rect::from_center_size(
                    bar.center(),
                    vec2(bar.width() * w / bw, bar.height() * h / bh),
                );
                images.paint(painter, art::HEALTH_FRAME, frame, FRAME);
            } else {
                painter.rect(
                    bar,
                    CornerRadius::same(3),
                    Color32::from_gray(45),
                    Stroke::new(1.0, BORDER),
                    egui::StrokeKind::Inside,
                );
                painter.rect_filled(
                    filled.shrink(2.0),
                    CornerRadius::same(2),
                    if low { LOW } else { HEALTHY },
                );
            }
            let scale = f32::from(style.number_scale_pct) / 100.0;
            let size = if low { 15.0 } else { 13.0 } * scale;
            let number = ((fill * 700.0) as u32).to_string();
            let anchor = pos2(bar.right() + 8.0, bar.bottom() - 14.0);
            if !style.hide_backer {
                let backer = Rect::from_min_size(
                    pos2(anchor.x - 2.0, anchor.y - size * 0.8),
                    vec2(size * 2.2, size * 1.25),
                );
                if !images.paint(painter, art::HEALTH_BACKER, backer, BACKER) {
                    painter.rect_filled(backer, CornerRadius::same(3), BACKER.gamma_multiply(0.7));
                }
            }
            let shake = if low && !style.no_shake { 1.5 } else { 0.0 };
            let galley = painter.text(
                anchor + vec2(shake, 0.0),
                Align2::LEFT_CENTER,
                &number,
                FontId::new(size, theme::semibold()),
                color,
            );
            painter.text(
                pos2(galley.left(), galley.bottom() + 6.0),
                Align2::LEFT_CENTER,
                "/ 700",
                FontId::proportional(8.0 * scale.max(1.0)),
                TEXT.gamma_multiply(if style.clear_max_health { 0.85 } else { 0.2 }),
            );
            if !style.hide_regen {
                let regen = pos2(bar.center().x, bar.top() - 9.0);
                let arrows = Rect::from_center_size(regen - vec2(9.0, 0.0), vec2(7.0, 8.0));
                images.paint(painter, art::REGEN, arrows, TEXT.gamma_multiply(0.3));
                painter.text(
                    regen,
                    Align2::CENTER_CENTER,
                    "+4",
                    FontId::proportional(8.5),
                    TEXT.gamma_multiply(0.8),
                );
            }
            painter.text(
                pos2(x, rect.bottom() - 12.0),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(11.0),
                WEAK,
            );
        }
        widgets::hint(
            ui,
            if real {
                "Your game's health bar parts with the settings above."
            } else {
                "A sketch of the settings above, not the game's own art."
            },
        );
        ui.add_space(6.0);
        crate::game_shot::in_game(
            ui,
            dt_core::hud::elements::ElementId::HealthAndAmmo,
            vec2(ui.available_width(), 160.0),
        );
    });
}

fn credits(ui: &mut Ui) {
    ui.add_space(4.0);
    ui.label(
        RichText::new(
            "Ideas from bytenode's Minimal Healthbar Redux (original concept by Gerimboca) and \
             budhud-style Alternate Health Bar (idea by .Kaiz). DeadTune writes its own rules \
             from your game files. A health percentage, a smooth colour fade and a horizontal \
             bar come later.",
        )
        .size(11.5)
        .color(WEAK),
    );
}
