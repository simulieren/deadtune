//! Minimap page (simple-view section, advanced HUD tab): the game's own enemy colour
//! ConVars, the experimental marker sizes and map style (`hud::minimap_style`), the
//! experimental per-icon CSS colours (`hud::minimap_colors`), and a painted minimap mock.

use std::ops::RangeInclusive;

use dt_core::hud::art;
use dt_core::hud::minimap_colors::{self, Color, IconId};
use dt_core::hud::minimap_style::{
    MAP_OPACITY_RANGE, MARKER_SCALE_RANGE, MARKERS, MarkerGroup, MarkerSpec,
};
use eframe::egui::color_picker::{Alpha, color_edit_button_srgba};
use eframe::egui::{
    self, Align, Color32, CornerRadius, Layout, Painter, Pos2, Rect, RichText, Sense, Shape,
    Stroke, Ui, Vec2, pos2, vec2,
};

use crate::hud_art::{Images, arc, cut_top};
use crate::state::{AppState, CUSTOM_UI_COLORS, ENEMY_UI_COLOR, MinimapPreset};
use crate::theme::{ACCENT, BORDER, RAIL, TEXT, WARN, WEAK};
use crate::widgets;

const PREVIEW: f32 = 230.0;
const GROUPS: [(&str, &[IconId]); 3] = [
    (
        "Heroes",
        &[
            IconId::EnemyHero,
            IconId::AllyHero,
            IconId::LocalHero,
            IconId::EnemyHeroArrow,
            IconId::AllyHeroArrow,
        ],
    ),
    (
        "Objectives",
        &[
            IconId::EnemyObjective,
            IconId::AllyObjective,
            IconId::MidBoss,
            IconId::EnemyUrnReturn,
            IconId::AllyUrnReturn,
            IconId::UrnSpawn,
            IconId::CarriedUrn,
        ],
    ),
    (
        "Neutrals and pickups",
        &[
            IconId::SmallCamp,
            IconId::MediumCamp,
            IconId::LargeCamp,
            IconId::Vault,
            IconId::WeaponPowerup,
            IconId::SoulsPowerup,
            IconId::HealthPowerup,
            IconId::SpiritPowerup,
            IconId::MovementPowerup,
            IconId::PowerupSpawn,
            IconId::UnsecuredSouls,
            IconId::RejuvCrystal,
            IconId::Shop,
            IconId::Broker,
            IconId::Teleporter,
            IconId::Stairs,
        ],
    ),
];
/// Lane lines and allied guardians, left to right; the guardians' colour gives way to one the
/// user sets for every allied objective.
const LANES: [Color32; 3] = [
    Color32::from_rgb(0x6B, 0xB2, 0x47),
    Color32::from_rgb(0x2E, 0xC7, 0xE6),
    Color32::from_rgb(0xFF, 0xDF, 0x40),
];

enum Action {
    Icon(IconId, Option<Color>),
    Preset(Option<MinimapPreset>),
    CustomColors(bool),
    Enemy([u8; 3]),
    ResetEnemy,
    MarkerScale(MarkerGroup, u16),
    MapOpacity(u8),
    Minimal(bool),
    ResetStyle,
    Apples(dt_core::hud::apples_tunnels::ApplesTunnels),
}

pub fn page(ui: &mut Ui, state: &mut AppState) {
    crate::hud_view::hud_error(ui, state);
    crate::hud_view::show_layout_note(ui, state, dt_core::hud::elements::ElementId::Minimap);
    let mut actions = Vec::new();
    let ctx = ui.ctx().clone();
    crate::hud_art::with(&ctx, state, |state, images| {
        if ui.available_width() >= 760.0 {
            ui.horizontal_top(|ui| {
                let gap = 12.0;
                let left = ui.available_width() - PREVIEW - 28.0 - gap;
                ui.vertical(|ui| {
                    ui.set_width(left);
                    settings(ui, state, images, &mut actions);
                });
                ui.add_space(gap - ui.spacing().item_spacing.x);
                ui.vertical(|ui| {
                    ui.set_width(PREVIEW + 28.0);
                    preview_card(ui, state, images);
                });
            });
        } else {
            preview_card(ui, state, images);
            settings(ui, state, images, &mut actions);
        }
    });
    for action in actions {
        match action {
            Action::Icon(id, Some(color)) => state.set_minimap_color(id, color),
            Action::Icon(id, None) => state.reset_minimap_color(id),
            Action::Preset(p) => state.apply_minimap_preset(p),
            Action::CustomColors(on) => state.set_custom_ui_colors(on),
            Action::Enemy(rgb) => state.set_enemy_ui_color(rgb),
            Action::ResetEnemy => {
                state.reset_convars(ENEMY_UI_COLOR.iter().copied().chain([CUSTOM_UI_COLORS]))
            }
            Action::MarkerScale(group, pct) => state.set_marker_scale(group, pct),
            Action::MapOpacity(pct) => state.set_map_opacity(pct),
            Action::Minimal(on) => state.set_minimal_minimap(on),
            Action::ResetStyle => state.reset_minimap_style(),
            Action::Apples(next) => state.set_apples_tunnels(next),
        }
    }
}

