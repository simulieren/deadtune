//! Health bar page: a gallery of ready-made bar styles, the game's bar and yours side by
//! side at any health, and an inspector with every setting behind the styles. Drawing is
//! `crate::health_art`; edits go out with the HUD addon on Apply.

use dt_core::hud::health_style::{
    BarAngle, BarShape, HealthPreset, HealthStyle, LENGTH_RANGE, NumberFont, NumberLayout, Offset,
    PresetGroup, RegenPlace, THICKNESS_RANGE,
};
use dt_core::texture::adjust::Rgb;
use eframe::egui::color_picker::color_edit_button_srgb;
use eframe::egui::emath::Rot2;
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Id, Layout, Margin, Rect, RichText, Sense,
    Stroke, StrokeKind, Ui, pos2, vec2,
};

use crate::health_art::{Drawn, Part, draw_health};
use crate::hud_art::Images;
use crate::icons::{self, Icon};
use crate::state::AppState;
use crate::theme::{self, ACCENT, BORDER, CARD, CARD_HOVER, ON_ACCENT, RAIL, TEXT, WARN, WEAK};
use crate::widgets;

const INSPECTOR: f32 = 360.0;
const CARD_SIZE: egui::Vec2 = vec2(148.0, 156.0);
const NUMBER_SIZES: [u16; 5] = [80, 100, 130, 160, 200];
const SWATCHES: [(&str, Rgb); 6] = [
    ("Paper white", Rgb([0xFF, 0xEF, 0xD7])),
    ("White", Rgb::WHITE),
    ("Green", Rgb([0x5F, 0xCB, 0x8C])),
    ("Amber", Rgb([0xF0, 0xB3, 0x41])),
    ("Cyan", Rgb([0x2D, 0xDB, 0xF1])),
    ("Violet", Rgb([0xA7, 0x8B, 0xFA])),
];

pub fn page(ui: &mut Ui, state: &mut AppState) {
    crate::hud_view::hud_error(ui, state);
    crate::hud_view::show_layout_note(ui, state, dt_core::hud::elements::ElementId::HealthAndAmmo);
    let mut style = state.profile.hud.health.clone();
    let ctx = ui.ctx().clone();
    crate::hud_art::with(&ctx, state, |_, images| {
        header(ui, &mut style);
        ui.add_space(8.0);
        gallery(ui, &mut style, images);
        ui.add_space(14.0);
        if ui.available_width() >= 900.0 {
            ui.horizontal_top(|ui| {
                let left = ui.available_width() - INSPECTOR - 14.0;
                ui.vertical(|ui| {
                    ui.set_width(left);
                    compare(ui, &mut style, images);
                });
                ui.add_space(14.0 - ui.spacing().item_spacing.x);
                ui.vertical(|ui| {
                    ui.set_width(INSPECTOR);
                    inspector(ui, &mut style);
                });
            });
        } else {
            compare(ui, &mut style, images);
            ui.add_space(12.0);
            inspector(ui, &mut style);
        }
    });
    credits(ui);
    if style != state.profile.hud.health {
        state.set_health_style(style);
    }
}

fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(theme::RADIUS))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

fn header(ui: &mut Ui, style: &mut HealthStyle) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Bar style")
                .size(15.0)
                .family(theme::semibold())
                .color(TEXT),
        );
        widgets::badge(ui, "Experimental, untested in game", WARN);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let n = style.changed_count();
            if n > 0
                && widgets::reset_pill(ui)
                    .on_hover_text("Back to the game's own health bar")
                    .clicked()
            {
                *style = HealthStyle::default();
            }
            let text = match style.preset() {
                Some(p) => p.label().to_string(),
                None => format!("Custom · {n} {}", if n == 1 { "change" } else { "changes" }),
            };
            ui.label(
                RichText::new(text)
                    .size(12.5)
                    .color(if n > 0 { ACCENT } else { WEAK }),
            );
        });
    });
}

