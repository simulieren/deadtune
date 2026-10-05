//! "Apples & tunnels" card on the Minimap page (`hud::apples_tunnels`). Edits are made on
//! a copy of `profile.hud.apples_tunnels` and handed back whole; they go out with the
//! normal Apply.

use dt_core::hud::apples_tunnels::{
    APPLES, ApplesTunnels, DOT_SIZE_RANGE, Dots, GAME_BUILD, RADIUS_RANGE, TUNNEL_ENTRANCES,
    TunnelHero,
};
use dt_core::hud::minimap_colors::Color;
use eframe::egui::color_picker::{Alpha, color_edit_button_srgba};
use eframe::egui::{
    self, Align, Color32, Layout, Painter, Pos2, Rect, RichText, Sense, Stroke, Ui, pos2, vec2,
};

use crate::hud_art::Images;
use crate::minimap_view::marked;
use crate::state::AppState;
use crate::theme::{ACCENT, TEXT, WARN, WEAK};
use crate::widgets;

const PREVIEW: f32 = 170.0;
/// Where the preview puts the stand-in hero, as map fractions.
const SAMPLE_HERO: [f64; 2] = [0.5, 0.52];

/// The card; `Some` with the whole new setting when it was edited.
pub fn card(ui: &mut Ui, state: &AppState, images: &mut Images) -> Option<ApplesTunnels> {
    let current = state.profile.hud.apples_tunnels;
    let mut next = current;
    widgets::card(ui, |ui| {
        header(ui, &current, &mut next);
        let wide = ui.available_width() >= 640.0;
        if wide {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width() - PREVIEW - 24.0);
                    controls(ui, &current, &mut next);
                });
                ui.add_space(12.0);
                preview(ui, &current, images);
            });
        } else {
            controls(ui, &current, &mut next);
        }
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
        "Small dots on the minimap where healing apples spawn, and the tunnel entrances near you. \
         Idea by FesamAyt; rebuilt by DeadTune from your game files.",
    );
}

fn controls(ui: &mut Ui, current: &ApplesTunnels, next: &mut ApplesTunnels) {
    ui.add_space(4.0);
    dots_row(
        ui,
        "Apple spots",
        "Every hero, tunnel view too. Shows where apples spawn, not whether one is there now.",
        current.apples,
        &mut next.apples,
    );
    let heroes = TunnelHero::ALL.map(TunnelHero::name).join(", ");
    dots_row(
        ui,
        "Tunnel entrances",
        &format!("Only when you play {heroes}; only near you, hidden inside the tunnels."),
        current.tunnels,
        &mut next.tunnels,
    );
    ui.add_enabled_ui(current.tunnels.on, |ui| {
        ui.horizontal(|ui| {
            ui.add_space(46.0);
            ui.label(marked(
                "Show entrances within",
                current.tunnel_radius_pct != ApplesTunnels::default().tunnel_radius_pct,
            ));
            let mut r = current.tunnel_radius_pct;
            ui.spacing_mut().slider_width = 140.0;
            ui.add(egui::Slider::new(&mut r, RADIUS_RANGE).show_value(false));
            ui.label(
                RichText::new(format!("{r}% of the map"))
                    .size(12.5)
                    .color(TEXT),
            );
            next.tunnel_radius_pct = r;
        });
    });
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        if widgets::switch(ui, current.clear_switching).clicked() {
            next.clear_switching = !current.clear_switching;
        }
        ui.label(marked("Clear tunnel switching", current.clear_switching))
            .on_hover_text("Fades the surface map to a faint outline and brightens the tunnel map while you are in the tunnels.");
    });
    ui.add_space(2.0);
    widgets::hint(
        ui,
        &format!(
            "Positions are from game build {GAME_BUILD}. If a map update moves them, the dots \
             drift until DeadTune is updated."
        ),
    );
}

fn dots_row(ui: &mut Ui, label: &str, note: &str, current: Dots, next: &mut Dots) {
    ui.horizontal(|ui| {
        if widgets::switch(ui, current.on).clicked() {
            next.on = !current.on;
        }
        ui.label(marked(label, current.on)).on_hover_text(note);
        ui.add_enabled_ui(current.on, |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().interact_size = vec2(36.0, 18.0);
                let before = to_color32(current.color);
                let mut picked = before;
                color_edit_button_srgba(ui, &mut picked, Alpha::OnlyBlend);
                if picked != before {
                    let [r, g, b, a] = picked.to_srgba_unmultiplied();
                    next.color = Color([r, g, b, a]);
                }
                ui.add_space(6.0);
                let mut size = current.size_px;
                ui.label(RichText::new(format!("{size} px")).size(12.5).color(TEXT));
                ui.spacing_mut().slider_width = 110.0;
                ui.add(egui::Slider::new(&mut size, DOT_SIZE_RANGE).show_value(false));
                ui.label(RichText::new("Size").color(WEAK));
                next.size_px = size;
            });
        });
    });
    ui.horizontal(|ui| {
        ui.add_space(46.0);
        ui.add(egui::Label::new(RichText::new(note).size(11.5).color(WEAK)).wrap());
    });
}

fn to_color32(c: Color) -> Color32 {
    let [r, g, b, a] = c.0;
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

fn preview(ui: &mut Ui, style: &ApplesTunnels, images: &mut Images) {
    ui.vertical(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(PREVIEW, PREVIEW), Sense::hover());
        let real = paint(&ui.painter().with_clip_rect(rect), rect, style, images);
        ui.label(
            RichText::new(if real {
                "Your game's map, hero in the middle"
            } else {
                "Mock-up, hero in the middle"
            })
            .size(11.0)
            .color(WEAK),
        );
    });
}

/// The dots over the game's map, framed as on the Minimap preview, or over a sketch of
/// one; true when the map is the game's.
fn paint(p: &Painter, rect: Rect, style: &ApplesTunnels, images: &mut Images) -> bool {
    let map = rect.shrink(rect.width() * 20.0 / 400.0);
    let real = crate::minimap_view::base(p, rect, map, images, Color32::WHITE, true);
    let at = |u: f64, v: f64| -> Pos2 {
        pos2(
            map.left() + map.width() * u as f32,
            map.top() + map.height() * v as f32,
        )
    };
    let dot = |pos: Pos2, dots: Dots| {
        let radius = (f32::from(dots.size_px) * 0.35).max(1.5);
        p.circle(
            pos,
            radius,
            to_color32(dots.color),
            Stroke::new(1.0, Color32::from_black_alpha(176)),
        );
    };
    if style.apples.on {
        for a in &APPLES {
            dot(at(a.u, a.v), style.apples);
        }
    }
    let hero = at(SAMPLE_HERO[0], SAMPLE_HERO[1]);
    if style.tunnels.on {
        let show = f64::from(style.tunnel_radius_pct) / 100.0;
        p.circle_stroke(
            hero,
            map.width() * show as f32,
            Stroke::new(1.0, Color32::from_white_alpha(40)),
        );
        for e in &TUNNEL_ENTRANCES {
            let (du, dv) = (e.u - SAMPLE_HERO[0], e.v - SAMPLE_HERO[1]);
            if du * du + dv * dv <= show * show {
                dot(at(e.u, e.v), style.tunnels);
            }
        }
    }
    p.circle_filled(hero, 4.5, Color32::from_rgb(0x4D, 0xA3, 0xFF));
    p.circle_stroke(hero, 4.5, Stroke::new(1.0, Color32::WHITE));
    real
}