fn settings(ui: &mut Ui, state: &AppState, images: &mut Images, actions: &mut Vec<Action>) {
    official(ui, state, actions);
    map_style(ui, state, actions);
    if let Some(next) = crate::apples_view::card(ui, state, images) {
        actions.push(Action::Apples(next));
    }
    icons(ui, state, actions);
}

fn official(ui: &mut Ui, state: &AppState, actions: &mut Vec<Action>) {
    widgets::section(ui, "Game colour settings (official)", |ui| {
        widgets::hint(
            ui,
            "The game's own Enemy UI Color setting. Applies instantly with the key bind. \
             Ranked-safe mode resets it to the game's default while it is on.",
        );
        ui.add_space(4.0);
        let on = state.custom_ui_colors();
        let toggle_changed = state.is_changed(CUSTOM_UI_COLORS);
        let color_changed = ENEMY_UI_COLOR.iter().any(|n| state.is_changed(n));
        ui.horizontal(|ui| {
            if widgets::switch(ui, on).clicked() {
                actions.push(Action::CustomColors(!on));
            }
            ui.label(marked("Use custom enemy colour", toggle_changed));
            ui.add_space(18.0);
            ui.label(marked("Enemy colour", color_changed));
            let [r, g, b] = state.enemy_ui_color();
            let mut rgb = [r, g, b];
            ui.add_enabled_ui(on, |ui| {
                ui.spacing_mut().interact_size = vec2(40.0, 20.0);
                egui::widgets::color_picker::color_edit_button_srgb(ui, &mut rgb)
            })
            .response
            .on_disabled_hover_text("Turn on custom enemy colour first.");
            if rgb != [r, g, b] {
                actions.push(Action::Enemy(rgb));
            }
            ui.label(
                RichText::new(format!("{r}, {g}, {b}"))
                    .size(11.5)
                    .color(WEAK),
            );
            if (toggle_changed || color_changed)
                && widgets::reset_pill(ui)
                    .on_hover_text("Back to your preset")
                    .clicked()
            {
                actions.push(Action::ResetEnemy);
            }
        });
    });
}

fn map_style(ui: &mut Ui, state: &AppState, actions: &mut Vec<Action>) {
    let style = &state.profile.hud.minimap;
    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            widgets::caption(ui, "Map and markers");
            widgets::badge(ui, "Experimental, untested in game", WARN);
        });
        widgets::hint(
            ui,
            "Goes into the HUD addon on Apply. Sizes are relative to the game's own.",
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if widgets::switch(ui, style.minimal).clicked() {
                actions.push(Action::Minimal(!style.minimal));
            }
            ui.label(marked("Hide the frame around the minimap", style.minimal));
            let n = style.changed_count();
            ui.add_space(12.0);
            ui.label(
                RichText::new(format!("Changed: {n}"))
                    .small()
                    .color(if n > 0 { ACCENT } else { WEAK }),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(n > 0, egui::Button::new("Reset map and markers"))
                    .on_hover_text("Back to the game's own map and marker sizes")
                    .clicked()
                {
                    actions.push(Action::ResetStyle);
                }
            });
        });
        let columns = if ui.available_width() >= 560.0 { 2 } else { 1 };
        let gap = 24.0;
        let width = (ui.available_width() - gap * (columns - 1) as f32) / columns as f32;
        let rows: Vec<Option<&MarkerSpec>> = std::iter::once(None)
            .chain(MARKERS.iter().map(Some))
            .collect();
        ui.add_space(4.0);
        for chunk in rows.chunks(columns) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for (i, row) in chunk.iter().enumerate() {
                    if i > 0 {
                        ui.add_space(gap);
                    }
                    match row {
                        None => {
                            let pct = style.map_opacity_pct;
                            if let Some(v) =
                                percent_slider(ui, "Map opacity", pct, MAP_OPACITY_RANGE, 5, width)
                            {
                                actions.push(Action::MapOpacity(v));
                            }
                        }
                        Some(spec) => {
                            let pct = style.scale(spec.group);
                            if let Some(v) =
                                percent_slider(ui, spec.label, pct, MARKER_SCALE_RANGE, 10, width)
                            {
                                actions.push(Action::MarkerScale(spec.group, v));
                            }
                        }
                    }
                }
            });
        }
    });
}

