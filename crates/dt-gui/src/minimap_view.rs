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

use crate::hud_art::Images;
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
/// Allied objectives keep a colour per lane unless the user sets one for all.
const LANES: [Color32; 4] = [
    Color32::from_rgb(0xFF, 0xDF, 0x40),
    Color32::from_rgb(0xFF, 0x00, 0xFF),
    Color32::from_rgb(0x2E, 0xC7, 0xE6),
    Color32::from_rgb(0x6B, 0xB2, 0x47),
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
    fn at(&self, x: f32, y: f32) -> Pos2 {
        self.centre + vec2(x, y) * self.radius
    }

    fn rect(&self, art: art::Art, scale: f32, at: Pos2) -> Rect {
        let [w, h] = art.size.map(f32::from);
        Rect::from_center_size(at, vec2(w, h) * self.k * scale)
    }
}

/// Paints the minimap: the game's map, frame and markers where their images load, the
/// hand-drawn stand-ins where they don't. True when the map itself is the game's.
fn minimap(p: &Painter, rect: Rect, state: &AppState, images: &mut Images) -> bool {
    let style = &state.profile.hud.minimap;
    let area = rect.shrink(rect.width() * 20.0 / 400.0);
    let map = Map {
        centre: area.center(),
        radius: area.width() / 2.0,
        k: area.width() / 360.0,
    };
    let fade = Color32::WHITE.gamma_multiply(f32::from(style.map_opacity_pct) / 100.0);
    let real = images.paint(p, art::MINIMAP_MAP, area, fade);
    if !real {
        ground(p, &map);
    }
    if !style.minimal && !images.paint(p, art::MINIMAP_FRAME, rect, Color32::WHITE) {
        p.circle_stroke(map.centre, map.radius, Stroke::new(1.5, BORDER));
    }
    let scale = |group| f32::from(style.scale(group)) / 100.0;
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

    let lane_x = [-0.62, -0.2, 0.2, 0.62];
    let enemy_obj = ink(state, IconId::EnemyObjective, WEAK);
    let ally_set = state
        .profile
        .hud
        .minimap_colors
        .get(&IconId::AllyObjective)
        .copied()
        .map(to_color32);
    for (i, x) in lane_x.into_iter().enumerate() {
        mark(
            images,
            IconId::EnemyObjective,
            map.at(x * 0.86, -0.55),
            enemy_obj,
            &tower,
        );
        let ally = ally_set.unwrap_or(LANES[i]);
        mark(
            images,
            IconId::AllyObjective,
            map.at(x * 0.86, 0.55),
            ally,
            &tower,
        );
    }
    for (y, tint) in [(-0.84, enemy_obj), (0.84, ally_set.unwrap_or(TEXT))] {
        let at = map.at(0.0, y);
        let scale = scale(MarkerGroup::Objectives);
        if !images.paint(p, art::PATRON, map.rect(art::PATRON, scale, at), tint) {
            tower(p, at, tint);
        }
    }
    for (id, x, y) in [
        (IconId::EnemyUrnReturn, -0.42, -0.72),
        (IconId::AllyUrnReturn, 0.42, 0.72),
    ] {
        mark(images, id, map.at(x, y), ink(state, id, WEAK), &ring);
    }
    let mid = ink(state, IconId::MidBoss, Color32::from_gray(110));
    let mid_ring = |p: &Painter, at: Pos2, c: Color32| {
        p.circle_stroke(at, 16.0, Stroke::new(2.0, c));
    };
    mark(images, IconId::MidBoss, map.at(0.0, 0.0), mid, &mid_ring);
    let camp = |p: &Painter, at: Pos2, c: Color32| triangle(p, at, 4.5, c);
    for (id, x, y) in [
        (IconId::SmallCamp, -0.42, -0.2),
        (IconId::SmallCamp, 0.42, 0.2),
        (IconId::MediumCamp, 0.42, -0.25),
        (IconId::MediumCamp, -0.42, 0.25),
        (IconId::LargeCamp, -0.85, 0.05),
        (IconId::Vault, 0.86, -0.05),
    ] {
        let texture = if real {
            Color32::WHITE
        } else {
            Color32::from_rgb(196, 168, 120)
        };
        mark(images, id, map.at(x, y), ink(state, id, texture), &camp);
    }
    let gem = |p: &Painter, at: Pos2, c: Color32| diamond(p, at, 4.0, c);
    for (id, x, y) in [
        (IconId::WeaponPowerup, -0.05, 0.35),
        (IconId::SpiritPowerup, 0.05, -0.35),
        (IconId::UnsecuredSouls, 0.3, 0.05),
    ] {
        mark(images, id, map.at(x, y), ink(state, id, TEXT), &gem);
    }
    let shop_ink = ink(state, IconId::Shop, TEXT);
    mark(images, IconId::Shop, map.at(-0.25, 0.42), shop_ink, &shop);
    mark(images, IconId::Shop, map.at(0.25, -0.42), shop_ink, &shop);

    let teams = [
        (
            &art::ENEMIES[..],
            &[(-0.55, -0.32), (0.2, -0.62), (0.68, -0.3), (-0.15, -0.15)][..],
            IconId::EnemyHero,
            MarkerGroup::EnemyHeroes,
        ),
        (
            &art::ALLIES[..],
            &[(-0.6, 0.3), (0.15, 0.3), (0.6, 0.48)][..],
            IconId::AllyHero,
            MarkerGroup::AllyHeroes,
        ),
    ];
    for (team, spots, id, group) in teams {
        let fill = ink(state, id, TEXT);
        for (hero, &(x, y)) in team.iter().zip(spots) {
            let at = map.at(x, y);
            let face = map.rect(hero.marker, scale(group), at);
            if images.get(hero.marker, face.width()).is_some() {
                p.circle_filled(at, face.width() * 0.4, fill);
                images.paint(
                    p,
                    hero.marker,
                    face.shrink(face.width() * 0.075),
                    Color32::WHITE,
                );
            } else {
                self::hero(p, at, fill);
            }
        }
    }
    let me = map.at(-0.2, 0.62);
    let mine = art::marker(IconId::LocalHero).expect("local hero art");
    let tint = ink(state, IconId::LocalHero, Color32::WHITE);
    let rect = map.rect(mine, scale(MarkerGroup::LocalHero), me);
    if !images.paint(p, mine, rect, tint) {
        hero(p, me, ink(state, IconId::LocalHero, TEXT));
        p.circle_stroke(me, 9.5, Stroke::new(1.5, Color32::WHITE));
    }
    let enemy = ink(state, IconId::EnemyHero, TEXT);
    let arrow = ink(state, IconId::EnemyHeroArrow, enemy);
    edge_arrow(p, map.centre, map.radius, -0.35, arrow);
    real
}

