//! Top bar page (simple-view section, advanced HUD tab): a painted mock of the in-game
//! top bar that follows every option as it changes, presets, and the options in cards.
//! Edits live in `profile.hud.top_bar` (`hud::topbar`) and go out with the normal Apply.

use dt_core::hud::minimap_colors::Color;
use dt_core::hud::topbar::{
    DeadLook, MISSING_OPACITY_RANGE, PORTRAIT_GAP_RANGE, PORTRAIT_SCALE_RANGE, TopBarPreset,
    TopBarStyle, Treatment,
};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Layout, Painter, Pos2, Rect, RichText,
    Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2,
};

use crate::state::{AppState, TopBarPreview};
use crate::theme::{self, ACCENT, RAIL, TEXT, WARN, WEAK};
use crate::widgets;

/// The mock is laid out in bar units, 1400 wide, and scaled to the card.
const BAR_WIDTH: f32 = 1400.0;
const BAR_HEIGHT: f32 = 150.0;
const PORTRAIT_WIDTH: f32 = 88.0;
const CENTRE_WIDTH: f32 = 300.0;
const AMBER: Color32 = Color32::from_rgb(0xD9, 0xA0, 0x54);
const SAPPHIRE: Color32 = Color32::from_rgb(0x4F, 0x8F, 0xD9);
const ALLY_HEALTH: Color32 = Color32::from_rgb(0xFF, 0xEF, 0xD7);
const ENEMY_HEALTH: Color32 = Color32::from_rgb(0xFF, 0x56, 0x56);
const RESPAWN: Color32 = Color32::from_rgb(0xFE, 0x42, 0x0E);
const OFF_WHITE: Color32 = Color32::from_rgb(0xEC, 0xE8, 0xE1);
const ALLIES: [(&str, Color32); 6] = [
    ("Ab", Color32::from_rgb(0x8C, 0x5A, 0x3C)),
    ("Be", Color32::from_rgb(0x4E, 0x7A, 0x8C)),
    ("Dy", Color32::from_rgb(0x6A, 0x5A, 0x9C)),
    ("Ha", Color32::from_rgb(0x8C, 0x3C, 0x5A)),
    ("In", Color32::from_rgb(0xB0, 0x5A, 0x2C)),
    ("Iv", Color32::from_rgb(0x4E, 0x8C, 0x5A)),
];
const ENEMIES: [(&str, Color32); 6] = [
    ("Ke", Color32::from_rgb(0x3C, 0x6A, 0x9C)),
    ("La", Color32::from_rgb(0x9C, 0x4E, 0x3C)),
    ("Mc", Color32::from_rgb(0x6A, 0x6A, 0x3C)),
    ("Pa", Color32::from_rgb(0x5A, 0x3C, 0x8C)),
    ("Se", Color32::from_rgb(0x3C, 0x8C, 0x8C)),
    ("Wr", Color32::from_rgb(0x8C, 0x7A, 0x4E)),
];
const MISSING_ENEMY: usize = 2;
const DEAD_ALLY: usize = 4;
const PURCHASE_ENEMY: usize = 0;

enum Action {
    Set(TopBarStyle),
    Preset(TopBarPreset),
    Reset,
    Preview(TopBarPreview),
}

pub fn page(ui: &mut Ui, state: &mut AppState) {
    crate::hud_view::hud_error(ui, state);
    crate::hud_view::show_layout_note(ui, state, dt_core::hud::elements::ElementId::TopBar);
    let mut actions = Vec::new();
    toolbar(ui, state, &mut actions);
    ui.add_space(6.0);
    mock_card(ui, state, &mut actions);
    ui.add_space(6.0);
    controls(ui, state, &mut actions);
    ui.add_space(4.0);
    widgets::hint(
        ui,
        "Out-of-vision dimming: idea by NA-45 (GameBanana 619963). Spawn timers and urn soul \
         lead: ideas by Stovven (651169) and bonclide (623518). Purchase popups: idea by \
         bonclide. Rebuilt by DeadTune from your game's own stylesheet and layout.",
    );
    for action in actions {
        match action {
            Action::Set(style) => state.set_top_bar(style),
            Action::Preset(preset) => state.apply_top_bar_preset(preset),
            Action::Reset => state.apply_top_bar_preset(TopBarPreset::Vanilla),
            Action::Preview(preview) => state.ui.top_bar_preview = preview,
        }
    }
}