/// A labelled percent slider, highlighted away from 100 %; `Some` when moved.
pub(crate) fn percent_slider<T>(
    ui: &mut Ui,
    label: &str,
    value: T,
    range: RangeInclusive<T>,
    step: u8,
    width: f32,
) -> Option<T>
where
    T: egui::emath::Numeric + Into<f64>,
{
    let mut v = value;
    let changed = value.into() != 100.0;
    ui.allocate_ui_with_layout(
        vec2(width, 24.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.allocate_ui_with_layout(
                vec2(120.0, 20.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.set_min_width(120.0);
                    ui.add(egui::Label::new(marked(label, changed)).truncate());
                },
            );
            let readout = 40.0;
            ui.spacing_mut().slider_width = (ui.available_width() - readout - 8.0).max(60.0);
            ui.add(
                egui::Slider::new(&mut v, range)
                    .show_value(false)
                    .step_by(f64::from(step)),
            );
            ui.label(
                RichText::new(format!("{}%", v.into().round()))
                    .size(12.5)
                    .color(if changed { ACCENT } else { TEXT }),
            );
        },
    );
    (v.into() != value.into()).then_some(v)
}

fn icons(ui: &mut Ui, state: &AppState, actions: &mut Vec<Action>) {
    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            widgets::caption(ui, "Minimap icons");
            widgets::badge(ui, "Experimental, untested in game", WARN);
        });
        widgets::hint(
            ui,
            "Goes into the HUD addon on Apply. Troopers and ziplines can't be recoloured. With \
             the game's own enemy colour (above) turned on, it may win for enemy icons; untested.",
        );
        ui.add_space(4.0);
        let colors = &state.profile.hud.minimap_colors;
        ui.horizontal_wrapped(|ui| {
            let current = state.minimap_preset();
            for preset in MinimapPreset::ALL {
                if ui
                    .selectable_label(current == Some(preset), preset.label())
                    .on_hover_text(preset.blurb())
                    .clicked()
                {
                    actions.push(Action::Preset(Some(preset)));
                }
            }
            ui.add_space(6.0);
            let n = colors.len();
            ui.label(
                RichText::new(format!("Changed: {n}"))
                    .small()
                    .color(if n > 0 { ACCENT } else { WEAK }),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(n > 0, egui::Button::new("Reset icon colours"))
                    .on_hover_text("Back to the game's own icon colours")
                    .clicked()
                {
                    actions.push(Action::Preset(None));
                }
            });
        });
        let columns = if ui.available_width() >= 560.0 { 2 } else { 1 };
        let gap = 24.0;
        for (title, ids) in GROUPS {
            ui.add_space(6.0);
            widgets::caption(ui, title);
            let width = (ui.available_width() - gap * (columns - 1) as f32) / columns as f32;
            for chunk in ids.chunks(columns) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for (i, &id) in chunk.iter().enumerate() {
                        if i > 0 {
                            ui.add_space(gap);
                        }
                        icon_row(ui, state, id, width, actions);
                    }
                });
            }
        }
    });
}

fn icon_row(ui: &mut Ui, state: &AppState, id: IconId, width: f32, actions: &mut Vec<Action>) {
    let spec = minimap_colors::spec(id);
    let set = state.profile.hud.minimap_colors.get(&id).copied();
    let vanilla = spec.vanilla.and_then(|v| v.parse::<Color>().ok());
    ui.allocate_ui_with_layout(
        vec2(width, 24.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing.x = 6.0;
            let top = ui.cursor().top();
            if set.is_some() {
                let x = ui.max_rect().left() - 7.0;
                ui.painter().rect_filled(
                    Rect::from_x_y_ranges(x..=x + 3.0, top + 2.0..=top + 22.0),
                    CornerRadius::same(2),
                    ACCENT,
                );
            }
            let label = ui.label(marked(spec.label, set.is_some()));
            if !spec.notes.is_empty() {
                label.on_hover_text(spec.notes);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let before = to_color32(set.or(vanilla).unwrap_or(Color([0xFF; 4])));
                let mut picked = before;
                let alpha = if spec.property == "wash-color" {
                    Alpha::OnlyBlend
                } else {
                    Alpha::Opaque
                };
                ui.spacing_mut().interact_size = vec2(36.0, 18.0);
                let response = color_edit_button_srgba(ui, &mut picked, alpha);
                if set.is_none() && vanilla.is_none() {
                    unset_swatch(ui.painter(), response.rect);
                    response
                        .on_hover_text("Shows the icon's own texture colours. Click to pick one.");
                }
                if picked != before {
                    let [r, g, b, a] = picked.to_srgba_unmultiplied();
                    actions.push(Action::Icon(id, Some(Color([r, g, b, a]))));
                }
                default_chip(ui, vanilla);
                if set.is_some()
                    && widgets::reset_pill(ui)
                        .on_hover_text("Back to the game's colour")
                        .clicked()
                {
                    actions.push(Action::Icon(id, None));
                }
            });
        },
    );
}