/// Every ready-made style as a card with its look at hurt health, filtered by group.
fn gallery(ui: &mut Ui, style: &mut HealthStyle, images: &mut Images) {
    let id = Id::new("health_gallery_group");
    let mut group: Option<usize> = ui.data_mut(|d| *d.get_temp_mut_or(id, None));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        if widgets::chip(ui, "All", ACCENT, Some(group.is_none())).clicked() {
            group = None;
        }
        for (i, g) in PresetGroup::ALL.into_iter().enumerate() {
            if widgets::chip(ui, g.label(), ACCENT, Some(group == Some(i))).clicked() {
                group = if group == Some(i) { None } else { Some(i) };
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(id, group));
    ui.add_space(8.0);
    let current = style.preset();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
        for preset in HealthPreset::ALL {
            if group.is_some_and(|g| PresetGroup::ALL[g] != preset.group()) {
                continue;
            }
            if style_card(ui, images, preset, current == Some(preset)).clicked() {
                *style = preset.style();
            }
        }
    });
}

fn style_card(
    ui: &mut Ui,
    images: &mut Images,
    preset: HealthPreset,
    selected: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(CARD_SIZE, Sense::click());
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(10),
        if response.hovered() { CARD_HOVER } else { CARD },
        Stroke::new(
            if selected { 2.0 } else { 1.0 },
            if selected { ACCENT } else { BORDER },
        ),
        StrokeKind::Inside,
    );
    let stage = Rect::from_min_size(rect.min + vec2(7.0, 7.0), vec2(rect.width() - 14.0, 112.0));
    backdrop(ui, stage);
    let painter = ui.painter();
    draw_health(painter, images, &preset.style(), stage.shrink(6.0), 0.55);
    painter.text(
        pos2(rect.left() + 11.0, stage.bottom() + 9.0),
        Align2::LEFT_TOP,
        preset.label(),
        FontId::new(12.5, theme::semibold()),
        if selected { ACCENT } else { TEXT },
    );
    if selected {
        let c = pos2(rect.right() - 17.0, rect.top() + 17.0);
        painter.circle_filled(c, 8.0, ACCENT);
        icons::paint(
            painter,
            Rect::from_center_size(c, vec2(11.0, 11.0)),
            Icon::Check,
            ON_ACCENT,
        );
    }
    response
        .on_hover_text(preset.blurb())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A dimmed patch of the in-game screenshot (the wooden door right of the crosshair, clear of
/// any HUD) behind a preview, so dark outlines read the way they do over the game.
fn backdrop(ui: &Ui, rect: Rect) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(8), RAIL);
    let Some(texture) = crate::game_shot::texture(ui.ctx()) else {
        return;
    };
    let h = 0.38;
    let w = (rect.aspect_ratio() * h * 720.0 / 1280.0).min(0.3);
    let crop = [0.70 - w / 2.0, 0.22, w, h];
    crate::game_shot::paint(&painter, &texture, crop, rect, Color32::from_gray(105));
}