fn toolbar(ui: &mut Ui, state: &AppState, actions: &mut Vec<Action>) {
    ui.horizontal_wrapped(|ui| {
        let current = state.top_bar_preset();
        for preset in TopBarPreset::ALL {
            if ui
                .selectable_label(current == Some(preset), preset.label())
                .on_hover_text(preset.blurb())
                .clicked()
            {
                actions.push(Action::Preset(preset));
            }
        }
        ui.add_space(6.0);
        let n = state.top_bar_changed_count();
        ui.label(
            RichText::new(format!("Changed: {n}"))
                .small()
                .color(if n > 0 { ACCENT } else { WEAK }),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add_enabled(n > 0, egui::Button::new("Reset top bar"))
                .on_hover_text("Back to the game's own top bar")
                .clicked()
            {
                actions.push(Action::Reset);
            }
        });
    });
}

fn mock_card(ui: &mut Ui, state: &AppState, actions: &mut Vec<Action>) {
    let style = &state.profile.hud.top_bar;
    let preview = state.ui.top_bar_preview;
    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            widgets::caption(ui, "Preview");
            widgets::badge(ui, "Experimental, untested in game", WARN);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let mut p = preview;
                ui.label(marked("Dead hero", false));
                if widgets::switch(ui, p.dead_hero).clicked() {
                    p.dead_hero = !p.dead_hero;
                }
                ui.add_space(10.0);
                ui.label(marked("Enemy out of vision", false));
                if widgets::switch(ui, p.missing_enemy).clicked() {
                    p.missing_enemy = !p.missing_enemy;
                }
                if p != preview {
                    actions.push(Action::Preview(p));
                }
            });
        });
        ui.add_space(4.0);
        let width = ui.available_width();
        let height = (width / BAR_WIDTH * BAR_HEIGHT).max(60.0);
        let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        bar(&ui.painter().with_clip_rect(rect), rect, style, preview);
        ui.add_space(2.0);
        widgets::hint(
            ui,
            "A mock-up with stand-in heroes. The game draws the real portraits, names and numbers.",
        );
    });
}

fn controls(ui: &mut Ui, state: &AppState, actions: &mut Vec<Action>) {
    let current = &state.profile.hud.top_bar;
    let mut s = current.clone();
    if ui.available_width() >= 760.0 {
        ui.columns(2, |cols| {
            missing_card(&mut cols[0], &mut s);
            cols[0].add_space(6.0);
            portraits_card(&mut cols[0], &mut s);
            centre_card(&mut cols[1], &mut s);
            cols[1].add_space(6.0);
            colours_card(&mut cols[1], &mut s);
            cols[1].add_space(6.0);
            extras_card(&mut cols[1], &mut s);
        });
    } else {
        missing_card(ui, &mut s);
        ui.add_space(6.0);
        portraits_card(ui, &mut s);
        ui.add_space(6.0);
        colours_card(ui, &mut s);
        ui.add_space(6.0);
        centre_card(ui, &mut s);
        ui.add_space(6.0);
        extras_card(ui, &mut s);
    }
    if s != *current {
        actions.push(Action::Set(s));
    }
}