/// Covers the picker's white swatch so an unset texture row doesn't read as white.
fn unset_swatch(p: &Painter, r: Rect) {
    p.rect(
        r,
        CornerRadius::same(2),
        RAIL,
        Stroke::new(1.0, BORDER),
        egui::StrokeKind::Inside,
    );
    p.line_segment(
        [
            r.left_bottom() + vec2(4.0, -3.0),
            r.right_top() + vec2(-4.0, 3.0),
        ],
        Stroke::new(1.0, WEAK),
    );
}

/// "default" with the vanilla swatch, or "texture" when the icon's own colours show.
fn default_chip(ui: &mut Ui, vanilla: Option<Color>) {
    let text = if vanilla.is_some() {
        "default"
    } else {
        "texture"
    };
    let font = egui::FontId::proportional(10.5);
    let galley = ui.painter().layout_no_wrap(text.into(), font, WEAK);
    let (rect, response) =
        ui.allocate_exact_size(vec2(galley.size().x + 22.0, 16.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(255), RAIL);
    let dot = pos2(rect.left() + 9.0, rect.center().y);
    match vanilla {
        Some(c) => {
            p.circle_filled(dot, 4.0, to_color32(c));
        }
        None => {
            p.circle_stroke(dot, 3.5, Stroke::new(1.0, WEAK));
        }
    }
    p.galley(
        pos2(rect.left() + 16.0, rect.center().y - galley.size().y / 2.0),
        galley,
        WEAK,
    );
    if let Some(c) = vanilla {
        response.on_hover_text(format!("Game colour {c}"));
    }
}

pub(crate) fn marked(text: &str, changed: bool) -> RichText {
    RichText::new(text)
        .size(12.5)
        .color(if changed { ACCENT } else { TEXT })
}

fn to_color32(c: Color) -> Color32 {
    let [r, g, b, a] = c.0;
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

fn preview_card(ui: &mut Ui, state: &AppState, images: &mut Images) {
    widgets::section(ui, "Preview", |ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(PREVIEW), Sense::hover());
        let real = minimap(ui.painter(), rect, state, images);
        ui.add_space(2.0);
        widgets::hint(
            ui,
            if real {
                "Your game's map and icons with the settings above. Top is the enemy base; \
                 heroes and objectives are placed by hand."
            } else {
                "A mock-up of the colours above. Top is the enemy base."
            },
        );
        ui.add_space(6.0);
        crate::game_shot::in_game(
            ui,
            dt_core::hud::elements::ElementId::Minimap,
            Vec2::splat(PREVIEW),
        );
    });
}

/// The colour an icon shows: the user's, else the game's, else a stand-in for its texture.
fn ink(state: &AppState, id: IconId, texture: Color32) -> Color32 {
    let spec = minimap_colors::spec(id);
    state
        .profile
        .hud
        .minimap_colors
        .get(&id)
        .copied()
        .or_else(|| spec.vanilla.and_then(|v| v.parse().ok()))
        .map_or(texture, to_color32)
}

/// The map's circle on screen; pieces are placed as fractions of its radius.
struct Map {
    centre: Pos2,
    radius: f32,
    /// Screen points per game pixel: the game's map is 360 px across.
    k: f32,
}

impl Map {
    fn new(area: Rect) -> Self {
        Map {
            centre: area.center(),
            radius: area.width() / 2.0,
            k: area.width() / 360.0,
        }
    }

    fn at(&self, x: f32, y: f32) -> Pos2 {
        self.centre + vec2(x, y) * self.radius
    }

    fn rect(&self, art: art::Art, scale: f32, at: Pos2) -> Rect {
        let [w, h] = art.size.map(f32::from);
        Rect::from_center_size(at, vec2(w, h) * self.k * scale)
    }
}

