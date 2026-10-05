//! "Apples & tunnels" card on the Minimap page (`hud::apples_tunnels`). Edits are made on
//! a copy of `profile.hud.apples_tunnels` and handed back whole; they go out with the
//! normal Apply. The dots draw on the page's one minimap preview (`overlay`).

use std::ops::RangeInclusive;

use dt_core::hud::apples_tunnels::{
    APPLE_RADIUS_RANGE, APPLES, ApplesTunnels, DOT_SIZE_RANGE, DotIcon, Dots, GAME_BUILD, MapPoint,
    RADIUS_RANGE, TUNNEL_ENTRANCES, TunnelHero,
};
use dt_core::hud::art::Art;
use dt_core::hud::minimap_colors::Color;
use eframe::egui::color_picker::{Alpha, color_edit_button_srgba};
use eframe::egui::{
    self, Align, Color32, CornerRadius, Layout, Painter, Pos2, Rect, RichText, Sense, Shape,
    Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
};

use crate::hud_art::Images;
use crate::minimap_view::marked;
use crate::state::AppState;
use crate::theme::{ACCENT, BORDER, RAIL, TEXT, WARN, WEAK};
use crate::widgets;

/// Left edge of a kind's options, under its name past the switch.
const INDENT: f32 = 38.0;
const LABEL: f32 = 92.0;
/// Picture icons read from about this size on the minimap.
const PICTURE_MIN_PX: u8 = 10;
const SHADE: Color32 = Color32::from_black_alpha(176);

/// The card; `Some` with the whole new setting when it was edited.
pub fn card(ui: &mut Ui, state: &AppState, images: &mut Images) -> Option<ApplesTunnels> {
    let current = state.profile.hud.apples_tunnels;
    let mut next = current;
    widgets::card(ui, |ui| {
        header(ui, &current, &mut next);
        ui.add_space(6.0);
        apples(ui, images, &current, &mut next);
        ui.add_space(8.0);
        tunnels(ui, images, &current, &mut next);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if widgets::switch(ui, current.clear_switching).clicked() {
                next.clear_switching = !current.clear_switching;
            }
            ui.label(marked("Clear tunnel switching", current.clear_switching));
        });
        ui.horizontal(|ui| {
            ui.add_space(INDENT);
            note(
                ui,
                "In tunnel view the surface map fades to a faint outline and the tunnel map \
                 brightens.",
            );
        });
        ui.add_space(6.0);
        widgets::hint(
            ui,
            &format!(
                "Positions are from game build {GAME_BUILD}. If a map update moves them, the \
                 dots drift until DeadTune is updated."
            ),
        );
    });
    (next != current).then_some(next)
}

fn header(ui: &mut Ui, current: &ApplesTunnels, next: &mut ApplesTunnels) {
    ui.horizontal(|ui| {
        widgets::caption(ui, "Apples & tunnels");
        widgets::badge(ui, "Experimental, untested in game", WARN);
        let n = current.changed_count();
        ui.add_space(8.0);
        ui.label(
            RichText::new(format!("Changed: {n}"))
                .small()
                .color(if n > 0 { ACCENT } else { WEAK }),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add_enabled(
                    !current.is_vanilla(),
                    egui::Button::new("Reset apples and tunnels"),
                )
                .on_hover_text("Back to the game's own minimap")
                .clicked()
            {
                *next = ApplesTunnels::default();
            }
        });
    });
    widgets::hint(
        ui,
        "Marks on the minimap where healing apples spawn, and the tunnel entrances near you. \
         Idea by FesamAyt; rebuilt by DeadTune from your game files.",
    );
}