fn missing_card(ui: &mut Ui, s: &mut TopBarStyle) {
    let d = TopBarStyle::default();
    card(ui, "Enemies out of vision", Some("Idea by NA-45"), |ui| {
        widgets::hint(
            ui,
            "The game only drops the health bar when an enemy leaves your team's vision. \
             These make the whole portrait show it.",
        );
        ui.add_space(4.0);
        slider_row(
            ui,
            "Fade to",
            &mut s.missing_opacity_pct,
            MISSING_OPACITY_RANGE,
            5,
            "%",
            d.missing_opacity_pct,
        );
        switch_row(
            ui,
            "Desaturate",
            &mut s.missing_desaturate,
            d.missing_desaturate,
        );
        switch_row(ui, "Darken", &mut s.missing_darken, d.missing_darken);
    });
}

fn portraits_card(ui: &mut Ui, s: &mut TopBarStyle) {
    let d = TopBarStyle::default();
    card(ui, "Portraits", None, |ui| {
        slider_row(
            ui,
            "Size",
            &mut s.portrait_scale_pct,
            PORTRAIT_SCALE_RANGE,
            5,
            "%",
            d.portrait_scale_pct,
        );
        slider_row(
            ui,
            "Gap",
            &mut s.portrait_gap_px,
            PORTRAIT_GAP_RANGE,
            2,
            "px",
            d.portrait_gap_px,
        );
        choice_row(
            ui,
            "Dead heroes",
            &["Game's", "Grey", "Dark", "Faded"],
            &[
                DeadLook::Vanilla,
                DeadLook::Grayscale,
                DeadLook::Darken,
                DeadLook::Faded,
            ],
            &mut s.dead,
            d.dead,
        );
        switch_row(ui, "Always show levels", &mut s.show_levels, d.show_levels);
        switch_row(
            ui,
            "Hide souls tags",
            &mut s.hide_player_souls,
            d.hide_player_souls,
        );
    });
}

fn colours_card(ui: &mut Ui, s: &mut TopBarStyle) {
    card(ui, "Team colours", None, |ui| {
        widgets::hint(
            ui,
            "Health bars and souls tags of each side. With the game's own enemy colour \
             (Minimap page) turned on, it may win for enemies; untested.",
        );
        ui.add_space(4.0);
        color_row(ui, "Allies", &mut s.ally_color, ALLY_HEALTH);
        color_row(ui, "Enemies", &mut s.enemy_color, ENEMY_HEALTH);
    });
}

fn centre_card(ui: &mut Ui, s: &mut TopBarStyle) {
    let d = TopBarStyle::default();
    let labels = ["Game's", "Compact", "Hidden"];
    let values = [Treatment::Vanilla, Treatment::Compact, Treatment::Hidden];
    card(ui, "Clock and scores", None, |ui| {
        choice_row(ui, "Game clock", &labels, &values, &mut s.clock, d.clock);
        choice_row(
            ui,
            "Soul lead",
            &labels,
            &values,
            &mut s.soul_lead,
            d.soul_lead,
        );
        choice_row(
            ui,
            "Rejuvenator icons",
            &labels,
            &values,
            &mut s.rejuv_charges,
            d.rejuv_charges,
        );
        switch_row(
            ui,
            "Hide kill rows on the scoreboard",
            &mut s.hide_kill_counts,
            d.hide_kill_counts,
        );
    });
}

fn extras_card(ui: &mut Ui, s: &mut TopBarStyle) {
    let d = TopBarStyle::default();
    card(
        ui,
        "Extras",
        Some("Adds a small script to the top bar"),
        |ui| {
            widgets::hint(
                ui,
                "Each one reads what the bar already shows: the game clock, the team souls and \
             the shop's recent purchases.",
            );
            ui.add_space(4.0);
            switch_row(
                ui,
                "Spawn timers (powerups and rejuvenator)",
                &mut s.spawn_timers,
                d.spawn_timers,
            );
            switch_row(ui, "Urn soul lead", &mut s.urn_lead, d.urn_lead);
            switch_row(
                ui,
                "Purchase popups under portraits",
                &mut s.purchases,
                d.purchases,
            );
        },
    );
}