/// The blurred world the game shows through its 70% opaque frame, sampled from the in-game
/// screenshot around the minimap.
const WORLD: Color32 = Color32::from_rgb(113, 74, 47);
/// What frame and world make together, for when the frame's picture is missing.
const FRAMED: Color32 = Color32::from_rgb(52, 36, 28);
const OFF_WHITE: Color32 = Color32::from_rgb(0xFF, 0xEF, 0xD7);
const CAMP: Color32 = Color32::from_rgb(143, 179, 164);

/// A lane from the enemy base (top) to ours in map fractions, traced from the in-game
/// screenshot; points up to `front` are on the enemy's side.
struct Lane {
    path: &'static [(f32, f32)],
    front: usize,
}

const LANE_PATHS: [Lane; 3] = [
    Lane {
        path: &[
            (0.06, -0.88),
            (-0.05, -0.73),
            (-0.16, -0.67),
            (-0.39, -0.62),
            (-0.64, -0.51),
            (-0.80, -0.34),
            (-0.85, -0.13),
            (-0.86, 0.13),
            (-0.81, 0.27),
            (-0.73, 0.29),
            (-0.64, 0.43),
            (-0.40, 0.59),
            (-0.26, 0.69),
            (-0.21, 0.84),
        ],
        front: 10,
    },
    Lane {
        path: &[
            (0.06, -0.88),
            (0.06, -0.57),
            (0.05, -0.44),
            (-0.07, -0.25),
            (-0.06, -0.08),
            (-0.08, 0.08),
            (-0.17, 0.26),
            (-0.19, 0.46),
            (-0.19, 0.84),
        ],
        front: 3,
    },
    Lane {
        path: &[
            (0.06, -0.88),
            (0.18, -0.73),
            (0.28, -0.67),
            (0.43, -0.57),
            (0.61, -0.44),
            (0.70, -0.31),
            (0.72, -0.22),
            (0.69, -0.05),
            (0.65, 0.10),
            (0.60, 0.26),
            (0.51, 0.43),
            (0.28, 0.56),
            (-0.03, 0.60),
            (-0.13, 0.70),
            (-0.17, 0.84),
        ],
        front: 6,
    },
];

/// One guardian per lane, left to right: the enemy's, then ours.
const GUARDIANS: [[(f32, f32); 3]; 2] = [
    [(-0.16, -0.67), (0.06, -0.56), (0.28, -0.67)],
    [(-0.40, 0.59), (-0.19, 0.55), (0.28, 0.56)],
];
/// Per base: the patron arch, the diamond in its opening and the two shrines.
const BASES: [[(f32, f32); 4]; 2] = [
    [(0.06, -0.85), (0.06, -0.89), (-0.10, -0.80), (0.22, -0.80)],
    [(-0.20, 0.83), (-0.20, 0.87), (-0.33, 0.72), (-0.04, 0.72)],
];

/// The minimap's base: the dark fill and the frame behind the map (`rect`), then the map
/// (`area`) faded by `fade`. True when the map is the game's.
pub(crate) fn base(
    p: &Painter,
    images: &mut Images,
    rect: Rect,
    area: Rect,
    fade: Color32,
    frame: bool,
) -> bool {
    if frame {
        let picture = images.get(art::MINIMAP_FRAME, rect.width()).is_some();
        let fill = if picture { WORLD } else { FRAMED };
        let w = rect.width();
        p.circle_filled(rect.center(), w * 0.445, fill);
        for (x, y) in [(0.123, -0.795), (-0.123, 0.795)] {
            p.circle_filled(rect.center() + vec2(x, y) * w / 2.0, w * 0.1, fill);
        }
        images.paint(p, art::MINIMAP_FRAME, rect, Color32::WHITE);
    }
    let real = images.paint(p, art::MINIMAP_MAP, area, fade);
    if !real {
        ground(p, &Map::new(area), fade);
    }
    real
}

/// The hand-drawn map: the streets' silhouette, a disc with the two base lobes, the colour
/// of the game map's 70% black over the frame. Opaque, so the overlaps don't show.
fn ground(p: &Painter, map: &Map, fade: Color32) {
    let street = FRAMED.lerp_to_gamma(Color32::from_rgb(16, 11, 8), f32::from(fade.a()) / 255.0);
    p.circle_filled(map.centre, map.radius * 0.86, street);
    for (x, y) in [(0.12, -0.8), (-0.12, 0.81)] {
        p.circle_filled(map.at(x, y), map.radius * 0.18, street);
    }
}

