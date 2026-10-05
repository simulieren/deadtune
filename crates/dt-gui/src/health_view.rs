//! Health bar page: style presets, the controls behind them and a preview of the health
//! number at full, hurt and low health, drawn from the game's bar, frame and backer when
//! they load. Edits go out with the HUD addon on Apply.

use dt_core::hud::art;
use dt_core::hud::health_style::{HealthPreset, HealthStyle, NUMBER_SCALE_RANGE};
use eframe::egui::epaint::{Mesh, TextShape, Vertex};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Layout, Painter, Pos2, Rect, RichText,
    Sense, Shape, Stroke, Ui, emath::Rot2, pos2, vec2,
};

use crate::hud_art::{self, Images};
use crate::minimap_view::{marked, percent_slider};
use crate::state::AppState;
use crate::theme::{self, ACCENT, RAIL, WARN, WEAK};
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

/// Upright drawing coordinates (game px, origin at the bar's top-left) placed on screen:
/// scaled, then turned by `angle` (radians, clockwise on screen) about `origin`.
#[derive(Clone, Copy)]
struct Place {
    origin: Pos2,
    scale: f32,
    angle: f32,
}

impl Place {
    fn at(self, local: Pos2) -> Pos2 {
        self.origin + Rot2::from_angle(self.angle) * (local.to_vec2() * self.scale)
    }

    fn turned(self, degrees: f32) -> Self {
        Self {
            angle: self.angle + degrees.to_radians(),
            ..self
        }
    }

    fn polygon(self, shape: &[Pos2], fill: Color32) -> Shape {
        Shape::convex_polygon(
            shape.iter().map(|&at| self.at(at)).collect(),
            fill,
            Stroke::NONE,
        )
    }
}

/// `#health_bar`, 66x212.
const BAR: Rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(66.0, 212.0));
/// `#health_bar_frame`, 68x220, from the bar's top-left less 1 px.
const FRAME_RECT: Rect = Rect::from_min_max(pos2(-1.0, -1.0), pos2(67.0, 219.0));

const fn on_mask(x: f32, y: f32) -> Pos2 {
    pos2(x * 66.0 / 99.0, y * 212.0 / 330.0)
}

const fn on_frame(x: f32, y: f32) -> Pos2 {
    pos2(x * 68.0 / 105.0 - 1.0, y * 220.0 / 340.0 - 1.0)
}

/// `healthbar_backer_mask` (a 99x330 drawing): the ruler, wide at the top, straight on
/// the left, slanting in on the right.
const RULER: [Pos2; 4] = [
    on_mask(1.29, 0.95),
    on_mask(98.8, 0.27),
    on_mask(25.7, 329.5),
    on_mask(0.82, 324.1),
];
/// `healthbar_frame_with_regen` (a 105x340 drawing): its outline, and its hole cut into
/// two convex parts at the regen shelf's bottom, for drawing the frame without the picture.
const FRAME_OUTLINE: [Pos2; 4] = [
    on_frame(0.0, 0.81),
    on_frame(104.96, 0.0),
    on_frame(31.18, 339.19),
    on_frame(0.34, 332.49),
];
const FRAME_HOLE: [[Pos2; 4]; 2] = [
    [
        on_frame(17.79, 35.0),
        on_frame(90.9, 35.0),
        on_frame(28.82, 331.19),
        on_frame(7.15, 327.11),
    ],
    [
        on_frame(83.19, 7.61),
        on_frame(97.01, 6.1),
        on_frame(90.9, 35.0),
        on_frame(69.5, 35.0),
    ],
];
/// `.bars_container { transform: rotateZ(-20deg) }`.
const BAR_TILT: f32 = -20.0;
/// The number block leans about 10 degrees in the in-game screenshot.
const NUMBER_TILT: f32 = -10.0;
/// The block's extent around the bar's top-left (game px, on screen after the tilts) at
/// 100% number size; a bigger number reaches further left over the bar.
const BLOCK: Rect = Rect::from_min_max(pos2(-2.0, -25.0), pos2(146.0, 207.0));
const MAX_HEALTH: u32 = 4557;
const BODY: Color32 = Color32::from_rgba_unmultiplied_const(0x33, 0x33, 0x33, 0xEA);
/// `healthbar_fill_texture_png`'s average colour, for the fill without the picture.
const PAPER: Color32 = Color32::from_rgb(0xDF, 0xD1, 0xBC);
const OFF_BLACK: Color32 = Color32::from_rgb(0x10, 0x13, 0x0D);
const FRAME_LOW: Color32 = Color32::from_rgb(0xCC, 0x34, 0x0A);