fn card(ui: &mut Ui, title: &str, badge: Option<&str>, add: impl FnOnce(&mut Ui)) {
    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            widgets::caption(ui, title);
            if let Some(text) = badge {
                widgets::badge(ui, text, WEAK);
            }
        });
        ui.add_space(2.0);
        add(ui);
    });
}

fn marked(text: &str, changed: bool) -> RichText {
    RichText::new(text)
        .size(12.5)
        .color(if changed { ACCENT } else { TEXT })
}

fn label_cell(ui: &mut Ui, text: &str, changed: bool) {
    ui.allocate_ui_with_layout(
        vec2(140.0, 20.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.set_min_width(140.0);
            ui.add(egui::Label::new(marked(text, changed)).truncate());
        },
    );
}

fn slider_row<T>(
    ui: &mut Ui,
    label: &str,
    value: &mut T,
    range: std::ops::RangeInclusive<T>,
    step: u8,
    unit: &str,
    default: T,
) where
    T: egui::emath::Numeric + PartialEq + Copy,
{
    let changed = *value != default;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        label_cell(ui, label, changed);
        let readout = 44.0;
        let reset = if changed { 54.0 } else { 0.0 };
        ui.spacing_mut().slider_width = (ui.available_width() - readout - reset - 16.0).max(60.0);
        ui.add(
            egui::Slider::new(value, range)
                .show_value(false)
                .step_by(f64::from(step)),
        );
        ui.label(
            RichText::new(format!("{}{unit}", value.to_f64().round()))
                .size(12.5)
                .color(if changed { ACCENT } else { TEXT }),
        );
        if changed
            && widgets::reset_pill(ui)
                .on_hover_text("Back to the game's value")
                .clicked()
        {
            *value = default;
        }
    });
}

fn switch_row(ui: &mut Ui, label: &str, value: &mut bool, default: bool) {
    let changed = *value != default;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if widgets::switch(ui, *value).clicked() {
            *value = !*value;
        }
        ui.label(marked(label, changed));
        if changed
            && widgets::reset_pill(ui)
                .on_hover_text("Back to the game's value")
                .clicked()
        {
            *value = default;
        }
    });
}

fn choice_row<T: Copy + PartialEq>(
    ui: &mut Ui,
    label: &str,
    labels: &[&str],
    values: &[T],
    value: &mut T,
    default: T,
) {
    let changed = *value != default;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        label_cell(ui, label, changed);
        let selected = values.iter().position(|v| *v == *value).unwrap_or(0);
        if let Some(i) = ui
            .push_id(label, |ui| widgets::segmented(ui, labels, selected))
            .inner
        {
            *value = values[i];
        }
        if changed
            && widgets::reset_pill(ui)
                .on_hover_text("Back to the game's value")
                .clicked()
        {
            *value = default;
        }
    });
}

fn color_row(ui: &mut Ui, label: &str, value: &mut Option<Color>, vanilla: Color32) {
    let changed = value.is_some();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        label_cell(ui, label, changed);
        let before = value.map_or(vanilla, to_color32);
        let [r, g, b, _] = before.to_array();
        let mut rgb = [r, g, b];
        ui.spacing_mut().interact_size = vec2(36.0, 18.0);
        egui::widgets::color_picker::color_edit_button_srgb(ui, &mut rgb);
        if rgb != [r, g, b] {
            *value = Some(Color([rgb[0], rgb[1], rgb[2], 255]));
        }
        let font = FontId::proportional(10.5);
        let galley = ui.painter().layout_no_wrap("default".into(), font, WEAK);
        let (rect, response) =
            ui.allocate_exact_size(vec2(galley.size().x + 22.0, 16.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, CornerRadius::same(255), RAIL);
        p.circle_filled(pos2(rect.left() + 9.0, rect.center().y), 4.0, vanilla);
        p.galley(
            pos2(rect.left() + 16.0, rect.center().y - galley.size().y / 2.0),
            galley,
            WEAK,
        );
        response.on_hover_text("The game's colour");
        if changed
            && widgets::reset_pill(ui)
                .on_hover_text("Back to the game's colour")
                .clicked()
        {
            *value = None;
        }
    });
}