/// Paints the minimap: the game's map, frame and markers where their images load, the
/// hand-drawn stand-ins where they don't. True when the map itself is the game's.
fn minimap(p: &Painter, rect: Rect, state: &AppState, images: &mut Images) -> bool {
    let style = &state.profile.hud.minimap;
    let area = rect.shrink(rect.width() * 20.0 / 400.0);
    let map = Map::new(area);
    let fade = Color32::WHITE.gamma_multiply(f32::from(style.map_opacity_pct) / 100.0);
    let real = base(p, images, rect, area, fade, !style.minimal);
    let scale = |group| f32::from(style.scale(group)) / 100.0;
    let k = map.k;

    let enemy_obj = ink(state, IconId::EnemyObjective, WEAK);
    let ally_set = state
        .profile
        .hud
        .minimap_colors
        .get(&IconId::AllyObjective)
        .copied()
        .map(to_color32);
    for (lane, colour) in LANE_PATHS.iter().zip(LANES) {
        let points: Vec<Pos2> = lane.path.iter().map(|&(x, y)| map.at(x, y)).collect();
        let (enemy, ours) = (&points[..=lane.front], &points[lane.front..]);
        for (half, c) in [(enemy, enemy_obj), (ours, colour)] {
            p.add(Shape::line(
                half.to_vec(),
                Stroke::new(2.0 * k, c.gamma_multiply(0.7)),
            ));
        }
    }

    let obj = scale(MarkerGroup::Objectives);
    let ally_base = ally_set.unwrap_or(OFF_WHITE);
    for (team, spots) in GUARDIANS.iter().enumerate() {
        for (lane, &(x, y)) in spots.iter().enumerate() {
            let (tint, health) = match team {
                0 => (enemy_obj, if lane == 1 { 0.5 } else { 1.0 }),
                _ => (ally_set.unwrap_or(LANES[lane]), 1.0),
            };
            let at = map.at(x, y);
            if !objective(images, p, &map, art::GUARDIAN, at, obj, 0.0, tint, health) {
                let s = 9.0 * k * obj;
                diamond(p, at, s, tint.gamma_multiply(0.35));
                let lower = cut_top(&diamond_points(at, s), at.y + s * (1.0 - 2.0 * health));
                p.add(Shape::convex_polygon(lower, tint, Stroke::NONE));
            }
        }
    }
    for (team, spots) in BASES.iter().enumerate() {
        let (tint, turn) = match team {
            0 => (enemy_obj, 0.0),
            _ => (ally_base, std::f32::consts::PI),
        };
        let [patron, stage2, shrines @ ..] = *spots;
        let at = map.at(patron.0, patron.1);
        if !objective(images, p, &map, art::PATRON, at, obj, turn, tint, 1.0) {
            arch(p, at, turn, k * obj, tint);
        }
        let at = map.at(stage2.0, stage2.1);
        if !objective(images, p, &map, art::PATRON_STAGE2, at, obj, 0.0, tint, 1.0) {
            diamond(p, at, 7.0 * k * obj, tint);
        }
        for (x, y) in shrines {
            let at = map.at(x, y);
            if !objective(images, p, &map, art::SHRINE, at, obj, 0.0, tint, 1.0) {
                let mut outline = diamond_points(at, 6.0 * k * obj);
                outline.push(outline[0]);
                p.add(Shape::line(outline, Stroke::new(2.2 * k * obj, tint)));
            }
        }
    }

    let mark = |images: &mut Images,
                id: IconId,
                at: Pos2,
                tint: Color32,
                fallback: &dyn Fn(&Painter, Pos2, Color32)| {
        let group = if id == IconId::Shop {
            MarkerGroup::Shops
        } else {
            MarkerGroup::Objectives
        };
        let drawn = art::marker(id)
            .is_some_and(|a| images.paint(p, a, map.rect(a, scale(group), at), tint));
        if !drawn {
            fallback(p, at, tint);
        }
    };
    let texture = |stand_in: Color32| if real { Color32::WHITE } else { stand_in };
    let camp = |p: &Painter, at: Pos2, c: Color32| triangle(p, at, 4.0 * k * obj, c);
    for (id, x, y) in [
        (IconId::SmallCamp, -0.84, -0.50),
        (IconId::MediumCamp, -0.27, -0.31),
        (IconId::SmallCamp, -0.48, 0.05),
        (IconId::SmallCamp, 0.32, -0.21),
        (IconId::MediumCamp, 0.42, -0.12),
        (IconId::SmallCamp, 0.18, 0.20),
        (IconId::LargeCamp, -0.43, 0.33),
        (IconId::SmallCamp, 0.77, 0.37),
        (IconId::Vault, 0.13, -0.12),
    ] {
        mark(
            images,
            id,
            map.at(x, y),
            ink(state, id, texture(CAMP)),
            &camp,
        );
    }
    let ring = |p: &Painter, at: Pos2, c: Color32| {
        p.circle_stroke(at, 6.0 * k * obj, Stroke::new(2.0 * k, c));
    };
    for (id, x, y) in [
        (IconId::EnemyUrnReturn, 0.45, -0.45),
        (IconId::AllyUrnReturn, 0.12, 0.46),
    ] {
        mark(
            images,
            id,
            map.at(x, y),
            ink(state, id, texture(TEXT)),
            &ring,
        );
    }
    let mid = ink(state, IconId::MidBoss, texture(Color32::from_gray(110)));
    let mid_ring = |p: &Painter, at: Pos2, c: Color32| {
        p.circle_stroke(at, 14.0 * k * obj, Stroke::new(3.0 * k, c));
    };
    mark(images, IconId::MidBoss, map.at(0.0, 0.04), mid, &mid_ring);
    let gem = |p: &Painter, at: Pos2, c: Color32| diamond(p, at, 5.0 * k * obj, c);
    for (id, x, y) in [
        (IconId::WeaponPowerup, -0.30, 0.15),
        (IconId::SpiritPowerup, 0.20, -0.45),
    ] {
        mark(
            images,
            id,
            map.at(x, y),
            ink(state, id, texture(TEXT)),
            &gem,
        );
    }
    let shop_ink = ink(state, IconId::Shop, texture(CAMP));
    let shop = |p: &Painter, at: Pos2, c: Color32| {
        p.rect_filled(
            Rect::from_center_size(at, vec2(12.0, 9.0) * k * scale(MarkerGroup::Shops)),
            CornerRadius::same(1),
            c,
        );
    };
    mark(images, IconId::Shop, map.at(-0.43, -0.15), shop_ink, &shop);
    mark(images, IconId::Shop, map.at(0.30, 0.10), shop_ink, &shop);

    let teams = [
        (
            &art::ENEMIES[..],
            &[(-0.55, -0.30), (0.32, -0.42), (0.55, -0.02), (-0.36, -0.50)][..],
            IconId::EnemyHero,
            MarkerGroup::EnemyHeroes,
        ),
        (
            &art::ALLIES[..],
            &[(0.27, 0.02), (-0.58, 0.20), (0.42, 0.36)][..],
            IconId::AllyHero,
            MarkerGroup::AllyHeroes,
        ),
    ];
    for (team, spots, id, group) in teams {
        let fill = ink(state, id, TEXT).gamma_multiply(0.9);
        let r = 12.5 * k * scale(group);
        for (hero, &(x, y)) in team.iter().zip(spots) {
            let at = map.at(x, y);
            p.circle_filled(at, r, fill);
            if !images.paint_disc(p, hero.marker, at, r * 0.85, Color32::WHITE) {
                p.circle_filled(at, r * 0.5, Color32::from_rgb(30, 30, 34));
            }
        }
    }

    let me = map.at(-0.12, -0.24);
    let s = scale(MarkerGroup::LocalHero);
    let r = 15.0 * k * s;
    let look = 0.95_f32;
    let dir = vec2(look.cos(), look.sin());
    let cone = vec2(36.0, 36.0) * k * s;
    let warm = Color32::from_rgb(0xF4, 0xD3, 0x5E);
    if !images.paint_turned(
        p,
        art::VIEW_CONE,
        me + dir * (r + 0.1 * cone.x),
        cone,
        look,
        warm,
    ) {
        let side = vec2(-dir.y, dir.x) * r * 0.6;
        let tip = me + dir * (r + 12.0 * k * s);
        p.add(Shape::convex_polygon(
            vec![tip, me + side, me - side],
            warm,
            Stroke::NONE,
        ));
    }
    p.circle_filled(me, r, ink(state, IconId::LocalHero, TEXT));
    let bust = art::ALLIES[3].marker;
    let mine = art::marker(IconId::LocalHero).expect("local hero art");
    let shine = map.rect(mine, s, me);
    let drawn = images.paint_disc(p, bust, me, r * 1.1, Color32::WHITE)
        && images.paint(p, mine, shine, Color32::WHITE);
    if !drawn {
        p.circle_filled(me, r * 0.5, Color32::from_rgb(30, 30, 34));
        p.circle_stroke(me, r, Stroke::new(1.5, Color32::WHITE));
    }

    let angle = -0.35_f32;
    let out = vec2(angle.sin(), -angle.cos());
    let enemy = ink(state, IconId::EnemyHero, TEXT);
    let arrow_art = art::marker(IconId::EnemyHeroArrow).expect("arrow art");
    let size = Vec2::splat(22.0) * k * scale(MarkerGroup::EnemyHeroes);
    let at = map.centre + out * (map.radius - size.x * 0.4);
    let tint = ink(state, IconId::EnemyHeroArrow, Color32::WHITE);
    if !images.paint_turned(p, arrow_art, at, size, out.y.atan2(out.x), tint) {
        let arrow = ink(state, IconId::EnemyHeroArrow, enemy);
        edge_arrow(p, map.centre, map.radius, angle, arrow);
    }
    real
}