/// The game's bar and yours at one health level, with the level to scrub. Yours is an
/// editor: the bar, the number and the regen can be dragged.
fn compare(ui: &mut Ui, style: &mut HealthStyle, images: &mut Images) {
    card(ui, |ui| {
        let id = Id::new("health_scrub");
        let start = std::env::var("DEADTUNE_HEALTH_LEVEL")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(55);
        let mut pct: i32 = ui.data_mut(|d| *d.get_temp_mut_or(id, start));
        let fill = pct as f32 / 100.0;
        let gap = 10.0;
        let w = (ui.available_width() - gap) / 2.0;
        let height = (w * 1.05).clamp(240.0, 340.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for yours in [false, true] {
                ui.vertical(|ui| {
                    ui.set_width(w);
                    ui.label(
                        RichText::new(if yours { "Yours" } else { "Game's" })
                            .size(12.0)
                            .family(theme::semibold())
                            .color(if yours { ACCENT } else { WEAK }),
                    );
                    let (rect, _) = ui.allocate_exact_size(vec2(w, height), Sense::hover());
                    backdrop(ui, rect);
                    let painter = ui.painter_at(rect);
                    if yours {
                        let drawn = draw_health(&painter, images, style, rect.shrink(20.0), fill);
                        editor(ui, rect, style, &drawn);
                    } else {
                        draw_health(
                            &painter,
                            images,
                            &HealthStyle::default(),
                            rect.shrink(20.0),
                            fill,
                        );
                    }
                    ui.painter().rect_stroke(
                        rect,
                        CornerRadius::same(8),
                        Stroke::new(
                            1.0,
                            if yours {
                                ACCENT.gamma_multiply(0.5)
                            } else {
                                BORDER
                            },
                        ),
                        StrokeKind::Inside,
                    );
                });
            }
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Health").size(12.0).color(WEAK));
            for (label, value) in [("Full", 100), ("Hurt", 55), ("Low", 20)] {
                if widgets::chip(ui, label, ACCENT, Some(pct == value)).clicked() {
                    pct = value;
                }
            }
            ui.spacing_mut().slider_width = (ui.available_width() - 70.0).max(80.0);
            ui.add(egui::Slider::new(&mut pct, 5..=100).suffix("%"));
        });
        ui.data_mut(|d| d.insert_temp(id, pct));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Drag the bar, the number or the regen in Yours to move them.")
                    .size(11.5)
                    .color(WEAK),
            );
            let moved = [style.bar_offset, style.number_offset, style.regen_offset]
                .iter()
                .any(|o| !o.is_zero());
            if moved
                && ui
                    .add(egui::Button::new(RichText::new("Reset positions").size(11.5)).small())
                    .clicked()
            {
                style.bar_offset = Offset::default();
                style.number_offset = Offset::default();
                style.regen_offset = Offset::default();
            }
        });
    });
    ui.add_space(10.0);
    crate::game_shot::in_game(
        ui,
        dt_core::hud::elements::ElementId::HealthAndAmmo,
        vec2(ui.available_width().min(320.0), 150.0),
    );
}

fn part_name(part: Part) -> &'static str {
    match part {
        Part::Bar => "Bar",
        Part::Number => "Number",
        Part::Regen => "Regen",
    }
}

fn offset_of(style: &mut HealthStyle, part: Part) -> &mut Offset {
    match part {
        Part::Bar => &mut style.bar_offset,
        Part::Number => &mut style.number_offset,
        Part::Regen => &mut style.regen_offset,
    }
}

/// Makes each drawn part draggable: hovering outlines it, a drag moves it in its own frame
/// (the number block leans with a tilted bar), measured from where the drag began.
fn editor(ui: &mut Ui, area: Rect, style: &mut HealthStyle, drawn: &Drawn) {
    let start_id = Id::new("health_drag_start");
    for (part, rect, frame) in &drawn.parts {
        let hit = rect.expand(4.0).intersect(area);
        if !hit.is_positive() {
            continue;
        }
        let response = ui
            .interact(hit, Id::new(("health_part", *part)), Sense::drag())
            .on_hover_cursor(egui::CursorIcon::Grab);
        if response.drag_started() {
            let start = *offset_of(style, *part);
            ui.data_mut(|d| d.insert_temp(start_id, (start.x, start.y)));
        }
        if response.dragged()
            && let (Some(origin), Some(now)) = (
                ui.input(|i| i.pointer.press_origin()),
                ui.input(|i| i.pointer.interact_pos()),
            )
        {
            let (x, y): (i16, i16) = ui.data(|d| d.get_temp(start_id)).unwrap_or((0, 0));
            let local = Rot2::from_angle(-frame.angle) * (now - origin) / frame.scale;
            *offset_of(style, *part) =
                Offset::clamped(f32::from(x) + local.x, f32::from(y) + local.y);
        }
        let active = response.dragged();
        if response.hovered() || active {
            let painter = ui.painter_at(area);
            let outline = rect.expand(3.0);
            painter.rect_stroke(
                outline,
                CornerRadius::same(4),
                Stroke::new(if active { 1.5 } else { 1.0 }, ACCENT),
                StrokeKind::Outside,
            );
            let at_now = *offset_of(style, *part);
            let tag = format!("{}  {}, {}", part_name(*part), at_now.x, at_now.y);
            let galley = painter.layout_no_wrap(tag, FontId::proportional(10.5), ON_ACCENT);
            let at = pos2(
                outline.left(),
                (outline.top() - galley.size().y - 4.0).max(area.top() + 2.0),
            );
            painter.rect_filled(
                Rect::from_min_size(at, galley.size() + vec2(8.0, 3.0)),
                CornerRadius::same(3),
                ACCENT,
            );
            painter.galley(at + vec2(4.0, 1.5), galley, ON_ACCENT);
        }
    }
}