/// The x range of convex `shape` on the line `y`, if the line crosses it.
fn span(shape: &[Pos2], y: f32) -> Option<(f32, f32)> {
    let mut range: Option<(f32, f32)> = None;
    for (i, &a) in shape.iter().enumerate() {
        let b = shape[(i + 1) % shape.len()];
        if (a.y - y) * (b.y - y) > 0.0 || a.y == b.y {
            continue;
        }
        let x = a.x + (b.x - a.x) * (y - a.y) / (b.y - a.y);
        range = Some(range.map_or((x, x), |(l, r)| (l.min(x), r.max(x))));
    }
    range
}

/// `#healthLines` as the screenshot shows them: a tick every 250 health up from the
/// bottom, full width every 1000 and 30% wide between. Heights are fractions of the bar.
fn ticks(max: u32) -> Vec<(f32, bool)> {
    (250..max)
        .step_by(250)
        .map(|hp| (hp as f32 / max as f32, hp % 1000 == 0))
        .collect()
}

fn corners(rect: Rect) -> [Pos2; 4] {
    [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ]
}

/// `Images::paint_shape` with a rotation: paints `art` laid over the upright `image`, only
/// inside the upright convex `shape`, both put on screen by `place`.
fn paint_placed(
    p: &Painter,
    images: &mut Images,
    art: art::Art,
    place: Place,
    image: Rect,
    shape: &[Pos2],
    tint: Color32,
) -> bool {
    if shape.len() < 3 {
        return false;
    }
    let Some(texture) = images.get(art, image.size().max_elem() * place.scale) else {
        return false;
    };
    let mut mesh = Mesh::with_texture(texture.id());
    for &at in shape {
        mesh.vertices.push(Vertex {
            pos: place.at(at),
            uv: pos2(
                (at.x - image.left()) / image.width(),
                (at.y - image.top()) / image.height(),
            ),
            color: tint,
        });
    }
    for i in 1..shape.len() as u32 - 1 {
        mesh.add_triangle(0, i, i + 1);
    }
    p.add(mesh);
    true
}