/// Paints objective `o` at `at`, turned `turn` radians clockwise, its fill kept only on
/// the lower `health` of its picture; false when a picture is missing.
#[allow(clippy::too_many_arguments)]
fn objective(
    images: &mut Images,
    p: &Painter,
    map: &Map,
    o: art::Objective,
    at: Pos2,
    scale: f32,
    turn: f32,
    tint: Color32,
    health: f32,
) -> bool {
    let rect = map.rect(o.back, scale, at);
    if !images.paint_turned(p, o.back, at, rect.size(), turn, tint) {
        return false;
    }
    if health >= 1.0 {
        return images.paint_turned(p, o.fill, at, rect.size(), turn, tint);
    }
    let corners = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ];
    let cut = rect.bottom() - rect.height() * health;
    images.paint_shape(p, o.fill, rect, &cut_top(&corners, cut), tint)
}

/// The patron's arch where its picture is missing: a thick half ring over the opening,
/// turned like the picture (the arch fills the top half of its 75 px image).
fn arch(p: &Painter, at: Pos2, turn: f32, k: f32, color: Color32) {
    let (sin, cos) = turn.sin_cos();
    let centre = at + vec2(14.0 * k * sin, -14.0 * k * cos);
    let pi = std::f32::consts::PI;
    let points = arc(centre, 20.0 * k, pi + turn, 2.0 * pi + turn, 24);
    p.add(Shape::line(points, Stroke::new(13.0 * k, color)));
}