fn apples(ui: &mut Ui, images: &mut Images, current: &ApplesTunnels, next: &mut ApplesTunnels) {
    kind_title(
        ui,
        images,
        "Apple spots",
        "Where apples spawn, not whether one is there now. Every hero sees them.",
        current.apples,
        &mut next.apples,
    );
    ui.add_enabled_ui(current.apples.on, |ui| {
        look_rows(ui, images, current.apples, &mut next.apples);
        row(ui, "Show", |ui| {
            let picked = widgets::segmented(
                ui,
                &["Whole map", "Near you"],
                usize::from(current.apples_near),
            );
            if let Some(i) = picked {
                next.apples_near = i == 1;
            }
        });
        if current.apples_near {
            row(ui, "Within", |ui| {
                next.apple_radius_pct =
                    radius(ui, current.apple_radius_pct, APPLE_RADIUS_RANGE, 20);
            });
        }
        row(ui, "", |ui| {
            if widgets::switch(ui, current.apples_in_tunnels).clicked() {
                next.apples_in_tunnels = !current.apples_in_tunnels;
            }
            ui.label(marked("Also in tunnel view", !current.apples_in_tunnels));
        });
    });
}

fn tunnels(ui: &mut Ui, images: &mut Images, current: &ApplesTunnels, next: &mut ApplesTunnels) {
    let heroes = TunnelHero::ALL.map(TunnelHero::name).join(", ");
    kind_title(
        ui,
        images,
        "Tunnel entrances",
        &format!("Only when you play {heroes}; only near you, hidden inside the tunnels."),
        current.tunnels,
        &mut next.tunnels,
    );
    ui.add_enabled_ui(current.tunnels.on, |ui| {
        look_rows(ui, images, current.tunnels, &mut next.tunnels);
        row(ui, "Within", |ui| {
            next.tunnel_radius_pct = radius(ui, current.tunnel_radius_pct, RADIUS_RANGE, 11);
        });
    });
}

/// The switch, the kind's icon and name, and its note.
fn kind_title(
    ui: &mut Ui,
    images: &mut Images,
    label: &str,
    note_text: &str,
    current: Dots,
    next: &mut Dots,
) {
    ui.horizontal(|ui| {
        if widgets::switch(ui, current.on).clicked() {
            next.on = !current.on;
        }
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
        let sample = Dots {
            size_px: 12,
            ..current
        };
        paint_icon(ui.painter(), images, rect.center(), sample, 1.0);
        ui.label(marked(label, current.on));
    });
    ui.horizontal(|ui| {
        ui.add_space(INDENT);
        note(ui, note_text);
    });
}

/// Icon, then size, colour and outline.
fn look_rows(ui: &mut Ui, images: &mut Images, current: Dots, next: &mut Dots) {
    row(ui, "Icon", |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for icon in DotIcon::ALL {
            if icon_chip(ui, images, current, icon).clicked() && icon != current.icon {
                next.icon = icon;
                if icon.image().is_some() {
                    next.size_px = next.size_px.max(PICTURE_MIN_PX);
                }
            }
        }
    });
    row(ui, "Size", |ui| {
        let mut size = current.size_px;
        ui.spacing_mut().slider_width = 120.0;
        ui.add(egui::Slider::new(&mut size, DOT_SIZE_RANGE).show_value(false));
        ui.label(RichText::new(format!("{size} px")).size(12.5).color(TEXT));
        if size != current.size_px {
            next.size_px = size;
        }
        ui.add_space(14.0);
        ui.label(RichText::new("Colour").color(WEAK));
        ui.spacing_mut().interact_size = vec2(36.0, 18.0);
        let before = to_color32(current.color);
        let mut picked = before;
        color_edit_button_srgba(ui, &mut picked, Alpha::OnlyBlend);
        if picked != before {
            let [r, g, b, a] = picked.to_srgba_unmultiplied();
            next.color = Color([r, g, b, a]);
        }
        ui.add_space(14.0);
        if widgets::switch(ui, current.outline).clicked() {
            next.outline = !current.outline;
        }
        ui.label(RichText::new("Dark outline").color(WEAK))
            .on_hover_text("A thin dark edge so the mark reads on light parts of the map.");
    });
}

/// One option row: a weak label in a fixed column, then the controls.
fn row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.add_space(INDENT);
        ui.allocate_ui_with_layout(
            vec2(LABEL, 22.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.set_min_width(LABEL);
                ui.label(RichText::new(label).color(WEAK));
            },
        );
        add(ui);
    });
}

/// A radius slider and its "N% of the map" readout, highlighted away from `default`.
fn radius(ui: &mut Ui, value: u8, range: RangeInclusive<u8>, default: u8) -> u8 {
    let mut r = value;
    ui.spacing_mut().slider_width = 160.0;
    ui.add(egui::Slider::new(&mut r, range).show_value(false));
    ui.label(marked(&format!("{r}% of the map"), r != default));
    r
}