fn to_color32(c: Color) -> Color32 {
    let [r, g, b, a] = c.0;
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// How one portrait is drawn: a fade, grey, and a dark wash, from the style's options
/// for the state the hero is in.
#[derive(Clone, Copy)]
struct Look {
    alpha: f32,
    gray: bool,
    wash: f32,
}

impl Look {
    const PLAIN: Look = Look {
        alpha: 1.0,
        gray: false,
        wash: 0.0,
    };

    fn missing(style: &TopBarStyle) -> Look {
        Look {
            alpha: f32::from(style.missing_opacity_pct) / 100.0,
            gray: style.missing_desaturate,
            wash: if style.missing_darken { 0.56 } else { 0.0 },
        }
    }

    fn dead(style: &TopBarStyle) -> Look {
        match style.dead {
            DeadLook::Vanilla => Look::PLAIN,
            DeadLook::Grayscale => Look {
                gray: true,
                ..Look::PLAIN
            },
            DeadLook::Darken => Look {
                wash: 0.63,
                ..Look::PLAIN
            },
            DeadLook::Faded => Look {
                alpha: 0.45,
                ..Look::PLAIN
            },
        }
    }

    fn apply(self, c: Color32) -> Color32 {
        let [r, g, b, a] = c.to_array();
        let (r, g, b) = if self.gray {
            let l = (0.3 * f32::from(r) + 0.59 * f32::from(g) + 0.11 * f32::from(b)) as u8;
            (l, l, l)
        } else {
            (r, g, b)
        };
        let dark = |v: u8| (f32::from(v) * (1.0 - self.wash)) as u8;
        Color32::from_rgba_unmultiplied(dark(r), dark(g), dark(b), a).gamma_multiply(self.alpha)
    }
}

/// Paints the mock into `rect`. Deterministic for a style and preview.
fn bar(p: &Painter, rect: Rect, style: &TopBarStyle, preview: TopBarPreview) {
    let k = rect.width() / BAR_WIDTH;
    let at = |x: f32, y: f32| pos2(rect.left() + x * k, rect.top() + y * k);
    p.rect_filled(rect, CornerRadius::same(4), Color32::from_rgb(28, 30, 34));
    let band = Rect::from_min_max(rect.min, at(BAR_WIDTH, 60.0));
    p.rect_filled(
        band,
        CornerRadius::ZERO,
        Color32::from_rgba_unmultiplied(0, 0, 0, 70),
    );

    let ps = f32::from(style.portrait_scale_pct) / 100.0;
    let cell = PORTRAIT_WIDTH * ps + f32::from(style.portrait_gap_px) * 2.0;
    let centre = BAR_WIDTH / 2.0;
    for (i, (name, hue)) in ALLIES.iter().enumerate() {
        let x = centre - CENTRE_WIDTH / 2.0 - cell * (ALLIES.len() - i) as f32 + cell / 2.0;
        let dead = preview.dead_hero && i == DEAD_ALLY;
        let look = if dead { Look::dead(style) } else { Look::PLAIN };
        portrait(p, at, k, x, name, *hue, false, style, look, dead, true);
    }
    for (i, (name, hue)) in ENEMIES.iter().enumerate() {
        let x = centre + CENTRE_WIDTH / 2.0 + cell * i as f32 + cell / 2.0;
        let missing = preview.missing_enemy && i == MISSING_ENEMY;
        let look = if missing {
            Look::missing(style)
        } else {
            Look::PLAIN
        };
        portrait(p, at, k, x, name, *hue, true, style, look, false, !missing);
        if style.purchases && i == PURCHASE_ENEMY {
            purchase_popup(p, at, k, x, ps);
        }
    }
    centre_block(p, at, k, style);
}

#[allow(clippy::too_many_arguments)]
fn portrait(
    p: &Painter,
    at: impl Fn(f32, f32) -> Pos2,
    k: f32,
    x: f32,
    name: &str,
    hue: Color32,
    enemy: bool,
    style: &TopBarStyle,
    look: Look,
    dead: bool,
    health_visible: bool,
) {
    let ps = f32::from(style.portrait_scale_pct) / 100.0;
    let radius = 34.0 * ps * k;
    let centre = at(x, 58.0 * ps + 4.0);
    let team = if enemy { SAPPHIRE } else { AMBER };
    let health_color = if enemy {
        style.enemy_color.map_or(ENEMY_HEALTH, to_color32)
    } else {
        style.ally_color.map_or(ALLY_HEALTH, to_color32)
    };

    let coin = if dead { Color32::from_gray(40) } else { hue };
    p.circle_filled(centre, radius, look.apply(coin));
    p.circle_stroke(
        centre,
        radius,
        Stroke::new(2.0 * k, look.apply(team.gamma_multiply(0.9))),
    );
    let initials = if dead { "" } else { name };
    p.text(
        centre,
        Align2::CENTER_CENTER,
        initials,
        FontId::new((20.0 * ps * k).max(6.0), theme::semibold()),
        look.apply(OFF_WHITE),
    );
    if dead {
        p.text(
            centre + vec2(0.0, 2.0 * k),
            Align2::CENTER_CENTER,
            "23",
            FontId::new((26.0 * ps * k).max(6.0), theme::semibold()),
            RESPAWN,
        );
    }

    if health_visible && !dead {
        let w = 10.0 * ps * k;
        let h = 44.0 * ps * k;
        let side = if enemy { 1.0 } else { -1.0 };
        let bx = centre.x + side * (radius + 7.0 * ps * k);
        let bar = Rect::from_center_size(pos2(bx, centre.y), vec2(w, h));
        p.rect_filled(
            bar,
            CornerRadius::same(2),
            look.apply(Color32::from_rgb(12, 12, 14)),
        );
        let fill = Rect::from_min_max(pos2(bar.left(), bar.top() + h * 0.3), bar.max);
        p.rect_filled(fill, CornerRadius::same(2), look.apply(health_color));
    }

    let dot = pos2(centre.x + radius * 0.75, centre.y - radius * 0.75);
    p.circle_filled(dot, 6.0 * ps * k, look.apply(Color32::from_rgb(10, 10, 12)));
    p.circle_filled(dot, 4.0 * ps * k, look.apply(team));

    if style.show_levels {
        let badge = pos2(centre.x, centre.y + radius - 2.0 * k);
        p.circle_filled(
            badge,
            8.0 * ps * k,
            look.apply(Color32::from_rgb(14, 14, 16)),
        );
        p.circle_stroke(
            badge,
            8.0 * ps * k,
            Stroke::new(1.0 * k, look.apply(OFF_WHITE)),
        );
        p.text(
            badge,
            Align2::CENTER_CENTER,
            "14",
            FontId::proportional((9.0 * ps * k).max(5.0)),
            look.apply(OFF_WHITE),
        );
    }

    if !style.hide_player_souls {
        let tag = Rect::from_center_size(
            pos2(centre.x, centre.y + radius + 12.0 * ps * k),
            vec2(50.0 * ps * k, 14.0 * ps * k),
        );
        let tag_look = if dead {
            Look {
                alpha: look.alpha * 0.3,
                ..look
            }
        } else {
            look
        };
        p.rect_filled(tag, CornerRadius::same(3), tag_look.apply(team));
        p.text(
            tag.center(),
            Align2::CENTER_CENTER,
            "12.4k",
            FontId::new((10.0 * ps * k).max(5.0), theme::semibold()),
            tag_look.apply(Color32::from_rgb(20, 18, 14)),
        );
    }
}

fn purchase_popup(p: &Painter, at: impl Fn(f32, f32) -> Pos2, k: f32, x: f32, ps: f32) {
    let rect = Rect::from_center_size(at(x, 118.0 * ps + 10.0), vec2(84.0 * k, 16.0 * k));
    p.rect_filled(
        rect,
        CornerRadius::same(3),
        Color32::from_rgba_unmultiplied(27, 27, 27, 224),
    );
    p.rect_stroke(
        rect,
        CornerRadius::same(3),
        Stroke::new(1.0 * k, Color32::from_rgb(0xE7, 0xA1, 0x4F)),
        StrokeKind::Inside,
    );
    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        "Monster Rounds",
        FontId::proportional((10.0 * k).max(5.0)),
        OFF_WHITE,
    );
}