/// The hand-drawn map: a disc, four lanes and the mid.
fn ground(p: &Painter, map: &Map) {
    p.circle_filled(map.centre, map.radius, Color32::from_rgb(34, 38, 34));
    let road = Stroke::new(5.0, Color32::from_rgb(52, 58, 52));
    for x in [-0.62, -0.2, 0.2, 0.62] {
        p.line_segment([map.at(x * 0.8, -0.78), map.at(x, 0.0)], road);
        p.line_segment([map.at(x, 0.0), map.at(x * 0.8, 0.78)], road);
    }
    p.circle_filled(
        map.at(0.0, 0.0),
        map.radius * 0.13,
        Color32::from_rgb(44, 49, 44),
    );
}

fn hero(p: &Painter, pos: Pos2, fill: Color32) {
    p.circle_filled(pos, 7.5, fill);
    p.circle_filled(pos, 4.0, Color32::from_rgb(30, 30, 34));
}

fn tower(p: &Painter, pos: Pos2, fill: Color32) {
    p.rect_filled(
        Rect::from_center_size(pos, Vec2::splat(8.0)),
        CornerRadius::same(2),
        fill,
    );
}

fn ring(p: &Painter, pos: Pos2, color: Color32) {
    p.circle_stroke(pos, 5.0, Stroke::new(2.0, color));
}

fn triangle(p: &Painter, c: Pos2, s: f32, fill: Color32) {
    let pts = vec![
        c + vec2(0.0, -s),
        c + vec2(s, s * 0.8),
        c + vec2(-s, s * 0.8),
    ];
    p.add(Shape::convex_polygon(pts, fill, Stroke::NONE));
}

fn diamond(p: &Painter, c: Pos2, s: f32, fill: Color32) {
    let pts = vec![
        c + vec2(0.0, -s),
        c + vec2(s, 0.0),
        c + vec2(0.0, s),
        c + vec2(-s, 0.0),
    ];
    p.add(Shape::convex_polygon(pts, fill, Stroke::NONE));
}

fn shop(p: &Painter, c: Pos2, color: Color32) {
    p.rect_stroke(
        Rect::from_center_size(c, vec2(9.0, 7.0)),
        CornerRadius::same(1),
        Stroke::new(1.5, color),
        egui::StrokeKind::Inside,
    );
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