fn note(ui: &mut Ui, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).size(11.5).color(WEAK)).wrap());
}

fn icon_chip(ui: &mut Ui, images: &mut Images, current: Dots, icon: DotIcon) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
    let stroke = if icon == current.icon {
        Stroke::new(1.5, ACCENT)
    } else if response.hovered() {
        Stroke::new(1.0, TEXT)
    } else {
        Stroke::new(1.0, BORDER)
    };
    let p = ui.painter();
    p.rect(
        rect,
        CornerRadius::same(4),
        RAIL,
        stroke,
        StrokeKind::Inside,
    );
    let sample = Dots {
        icon,
        size_px: 14,
        ..current
    };
    paint_icon(p, images, rect.center(), sample, 1.0);
    response.on_hover_text(icon.label())
}

fn to_color32(c: Color) -> Color32 {
    let [r, g, b, a] = c.0;
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// One mark as the addon's CSS draws it, `dots.size_px` game pixels at `k` points each.
fn paint_icon(p: &Painter, images: &mut Images, at: Pos2, dots: Dots, k: f32) {
    let r = (f32::from(dots.size_px) * k / 2.0).max(1.5);
    let color = to_color32(dots.color);
    let edge = if dots.outline {
        Stroke::new(1.0, SHADE)
    } else {
        Stroke::NONE
    };
    match dots.icon {
        DotIcon::Dot => {
            p.circle(at, r, color, edge);
        }
        DotIcon::Ring => {
            let width = (f32::from((dots.size_px / 4).max(1)) * k).max(1.0);
            if dots.outline {
                p.circle_stroke(at, r - width / 2.0, Stroke::new(width + 2.0, SHADE));
            }
            p.circle_stroke(at, r - width / 2.0, Stroke::new(width, color));
        }
        DotIcon::Diamond => {
            let h = r * 0.72 * std::f32::consts::SQRT_2;
            let points = vec![
                at + vec2(0.0, -h),
                at + vec2(h, 0.0),
                at + vec2(0.0, h),
                at + vec2(-h, 0.0),
            ];
            p.add(Shape::convex_polygon(points, color, edge));
        }
        DotIcon::Cross | DotIcon::Heart | DotIcon::Stairs => {
            let art = Art {
                path: dots.icon.image().expect("picture icon"),
                size: [64, 64],
            };
            let rect = Rect::from_center_size(at, Vec2::splat(r * 2.0));
            if dots.outline {
                images.paint(p, art, rect.expand(1.0), SHADE);
            }
            if !images.paint(p, art, rect, color) {
                p.circle(at, r, color, edge);
            }
        }
    }
}

/// The marks over the map in `map` (the game map's square), with the hero at `hero`.
/// Sizes are game pixels on the game's 360 px map; a faint ring shows each range.
pub fn overlay(p: &Painter, images: &mut Images, map: Rect, style: &ApplesTunnels, hero: Pos2) {
    let k = map.width() / 360.0;
    let at = |m: &MapPoint| -> Pos2 {
        pos2(
            map.left() + map.width() * m.u as f32,
            map.top() + map.height() * m.v as f32,
        )
    };
    let reach = |pct: u8| map.width() * f32::from(pct) / 100.0;
    let ring = |pct: u8| {
        p.circle_stroke(
            hero,
            reach(pct),
            Stroke::new(1.0, Color32::from_white_alpha(60)),
        );
    };
    if style.apples.on {
        if style.apples_near {
            ring(style.apple_radius_pct);
        }
        for a in APPLES.iter() {
            if !style.apples_near || at(a).distance(hero) <= reach(style.apple_radius_pct) {
                paint_icon(p, images, at(a), style.apples, k);
            }
        }
    }
    if style.tunnels.on {
        ring(style.tunnel_radius_pct);
        for e in TUNNEL_ENTRANCES.iter() {
            if at(e).distance(hero) <= reach(style.tunnel_radius_pct) {
                paint_icon(p, images, at(e), style.tunnels, k);
            }
        }
    }
}