fn centre_block(p: &Painter, at: impl Fn(f32, f32) -> Pos2, k: f32, style: &TopBarStyle) {
    let cx = BAR_WIDTH / 2.0;
    let clock_scale = match style.clock {
        Treatment::Vanilla => 1.0,
        Treatment::Compact => 0.8,
        Treatment::Hidden => 0.0,
    };
    if clock_scale > 0.0 {
        let rect = Rect::from_center_size(
            at(cx, 19.0),
            vec2(84.0 * clock_scale * k, 24.0 * clock_scale * k),
        );
        p.rect_filled(
            rect,
            CornerRadius::same(4),
            Color32::from_rgba_unmultiplied(0, 0, 0, 144),
        );
        p.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "27:42",
            FontId::new((14.0 * clock_scale * k).max(5.0), theme::semibold()),
            OFF_WHITE,
        );
    }
    if style.spawn_timers {
        for (i, (text, soon)) in [("3:18", false), ("0:24", true)].into_iter().enumerate() {
            let x = cx + (i as f32 - 0.5) * 78.0;
            let rect = Rect::from_center_size(at(x, 46.0), vec2(70.0 * k, 18.0 * k));
            p.rect_filled(
                rect,
                CornerRadius::same(4),
                Color32::from_rgba_unmultiplied(0, 0, 0, 144),
            );
            let icon = pos2(rect.left() + 11.0 * k, rect.center().y);
            if i == 0 {
                let s = 5.0 * k;
                p.add(Shape::convex_polygon(
                    vec![
                        icon + vec2(0.0, -s),
                        icon + vec2(s, 0.0),
                        icon + vec2(0.0, s),
                        icon + vec2(-s, 0.0),
                    ],
                    OFF_WHITE.gamma_multiply(0.8),
                    Stroke::NONE,
                ));
            } else {
                p.circle_stroke(
                    icon,
                    4.5 * k,
                    Stroke::new(1.5 * k, OFF_WHITE.gamma_multiply(0.8)),
                );
            }
            p.text(
                pos2(rect.left() + 22.0 * k, rect.center().y),
                Align2::LEFT_CENTER,
                text,
                FontId::new((12.0 * k).max(5.0), theme::semibold()),
                if soon {
                    ACCENT
                } else {
                    OFF_WHITE.gamma_multiply(0.9)
                },
            );
        }
    }
    let lead_scale = match style.soul_lead {
        Treatment::Vanilla => 1.0,
        Treatment::Compact => 0.75,
        Treatment::Hidden => 0.0,
    };
    if lead_scale > 0.0 {
        let y = 74.0;
        let font = FontId::new((22.0 * lead_scale * k).max(6.0), theme::semibold());
        let dx = 78.0 * lead_scale;
        p.text(
            at(cx - dx, y),
            Align2::CENTER_CENTER,
            "41.2k",
            font.clone(),
            OFF_WHITE,
        );
        p.text(
            at(cx + dx, y),
            Align2::CENTER_CENTER,
            "38.9k",
            font,
            OFF_WHITE,
        );
        let icon = 7.0 * lead_scale * k;
        let spread = if style.urn_lead { 40.0 } else { 28.0 };
        p.circle_filled(at(cx - spread * lead_scale, y), icon, AMBER);
        p.circle_filled(at(cx + spread * lead_scale, y), icon, SAPPHIRE);
        if style.urn_lead {
            let rect = Rect::from_center_size(at(cx, y), vec2(48.0 * k, 20.0 * k));
            p.rect_filled(
                rect,
                CornerRadius::same(4),
                Color32::from_rgba_unmultiplied(0, 0, 0, 144),
            );
            p.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "+6%",
                FontId::new((13.0 * k).max(5.0), theme::semibold()),
                OFF_WHITE,
            );
        }
    }
    let rejuv_scale = match style.rejuv_charges {
        Treatment::Vanilla => 1.0,
        Treatment::Compact => 0.8,
        Treatment::Hidden => 0.0,
    };
    if rejuv_scale > 0.0 {
        let y = 112.0;
        for (dx, color) in [(-56.0, AMBER), (56.0, SAPPHIRE)] {
            let c = at(cx + dx * rejuv_scale, y);
            let s = 9.0 * rejuv_scale * k;
            p.add(Shape::convex_polygon(
                vec![
                    c + vec2(0.0, -s),
                    c + vec2(s, s * 0.8),
                    c + vec2(-s, s * 0.8),
                ],
                color.gamma_multiply(0.85),
                Stroke::NONE,
            ));
        }
        p.circle_stroke(
            at(cx, y),
            10.0 * rejuv_scale * k,
            Stroke::new(2.0 * k, OFF_WHITE.gamma_multiply(0.35)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        ctx
    }

    fn render(state: &mut AppState, width: f32) -> usize {
        let ctx = context();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 900.0))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| page(ui, state));
        output.textures_delta.clear();
        output.shapes.len()
    }

    #[test]
    fn page_renders_wide_and_narrow_without_editing() {
        let (_dir, mut state) = crate::state::testutil::state();
        for width in [1200.0, 500.0] {
            render(&mut state, width);
        }
        assert!(!state.is_dirty(), "drawing alone changes nothing");
        state.apply_top_bar_preset(TopBarPreset::TopBarPlus);
        render(&mut state, 1200.0);
        assert_eq!(state.top_bar_preset(), Some(TopBarPreset::TopBarPlus));
    }

    #[test]
    fn mock_follows_the_options() {
        let ctx = context();
        let count = |style: &TopBarStyle, preview: TopBarPreview| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 300.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let rect = Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 130.0));
                bar(ui.painter(), rect, style, preview);
            });
            output.textures_delta.clear();
            output.shapes.len()
        };
        let vanilla = TopBarStyle::default();
        let shown = TopBarPreview::default();
        let base = count(&vanilla, shown);
        let hidden = TopBarStyle {
            clock: Treatment::Hidden,
            soul_lead: Treatment::Hidden,
            rejuv_charges: Treatment::Hidden,
            hide_player_souls: true,
            ..vanilla.clone()
        };
        assert!(count(&hidden, shown) < base, "hidden elements draw nothing");
        let extras = TopBarPreset::TopBarPlus.style();
        assert!(count(&extras, shown) > base, "extras draw chips");
        let none = TopBarPreview {
            missing_enemy: false,
            dead_hero: false,
        };
        assert!(
            count(&vanilla, none) >= base,
            "everyone in vision draws health bars"
        );
    }

    #[test]
    fn looks_fade_grey_and_darken() {
        let c = Color32::from_rgb(200, 100, 50);
        assert_eq!(Look::PLAIN.apply(c), c);
        let faded = Look::missing(&TopBarStyle {
            missing_opacity_pct: 20,
            ..TopBarStyle::default()
        })
        .apply(c);
        assert!(faded.a() < c.a());
        let grey = Look::missing(&TopBarStyle {
            missing_desaturate: true,
            ..TopBarStyle::default()
        })
        .apply(c);
        assert_eq!((grey.r(), grey.g()), (grey.g(), grey.b()));
        let dark = Look::dead(&TopBarStyle {
            dead: DeadLook::Darken,
            ..TopBarStyle::default()
        })
        .apply(c);
        assert!(dark.r() < c.r() && dark.a() == 255);
    }
}