/// One inspector line: a label (with its help on hover) and its control on the right.
fn row(ui: &mut Ui, label: &str, help: &str, control: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.set_min_height(26.0);
        ui.label(RichText::new(label).size(12.5).color(TEXT))
            .on_hover_text(help);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.push_id(label, control);
        });
    });
    ui.add_space(2.0);
}

fn switch(ui: &mut Ui, value: &mut bool) {
    if widgets::switch(ui, *value).clicked() {
        *value = !*value;
    }
}

fn pick<T: Copy + PartialEq>(ui: &mut Ui, value: &mut T, all: &[T], label: fn(T) -> &'static str) {
    let labels: Vec<&str> = all.iter().map(|v| label(*v)).collect();
    let current = all.iter().position(|v| v == value).unwrap_or(usize::MAX);
    if let Some(i) = widgets::segmented(ui, &labels, current) {
        *value = all[i];
    }
}

fn section(ui: &mut Ui, title: &str) {
    ui.add_space(10.0);
    widgets::caption(ui, title);
    ui.add_space(2.0);
}

fn inspector(ui: &mut Ui, style: &mut HealthStyle) {
    card(ui, |ui| {
        widgets::caption(ui, "Bar");
        ui.add_space(2.0);
        row(ui, "Shape", "The bar's outline", |ui| {
            pick(ui, &mut style.shape, &BarShape::ALL, BarShape::label)
        });
        let has_bar = style.shape != BarShape::Hidden;
        ui.add_enabled_ui(has_bar, |ui| {
            row(
                ui,
                "Angle",
                "Which way the bar stands; it fills from its bottom or left end",
                |ui| pick(ui, &mut style.angle, &BarAngle::ALL, BarAngle::label),
            );
            row(ui, "Thickness", "How wide the bar is across", |ui| {
                percent(ui, &mut style.thickness_pct, THICKNESS_RANGE)
            });
            row(ui, "Length", "How long the bar is", |ui| {
                percent(ui, &mut style.length_pct, LENGTH_RANGE)
            });
            row(
                ui,
                "Fill",
                "The game's paper texture, or one flat colour",
                |ui| fill_row(ui, &mut style.fill),
            );
            row(ui, "Ticks", "A mark every 250 health", |ui| {
                switch(ui, &mut style.ticks)
            });
            row(ui, "Outline", "The dark green frame round the bar", |ui| {
                switch(ui, &mut style.frame)
            });
        });
        section(ui, "Number");
        let mut shown = !style.hide_number;
        row(
            ui,
            "Show the number",
            "Your current health as a number",
            |ui| switch(ui, &mut shown),
        );
        style.hide_number = !shown;
        ui.add_enabled_ui(shown, |ui| {
            row(ui, "Size", "How big the number reads", |ui| {
                let labels: Vec<String> = NUMBER_SIZES.iter().map(|s| format!("{s}%")).collect();
                let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
                let current = NUMBER_SIZES
                    .iter()
                    .position(|s| *s == style.number_scale_pct)
                    .unwrap_or(usize::MAX);
                if let Some(i) = widgets::segmented(ui, &refs, current) {
                    style.number_scale_pct = NUMBER_SIZES[i];
                }
            });
            row(
                ui,
                "Layout",
                "The game's two lines, or \"3645 / 4557\" on one line",
                |ui| {
                    pick(
                        ui,
                        &mut style.number_layout,
                        &NumberLayout::ALL,
                        NumberLayout::label,
                    )
                },
            );
            row(
                ui,
                "Font",
                "Typefaces from the game's own interface",
                |ui| pick(ui, &mut style.font, &NumberFont::ALL, NumberFont::label),
            );
            row(
                ui,
                "Colour by health",
                "Orange when hurt; the game already turns it red when low. A coloured bar follows.",
                |ui| switch(ui, &mut style.color_by_health),
            );
            row(ui, "Max health", "The \"/ 4557\" under the number", |ui| {
                let current = match (style.hide_max, style.clear_max_health) {
                    (true, _) => 2,
                    (false, true) => 1,
                    (false, false) => 0,
                };
                if let Some(i) = widgets::segmented(ui, &["Faint", "Clear", "Hidden"], current) {
                    style.hide_max = i == 2;
                    style.clear_max_health = i == 1;
                }
            });
            let mut backer = !style.hide_backer;
            row(
                ui,
                "Green backer",
                "The dark green shape behind the number",
                |ui| switch(ui, &mut backer),
            );
            style.hide_backer = !backer;
        });
        section(ui, "Motion and extras");
        row(
            ui,
            "No shaking",
            "Stops the health bar and HUD from shaking and pulsing when you're hurt",
            |ui| switch(ui, &mut style.no_shake),
        );
        let mut regen = !style.hide_regen;
        row(
            ui,
            "Regen number",
            "Your health regeneration per second",
            |ui| switch(ui, &mut regen),
        );
        style.hide_regen = !regen;
        let has_bar = style.shape != BarShape::Hidden;
        ui.add_enabled_ui(regen && has_bar, |ui| {
            row(
                ui,
                "Regen sits",
                "On the bar's top end like the game, or under the number as \"14.8/s\"",
                |ui| {
                    pick(
                        ui,
                        &mut style.regen_place,
                        &RegenPlace::ALL,
                        RegenPlace::label,
                    )
                },
            );
        });
    });
}