/// Text put on screen by `place`, its `align` point at the upright `anchor`, ringed with
/// an offBlack `outline` (screen px, none at 0) the way the game's number is.
#[allow(clippy::too_many_arguments)]
fn placed_text(
    p: &Painter,
    place: Place,
    anchor: Pos2,
    align: Align2,
    text: &str,
    font: FontId,
    color: Color32,
    outline: f32,
) {
    let galley = p.layout_no_wrap(text.to_owned(), font, color);
    let corner = place.at(align.anchor_size(anchor, galley.size() / place.scale).min);
    if outline > 0.0 {
        for step in 0..8 {
            let offset =
                Rot2::from_angle(step as f32 * std::f32::consts::FRAC_PI_4) * vec2(outline, 0.0);
            p.add(
                TextShape::new(corner + offset, galley.clone(), OFF_BLACK)
                    .with_override_text_color(OFF_BLACK)
                    .with_angle(place.angle),
            );
        }
    }
    p.add(TextShape::new(corner, galley, color).with_angle(place.angle));
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
        let area = Rect::from_min_max(rect.min + vec2(0.0, 6.0), rect.max - vec2(0.0, 24.0));
        let number_scale = f32::from(style.number_scale_pct) / 100.0;
        let mut block = BLOCK;
        block.min.x = block.min.x.min(116.0 - 82.0 * number_scale);
        let scale = (column * 0.94 / block.width()).min(area.height() / block.height());
        let mut real = false;
        for (i, (label, fill, color, low)) in states.into_iter().enumerate() {
            let x = rect.left() + column * (i as f32 + 0.5);
            let upright = Place {
                origin: pos2(x, area.center().y) - block.center().to_vec2() * scale,
                scale,
                angle: 0.0,
            };
            let bar = upright.turned(BAR_TILT);
            let (fill_tint, paper) = if low {
                (LOW, LOW)
            } else {
                (Color32::WHITE, PAPER)
            };
            let frame_tint = if low { FRAME_LOW } else { FRAME };
            let framed = images
                .get(art::HEALTH_FRAME, FRAME_RECT.height() * scale)
                .is_some();
            let shown: &[[Pos2; 4]] = if framed {
                std::slice::from_ref(&RULER)
            } else {
                painter.add(bar.polygon(&FRAME_OUTLINE, frame_tint));
                &FRAME_HOLE
            };
            let top = BAR.bottom() - BAR.height() * fill;
            for part in shown {
                painter.add(bar.polygon(part, BODY));
                let level = hud_art::cut_top(part, top);
                if paint_placed(
                    painter,
                    images,
                    art::HEALTH_FILL,
                    bar,
                    BAR,
                    &level,
                    fill_tint,
                ) {
                    real = true;
                } else if level.len() >= 3 {
                    painter.add(bar.polygon(&level, paper));
                }
            }
            let tick = Stroke::new(3.0 * scale, OFF_BLACK.gamma_multiply(0.7));
            for (height, large) in ticks(MAX_HEALTH) {
                let y = BAR.bottom() - BAR.height() * height;
                let reach = BAR.left() + BAR.width() * if large { 1.0 } else { 0.3 };
                for part in shown {
                    if let Some((left, right)) = span(part, y)
                        && right.min(reach) > left
                    {
                        let ends = [bar.at(pos2(left, y)), bar.at(pos2(right.min(reach), y))];
                        painter.line_segment(ends, tick);
                    }
                }
            }
            if framed {
                paint_placed(
                    painter,
                    images,
                    art::HEALTH_FRAME,
                    bar,
                    FRAME_RECT,
                    &corners(FRAME_RECT),
                    frame_tint,
                );
            }
            if !style.hide_regen {
                let arrows = Rect::from_center_size(pos2(13.0, 9.5), vec2(7.0, 8.0));
                let arrows_tint = HEALTHY.gamma_multiply(0.3);
                paint_placed(
                    painter,
                    images,
                    art::REGEN,
                    bar,
                    arrows,
                    &corners(arrows),
                    arrows_tint,
                );
                placed_text(
                    painter,
                    bar,
                    pos2(19.0, 9.0),
                    Align2::LEFT_CENTER,
                    "14.8",
                    FontId::new(10.0 * scale, theme::semibold()),
                    HEALTHY,
                    0.0,
                );
            }
            let shake = if low && !style.no_shake { 1.5 } else { 0.0 };
            let number = Place {
                origin: upright.origin + vec2(shake, 0.0),
                ..upright
            }
            .turned(NUMBER_TILT);
            if !style.hide_backer {
                let backer = Rect::from_min_size(pos2(52.0, 36.0), vec2(78.0, 62.4));
                let drawn = paint_placed(
                    painter,
                    images,
                    art::HEALTH_BACKER,
                    number,
                    backer,
                    &corners(backer),
                    BACKER,
                );
                if !drawn {
                    let at = |x: f32, y: f32| {
                        pos2(
                            backer.left() + backer.width() * x / 260.0,
                            backer.top() + backer.height() * y / 208.0,
                        )
                    };
                    let shape = [
                        at(13.6, 98.0),
                        at(239.1, 13.6),
                        at(259.1, 166.0),
                        at(3.6, 195.0),
                    ];
                    painter.add(number.polygon(&shape, BACKER));
                }
            }
            let digits = if low { 36.0 } else { 32.0 };
            let size = digits * scale * number_scale;
            placed_text(
                painter,
                number,
                pos2(116.0, 63.0),
                Align2::RIGHT_CENTER,
                &((fill * MAX_HEALTH as f32) as u32).to_string(),
                FontId::new(size, theme::semibold()),
                color,
                (1.2 * scale).max(1.0),
            );
            placed_text(
                painter,
                number.turned(-3.0),
                pos2(113.0, 82.0),
                Align2::RIGHT_CENTER,
                &format!("/ {MAX_HEALTH}"),
                FontId::new(14.0 * scale, theme::semibold()),
                HEALTHY.gamma_multiply(if style.clear_max_health { 0.85 } else { 0.2 }),
                0.0,
            );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_reads_the_ruler_width_at_a_height() {
        let (left, right) = span(&RULER, 106.0).unwrap();
        assert!((left - 0.7).abs() < 0.2, "{left}");
        assert!((right - 41.5).abs() < 0.5, "{right}");
        assert_eq!(span(&RULER, -5.0), None);
        assert_eq!(span(&RULER, 230.0), None);
    }

    #[test]
    fn ticks_every_250_and_full_width_every_1000() {
        let ticks = ticks(4557);
        assert_eq!(ticks.len(), 18);
        let large: Vec<f32> = ticks.iter().filter(|t| t.1).map(|t| t.0).collect();
        assert_eq!(large.len(), 4);
        assert!((large[0] - 1000.0 / 4557.0).abs() < 1e-6);
        assert!(ticks.iter().all(|t| t.0 > 0.0 && t.0 < 1.0));
    }

    #[test]
    fn the_frame_hole_parts_sit_inside_the_frame() {
        for part in FRAME_HOLE {
            for at in part {
                let (left, right) = span(&FRAME_OUTLINE, at.y).unwrap();
                assert!(at.x >= left && at.x <= right, "{at:?}");
            }
        }
    }
}