fn triangle(p: &Painter, c: Pos2, s: f32, fill: Color32) {
    let pts = vec![
        c + vec2(0.0, -s),
        c + vec2(s, s * 0.8),
        c + vec2(-s, s * 0.8),
    ];
    p.add(Shape::convex_polygon(pts, fill, Stroke::NONE));
}

fn diamond_points(c: Pos2, s: f32) -> Vec<Pos2> {
    vec![
        c + vec2(0.0, -s),
        c + vec2(s, 0.0),
        c + vec2(0.0, s),
        c + vec2(-s, 0.0),
    ]
}

fn diamond(p: &Painter, c: Pos2, s: f32, fill: Color32) {
    p.add(Shape::convex_polygon(
        diamond_points(c, s),
        fill,
        Stroke::NONE,
    ));
}

/// An enemy clamped to the rim, pointing outwards at `angle` (radians from straight up).
fn edge_arrow(p: &Painter, c: Pos2, r: f32, angle: f32, fill: Color32) {
    let dir = vec2(angle.sin(), -angle.cos());
    let side = vec2(-dir.y, dir.x);
    let tip = c + dir * (r - 3.0);
    let base = tip - dir * 9.0;
    p.add(Shape::convex_polygon(
        vec![tip, base + side * 5.0, base - side * 5.0],
        fill,
        Stroke::NONE,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(state: &mut AppState, width: f32) {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 900.0))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| page(ui, state));
        output.textures_delta.clear();
    }

    #[test]
    fn page_renders_wide_and_narrow_without_editing() {
        let (_dir, mut state) = crate::state::testutil::state();
        for width in [1200.0, 500.0] {
            render(&mut state, width);
        }
        assert!(!state.is_dirty(), "drawing alone changes nothing");
        state.apply_minimap_preset(Some(MinimapPreset::ColorBlind));
        render(&mut state, 1200.0);
        assert_eq!(state.minimap_preset(), Some(MinimapPreset::ColorBlind));
    }

    #[test]
    fn every_icon_is_in_exactly_one_group() {
        let mut ids: Vec<IconId> = GROUPS
            .iter()
            .flat_map(|(_, ids)| ids.iter().copied())
            .collect();
        ids.sort();
        let all: Vec<IconId> = minimap_colors::ICONS.iter().map(|s| s.id).collect();
        assert_eq!(ids, all);
    }
}