fn percent(ui: &mut Ui, value: &mut u16, range: std::ops::RangeInclusive<u16>) {
    ui.spacing_mut().slider_width = 150.0;
    ui.add(egui::Slider::new(value, range).suffix("%").step_by(5.0));
}

fn fill_row(ui: &mut Ui, fill: &mut Option<Rgb>) {
    ui.spacing_mut().item_spacing.x = 4.0;
    let mut rgb = fill.unwrap_or(SWATCHES[0].1).0;
    let before = rgb;
    ui.spacing_mut().interact_size = vec2(26.0, 20.0);
    color_edit_button_srgb(ui, &mut rgb).on_hover_text("Any colour");
    if rgb != before {
        *fill = Some(Rgb(rgb));
    }
    for (name, color) in SWATCHES.iter().rev() {
        if swatch(ui, *color, *fill == Some(*color))
            .on_hover_text(*name)
            .clicked()
        {
            *fill = Some(*color);
        }
    }
    if ui
        .selectable_label(fill.is_none(), RichText::new("Paper").size(12.0))
        .on_hover_text("The game's paper texture")
        .clicked()
    {
        *fill = None;
    }
}

fn swatch(ui: &mut Ui, color: Rgb, on: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
    let [r, g, b] = color.0;
    ui.painter().rect(
        rect.shrink(1.0),
        CornerRadius::same(4),
        Color32::from_rgb(r, g, b),
        Stroke::new(
            if on { 2.0 } else { 1.0 },
            if on {
                ACCENT
            } else if response.hovered() {
                TEXT
            } else {
                BORDER
            },
        ),
        StrokeKind::Outside,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub(crate) fn credits(ui: &mut Ui) {
    ui.add_space(12.0);
    ui.label(
        RichText::new(
            "Ideas from bytenode's Minimal Healthbar Redux (original concept by Gerimboca) and \
             budhud-style Alternate Health Bar (idea by .Kaiz). The wedge uses the game's own \
             bar mask. DeadTune writes its own rules from your game files. A health percentage \
             and a smooth colour fade come later.",
        )
        .size(11.5)
        .color(WEAK),
    );
}
