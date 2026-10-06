//! Top bar page (simple-view section, advanced HUD tab): a preview of the in-game top bar,
//! drawn from the game's portraits and icons when they load and painted shapes otherwise,
//! that follows every option as it changes; presets, and the options in cards.
//! Edits live in `profile.hud.top_bar` (`hud::topbar`) and go out with the normal Apply.

use dt_core::hud::art::{self, Art};
use dt_core::hud::minimap_colors::Color;
use dt_core::hud::topbar::{
    DeadLook, MISSING_OPACITY_RANGE, PORTRAIT_GAP_RANGE, PORTRAIT_SCALE_RANGE, TopBarPreset,
    TopBarStyle, Treatment,
};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Layout, Painter, Pos2, Rect, RichText,
    Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2,
};

use crate::hud_art::Images;
use crate::state::{AppState, TopBarPreview};
use crate::theme::{self, ACCENT, BORDER, CARD, CARD_HOVER, RAIL, TEXT, WARN, WEAK};
use crate::widgets;

/// The mock is laid out in bar units (1080p px): `#TeamsContainer` is 1260 wide, six 88 px
/// player panels a side, and scaled to the card.
const BAR_WIDTH: f32 = 1260.0;
const BAR_HEIGHT: f32 = 150.0;
const POPUP_HEIGHT: f32 = 28.0;
const PORTRAIT_WIDTH: f32 = 88.0;
/// `.TeamNetworth`; the panels squeeze it to the 204 px left between them, so the team
/// souls run under the nearest portraits.
const CENTRE_WIDTH: f32 = 300.0;
const DISC: f32 = 70.0;
const DISC_BOTTOM: f32 = 100.0;
const AMBER: Color32 = Color32::from_rgb(0xD4, 0x86, 0x0B);
const SAPPHIRE: Color32 = Color32::from_rgb(0x4D, 0x75, 0xC3);
const AMBER_TEXT: Color32 = Color32::from_rgb(0x20, 0x15, 0x00);
const OFF_WHITE: Color32 = Color32::from_rgb(0xFF, 0xEF, 0xD7);
const OFF_BLACK: Color32 = Color32::from_rgb(0x10, 0x13, 0x0D);
const ALLY_HEALTH: Color32 = OFF_WHITE;
const ENEMY_HEALTH: Color32 = Color32::from_rgb(0xFF, 0x56, 0x56);
const RESPAWN: Color32 = Color32::from_rgb(0xFE, 0x42, 0x0E);
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
const HEALTH: [f32; 6] = [0.7, 1.0, 0.85, 0.5, 1.0, 0.9];
const SOULS: [&str; 6] = ["38", "35", "33", "35", "45", "35"];
const ULTIMATE_READY: usize = 2;
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
    header(ui, state, &mut actions);
    ui.add_space(8.0);
    let ctx = ui.ctx().clone();
    crate::hud_art::with(&ctx, state, |state, images| {
        gallery(ui, state, images, &mut actions);
        ui.add_space(14.0);
        compare(ui, state, images, &mut actions);
    });
    ui.add_space(14.0);
    controls(ui, state, &mut actions);
    ui.add_space(10.0);
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

fn header(ui: &mut Ui, state: &AppState, actions: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Top bar style")
                .size(15.0)
                .family(theme::semibold())
                .color(TEXT),
        );
        widgets::badge(ui, "Experimental, untested in game", WARN);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let n = state.top_bar_changed_count();
            if n > 0
                && widgets::reset_pill(ui)
                    .on_hover_text("Back to the game's own top bar")
                    .clicked()
            {
                actions.push(Action::Reset);
            }
            let text = match state.top_bar_preset() {
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

/// The bar's middle, where most options show: the last three allies (one of them dead),
/// the centre and the first three enemies (one out of vision, one buying).
const CROP: (f32, f32) = (300.0, 960.0);

/// Draws the middle of the bar for `style` across `stage`, over the game.
fn bar_crop(
    ui: &Ui,
    stage: Rect,
    style: &TopBarStyle,
    preview: TopBarPreview,
    images: &mut Images,
) -> bool {
    crate::game_shot::backdrop(ui, stage);
    let span = extent(style).x;
    let width = CROP.1 - CROP.0 + span - BAR_WIDTH;
    let k = stage.width() / width;
    let size = extent(style) * k;
    let rect = Rect::from_min_size(
        pos2(stage.left() - CROP.0 * k, stage.center().y - size.y / 2.0),
        size,
    );
    bar(&ui.painter_at(stage), rect, style, preview, images)
}

fn gallery(ui: &mut Ui, state: &AppState, images: &mut Images, actions: &mut Vec<Action>) {
    let current = state.top_bar_preset();
    let gap = 10.0;
    let columns = if ui.available_width() > 640.0 { 3 } else { 2 };
    let width = (ui.available_width() - gap * (columns - 1) as f32) / columns as f32;
    let preview = TopBarPreview {
        missing_enemy: true,
        dead_hero: true,
    };
    for row in TopBarPreset::ALL.chunks(columns) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for preset in row {
                let selected = current == Some(*preset);
                let stage_height = ((width - 12.0) / (CROP.1 - CROP.0) * BAR_HEIGHT).min(96.0);
                let (rect, response) =
                    ui.allocate_exact_size(vec2(width, stage_height + 32.0), Sense::click());
                ui.painter().rect(
                    rect,
                    CornerRadius::same(10),
                    if response.hovered() { CARD_HOVER } else { CARD },
                    Stroke::new(
                        if selected { 2.0 } else { 1.0 },
                        if selected { ACCENT } else { BORDER },
                    ),
                    StrokeKind::Inside,
                );
                let stage = Rect::from_min_size(
                    rect.min + vec2(6.0, 6.0),
                    vec2(rect.width() - 12.0, stage_height),
                );
                bar_crop(ui, stage, &preset.style(), preview, images);
                let painter = ui.painter();
                painter.text(
                    pos2(rect.left() + 11.0, stage.bottom() + 13.0),
                    Align2::LEFT_CENTER,
                    preset.label(),
                    FontId::new(12.5, theme::semibold()),
                    if selected { ACCENT } else { TEXT },
                );
                if selected {
                    let c = pos2(rect.right() - 18.0, rect.top() + 18.0);
                    painter.circle_filled(c, 8.0, ACCENT);
                    crate::icons::paint(
                        painter,
                        Rect::from_center_size(c, vec2(11.0, 11.0)),
                        crate::icons::Icon::Check,
                        theme::ON_ACCENT,
                    );
                }
                if response
                    .on_hover_text(preset.blurb())
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    actions.push(Action::Preset(*preset));
                }
            }
        });
        ui.add_space(8.0);
    }
}

/// The game's bar and yours, whole and stacked, with the states the options act on.
fn compare(ui: &mut Ui, state: &AppState, images: &mut Images, actions: &mut Vec<Action>) {
    let style = &state.profile.hud.top_bar;
    let preview = state.ui.top_bar_preview;
    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Show").size(12.0).color(WEAK));
            let mut p = preview;
            if widgets::chip(ui, "An enemy out of vision", ACCENT, Some(p.missing_enemy)).clicked()
            {
                p.missing_enemy = !p.missing_enemy;
            }
            if widgets::chip(ui, "A dead ally", ACCENT, Some(p.dead_hero)).clicked() {
                p.dead_hero = !p.dead_hero;
            }
            if p != preview {
                actions.push(Action::Preview(p));
            }
        });
        ui.add_space(8.0);
        let width = ui.available_width();
        let mut real = false;
        for (label, look, yours) in [
            ("Game's", TopBarStyle::default(), false),
            ("Yours", style.clone(), true),
        ] {
            ui.label(
                RichText::new(label)
                    .size(12.0)
                    .family(theme::semibold())
                    .color(if yours { ACCENT } else { WEAK }),
            );
            let size = extent(&look);
            let height = (width / size.x * size.y).max(60.0) + 16.0;
            let (stage, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
            crate::game_shot::backdrop(ui, stage);
            let inner = stage.shrink2(vec2(0.0, 8.0));
            real |= bar(&ui.painter_at(stage), inner, &look, preview, images);
            ui.painter().rect_stroke(
                stage,
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
            ui.add_space(8.0);
        }
        widgets::hint(
            ui,
            if real {
                "Your game's portraits and icons; the heroes and numbers are examples."
            } else {
                "A mock-up with stand-in heroes. The game draws the real portraits, names and numbers."
            },
        );
    });
    ui.add_space(6.0);
    egui::CollapsingHeader::new(
        RichText::new("Compare with a screenshot from the game")
            .size(12.0)
            .color(WEAK),
    )
    .id_salt("top_bar_game_shot")
    .default_open(false)
    .show(ui, |ui| {
        let width = ui.available_width();
        crate::game_shot::in_game(
            ui,
            dt_core::hud::elements::ElementId::TopBar,
            vec2(width, width * 0.12),
        );
    });
}

fn controls(ui: &mut Ui, state: &AppState, actions: &mut Vec<Action>) {
    let current = &state.profile.hud.top_bar;
    let mut s = current.clone();
    if ui.available_width() >= 760.0 {
        ui.columns(2, |cols| {
            inspector_left(&mut cols[0], &mut s);
            inspector_right(&mut cols[1], &mut s);
        });
    } else {
        inspector_left(ui, &mut s);
        ui.add_space(10.0);
        inspector_right(ui, &mut s);
    }
    if s != *current {
        actions.push(Action::Set(s));
    }
}

fn section(ui: &mut Ui, title: &str, note: Option<&str>) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        widgets::caption(ui, title);
        if let Some(note) = note {
            widgets::badge(ui, note, WEAK);
        }
    });
    ui.add_space(2.0);
}

fn inspector_left(ui: &mut Ui, s: &mut TopBarStyle) {
    let d = TopBarStyle::default();
    widgets::card(ui, |ui| {
        section(ui, "Enemies out of vision", Some("Idea by NA-45"));
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
        section(ui, "Portraits", None);
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

fn inspector_right(ui: &mut Ui, s: &mut TopBarStyle) {
    let d = TopBarStyle::default();
    let labels = ["Game's", "Compact", "Hidden"];
    let values = [Treatment::Vanilla, Treatment::Compact, Treatment::Hidden];
    widgets::card(ui, |ui| {
        section(ui, "Clock and scores", None);
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
        section(ui, "Team colours", None);
        widgets::hint(
            ui,
            "Health bars and souls tags of each side. With the game's own enemy colour \
             (Minimap page) turned on, it may win for enemies; untested.",
        );
        ui.add_space(4.0);
        color_row(ui, "Allies", &mut s.ally_color, ALLY_HEALTH);
        color_row(ui, "Enemies", &mut s.enemy_color, ENEMY_HEALTH);
        section(ui, "Extras", Some("Adds a small script to the top bar"));
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

/// Bar units one portrait panel takes, its gap included.
fn cell(style: &TopBarStyle) -> f32 {
    PORTRAIT_WIDTH * f32::from(style.portrait_scale_pct) / 100.0 + f32::from(style.portrait_gap_px)
}

/// The mock's size in bar units: the game's 1260x150, wider when bigger or spaced-out
/// portraits would crowd the centre, taller for bigger portraits and the purchase popup.
fn extent(style: &TopBarStyle) -> egui::Vec2 {
    let ps = f32::from(style.portrait_scale_pct) / 100.0;
    let free = BAR_WIDTH - 12.0 * PORTRAIT_WIDTH;
    let width = (12.0 * cell(style) + free).max(BAR_WIDTH);
    let popup = if style.purchases { POPUP_HEIGHT } else { 0.0 };
    vec2(width, (BAR_HEIGHT + popup) * ps.max(1.0))
}

/// Left edge of player panel `i` (0 is the outermost), allies from the bar's left edge
/// inward, enemies from its right edge, so the centre keeps what is left.
fn panel_left(style: &TopBarStyle, i: usize, enemy: bool) -> f32 {
    let gap = f32::from(style.portrait_gap_px);
    if enemy {
        extent(style).x - (6 - i) as f32 * cell(style) + gap
    } else {
        i as f32 * cell(style)
    }
}

/// `healthbar_backer_vert_mask` over its rect, as fractions: wide at the top, a narrow
/// tip at the bottom.
const HEALTH_SHAPE: [(f32, f32); 4] = [(0.99, 0.0), (0.46, 0.98), (0.04, 0.98), (0.0, 0.02)];

/// Paints the preview into `rect`. Deterministic for a style, preview and the pictures
/// ready; true when the portraits are the game's.
fn bar(
    p: &Painter,
    rect: Rect,
    style: &TopBarStyle,
    preview: TopBarPreview,
    images: &mut Images,
) -> bool {
    let span = extent(style).x;
    let k = rect.width() / span;
    let at = |x: f32, y: f32| pos2(rect.left() + x * k, rect.top() + y * k);
    centre_block(p, images, at, k, span / 2.0, style);

    let mut real = false;
    for (i, ((name, hue), hero)) in ALLIES.iter().zip(&art::ALLIES).enumerate() {
        let dead = preview.dead_hero && i == DEAD_ALLY;
        let look = if dead { Look::dead(style) } else { Look::PLAIN };
        let hero = Stand {
            name,
            hue: *hue,
            art: hero.portrait,
            enemy: false,
            health: HEALTH[i],
            souls: SOULS[i],
            ready: i == ULTIMATE_READY,
        };
        let left = panel_left(style, i, false);
        real |= portrait(p, images, at, k, left, hero, style, look, dead, true);
    }
    for (i, ((name, hue), hero)) in ENEMIES.iter().zip(&art::ENEMIES).enumerate() {
        let missing = preview.missing_enemy && i == MISSING_ENEMY;
        let look = if missing {
            Look::missing(style)
        } else {
            Look::PLAIN
        };
        let hero = Stand {
            name,
            hue: *hue,
            art: hero.portrait,
            enemy: true,
            health: HEALTH[5 - i],
            souls: SOULS[5 - i],
            ready: i == 5 - ULTIMATE_READY,
        };
        let left = panel_left(style, i, true);
        real |= portrait(p, images, at, k, left, hero, style, look, false, !missing);
        if style.purchases && i == PURCHASE_ENEMY {
            purchase_popup(p, at, k, left, style);
        }
    }
    real
}

/// One example hero: initials and a colour for the painted coin, the portrait otherwise,
/// and the numbers its panel shows.
#[derive(Clone, Copy)]
struct Stand<'a> {
    name: &'a str,
    hue: Color32,
    art: Art,
    enemy: bool,
    health: f32,
    souls: &'a str,
    ready: bool,
}

/// `art` in `rect` if it is ready; false so the caller paints its shape instead.
fn icon(
    p: &Painter,
    images: &mut Images,
    art: Art,
    centre: Pos2,
    side: f32,
    tint: Color32,
) -> bool {
    let [w, h] = art.size.map(f32::from);
    let size = vec2(w, h) * (side / w.max(h));
    images.paint(p, art, Rect::from_center_size(centre, size), tint)
}

/// A souls number the game's way: the number, then a small faded "k".
fn souls_label(p: &Painter, anchor: Pos2, align: Align2, number: &str, size: f32, color: Color32) {
    let big = p.layout_no_wrap(
        number.into(),
        FontId::new(size.max(5.0), theme::semibold()),
        color,
    );
    let small = p.layout_no_wrap(
        "k".into(),
        FontId::new((size * 0.62).max(4.0), theme::semibold()),
        color.gamma_multiply(0.45),
    );
    let (bw, bh) = (big.size().x, big.size().y);
    let rect = align.anchor_size(anchor, vec2(bw + small.size().x, bh));
    let small_top = rect.bottom() - small.size().y - bh * 0.1;
    p.galley(rect.min, big, color);
    p.galley(pos2(rect.left() + bw, small_top), small, color);
}

/// Paints one player panel whose left edge is `left`; true when the hero's picture is
/// the game's.
#[allow(clippy::too_many_arguments)]
fn portrait(
    p: &Painter,
    images: &mut Images,
    at: impl Fn(f32, f32) -> Pos2,
    k: f32,
    left: f32,
    hero: Stand,
    style: &TopBarStyle,
    look: Look,
    dead: bool,
    health_visible: bool,
) -> bool {
    let Stand {
        name,
        hue,
        art: face,
        enemy,
        health,
        souls,
        ready,
    } = hero;
    let ps = f32::from(style.portrait_scale_pct) / 100.0;
    let q = |x: f32, y: f32| at(left + x * ps, y * ps);
    let s = ps * k;
    let mid = PORTRAIT_WIDTH / 2.0;
    let (team, team_text) = if enemy {
        (AMBER, AMBER_TEXT)
    } else {
        (SAPPHIRE, Color32::WHITE)
    };
    let side_color = if enemy {
        style.enemy_color
    } else {
        style.ally_color
    };
    let health_color =
        side_color.map_or(if enemy { ENEMY_HEALTH } else { ALLY_HEALTH }, to_color32);

    if !style.hide_player_souls {
        let tag = Rect::from_min_max(q(mid - 17.0, 76.0), q(mid + 17.0, 120.0));
        let tag_look = if dead {
            Look {
                alpha: look.alpha * 0.3,
                ..look
            }
        } else {
            look
        };
        let radius = (3.0 * s).round().clamp(0.0, 255.0) as u8;
        p.rect_filled(
            tag,
            CornerRadius {
                nw: 0,
                ne: 0,
                sw: radius,
                se: radius,
            },
            tag_look.apply(side_color.map_or(team, to_color32)),
        );
        souls_label(
            p,
            q(mid, 110.5),
            Align2::CENTER_CENTER,
            souls,
            15.0 * s,
            tag_look.apply(team_text),
        );
    }

    let centre = q(mid, DISC_BOTTOM - DISC / 2.0);
    let radius = DISC / 2.0 * s;
    p.circle_filled(centre, radius, look.apply(team));
    let card = Rect::from_min_max(q(mid - DISC / 2.0, 5.0), q(mid + DISC / 2.0, DISC_BOTTOM));
    let mask = crate::hud_art::badge_mask(card);
    let card_look = if dead {
        Look {
            gray: true,
            wash: look.wash.max(0.25),
            ..look
        }
    } else {
        look
    };
    let real = images.paint_shape(p, face, card, &mask, card_look.apply(Color32::WHITE));
    if real && card_look.gray {
        // A tint only multiplies, so grey is a wash over the disc; the card's top above it
        // is transparent around the bust and only darkens.
        p.circle_filled(
            centre,
            radius,
            Color32::from_rgba_unmultiplied(105, 105, 105, 170).gamma_multiply(look.alpha),
        );
    }
    if !real {
        let coin = if dead { Color32::from_gray(70) } else { hue };
        p.circle_filled(centre, radius * 0.86, card_look.apply(coin));
        if !dead {
            p.text(
                centre,
                Align2::CENTER_CENTER,
                name,
                FontId::new((20.0 * s).max(6.0), theme::semibold()),
                look.apply(OFF_WHITE),
            );
        }
    }

    if health_visible && !dead {
        let x = if enemy {
            PORTRAIT_WIDTH - 4.0 - 16.0
        } else {
            10.0
        };
        let (top, h) = (27.0, 50.0);
        let body: Vec<Pos2> = HEALTH_SHAPE
            .iter()
            .map(|&(fx, fy)| q(x + fx * 16.0, top + fy * h))
            .collect();
        p.add(Shape::convex_polygon(
            body.clone(),
            look.apply(Color32::from_rgba_unmultiplied(0, 0, 0, 0xDE)),
            Stroke::NONE,
        ));
        let line = q(0.0, top + h * 0.98 * (1.0 - health)).y;
        p.add(Shape::convex_polygon(
            crate::hud_art::cut_top(&body, line),
            look.apply(health_color),
            Stroke::NONE,
        ));
        let rect = Rect::from_min_max(q(x, top), q(x + 16.0, top + h));
        images.paint(p, art::PORTRAIT_HEALTH, rect, look.apply(OFF_BLACK));
    }

    if style.show_levels {
        let badge = q(mid, 82.0);
        p.circle_filled(
            badge,
            10.0 * s,
            look.apply(Color32::from_rgba_unmultiplied(0, 0, 0, 128)),
        );
        p.circle_stroke(
            badge,
            10.0 * s,
            Stroke::new(1.5 * s, look.apply(OFF_WHITE.gamma_multiply(0.5))),
        );
        p.text(
            badge,
            Align2::CENTER_CENTER,
            "14",
            FontId::new((12.0 * s).max(5.0), theme::semibold()),
            look.apply(OFF_WHITE),
        );
    }

    if dead {
        let font = FontId::new((32.0 * s).max(6.0), theme::semibold());
        p.text(
            centre + vec2(1.5, 1.5) * s,
            Align2::CENTER_CENTER,
            "23",
            font.clone(),
            Color32::from_rgba_unmultiplied(0, 0, 0, 170),
        );
        p.text(centre, Align2::CENTER_CENTER, "23", font, RESPAWN);
    }

    let status = q(mid, 136.0);
    if ready {
        if !icon(
            p,
            images,
            art::ULTIMATE,
            status,
            22.0 * s,
            look.apply(Color32::WHITE),
        ) {
            p.circle_filled(status, 9.0 * s, look.apply(OFF_WHITE));
        }
    } else {
        let dim = Look {
            alpha: look.alpha * 0.6,
            ..look
        };
        p.circle_filled(status, 11.0 * s, dim.apply(team));
        icon(
            p,
            images,
            art::ULTIMATE_OFF,
            status,
            22.0 * s,
            dim.apply(Color32::WHITE),
        );
    }
    real
}

fn purchase_popup(
    p: &Painter,
    at: impl Fn(f32, f32) -> Pos2,
    k: f32,
    left: f32,
    style: &TopBarStyle,
) {
    let ps = f32::from(style.portrait_scale_pct) / 100.0;
    let centre = at(left + PORTRAIT_WIDTH / 2.0 * ps, 162.0 * ps);
    let rect = Rect::from_center_size(centre, vec2(84.0 * k, 16.0 * k));
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

fn treatment_scale(t: Treatment, compact: f32) -> f32 {
    match t {
        Treatment::Vanilla => 1.0,
        Treatment::Compact => compact,
        Treatment::Hidden => 0.0,
    }
}

fn centre_block(
    p: &Painter,
    images: &mut Images,
    at: impl Fn(f32, f32) -> Pos2,
    k: f32,
    cx: f32,
    style: &TopBarStyle,
) {
    let chip = Color32::from_rgba_unmultiplied(0, 0, 0, 144);
    let clock_scale = treatment_scale(style.clock, 0.8);
    if clock_scale > 0.0 {
        let rect = Rect::from_center_size(
            at(cx, 8.0 + 15.0 * clock_scale),
            vec2(90.0 * clock_scale * k, 30.0 * clock_scale * k),
        );
        p.rect_filled(rect, CornerRadius::same((5.0 * k).round() as u8), chip);
        p.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "34:45",
            FontId::new((14.0 * clock_scale * k).max(5.0), theme::semibold()),
            OFF_WHITE.gamma_multiply(0.9),
        );
    }
    let lead_scale = treatment_scale(style.soul_lead, 0.75);
    if lead_scale > 0.0 {
        let (top, height) = (45.0, 33.0);
        let l = |dx: f32, dy: f32| at(cx + dx * lead_scale, top + dy * lead_scale);
        let half = CENTRE_WIDTH / 2.0;
        p.add(Shape::convex_polygon(
            vec![
                l(-half, 0.0),
                l(-3.0, 0.0),
                l(-3.0, height),
                l(-half, height),
            ],
            SAPPHIRE,
            Stroke::NONE,
        ));
        p.add(Shape::convex_polygon(
            vec![l(3.0, 0.0), l(half, 0.0), l(half, height), l(3.0, height)],
            AMBER,
            Stroke::NONE,
        ));
        let size = 22.0 * lead_scale * k;
        let mid = height / 2.0;
        souls_label(
            p,
            l(-38.0, mid),
            Align2::RIGHT_CENTER,
            "221",
            size,
            Color32::WHITE,
        );
        souls_label(
            p,
            l(45.0, mid),
            Align2::LEFT_CENTER,
            "248",
            size,
            AMBER_TEXT,
        );
        for (dx, team, color) in [(-22.0, 1, Color32::WHITE), (28.0, 0, AMBER_TEXT)] {
            let spot = l(dx, mid);
            if !icon(
                p,
                images,
                art::TEAM_ICONS[team],
                spot,
                30.0 * lead_scale * k,
                color,
            ) {
                p.circle_stroke(spot, 7.0 * lead_scale * k, Stroke::new(2.0 * k, color));
            }
        }
        if style.urn_lead {
            let rect = Rect::from_center_size(l(0.0, height + 11.0), vec2(48.0 * k, 18.0 * k));
            p.rect_filled(rect, CornerRadius::same(4), chip);
            p.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "+6%",
                FontId::new((13.0 * k).max(5.0), theme::semibold()),
                OFF_WHITE,
            );
        }
    }
    if style.spawn_timers {
        for (i, (text, soon)) in [("3:18", false), ("0:24", true)].into_iter().enumerate() {
            let x = cx + (i as f32 - 0.5) * 78.0;
            let rect = Rect::from_center_size(at(x, 112.0), vec2(70.0 * k, 18.0 * k));
            p.rect_filled(rect, CornerRadius::same(4), chip);
            let spot = pos2(rect.left() + 11.0 * k, rect.center().y);
            let tint = OFF_WHITE.gamma_multiply(0.8);
            if i == 0 {
                let s = 5.0 * k;
                p.add(Shape::convex_polygon(
                    vec![
                        spot + vec2(0.0, -s),
                        spot + vec2(s, 0.0),
                        spot + vec2(0.0, s),
                        spot + vec2(-s, 0.0),
                    ],
                    tint,
                    Stroke::NONE,
                ));
            } else if !icon(p, images, art::REJUV_ICON, spot, 13.0 * k, tint) {
                p.circle_stroke(spot, 4.5 * k, Stroke::new(1.5 * k, tint));
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
    let rejuv_scale = treatment_scale(style.rejuv_charges, 0.8);
    if rejuv_scale > 0.0 {
        let y = 135.0;
        for (dx, color) in [(-56.0, SAPPHIRE), (56.0, AMBER)] {
            let c = at(cx + dx * rejuv_scale, y);
            let s = 9.0 * rejuv_scale * k;
            if icon(p, images, art::TEAM_REJUV, c, 40.0 * rejuv_scale * k, color) {
                continue;
            }
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
            let (_dir, mut state) = crate::state::testutil::state();
            let mut output = ctx.run_ui(input, |ui| {
                let rect = Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 130.0));
                crate::hud_art::with(ui.ctx(), &mut state, |_, images| {
                    assert!(
                        !bar(ui.painter(), rect, style, preview, images),
                        "no game, no pictures"
                    );
                });
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
    fn panels_fill_the_bar_from_its_edges_and_leave_the_centre_free() {
        let game = TopBarStyle::default();
        assert_eq!(extent(&game), vec2(1260.0, 150.0));
        assert_eq!(panel_left(&game, 0, false), 0.0);
        assert_eq!(panel_left(&game, 5, false) + PORTRAIT_WIDTH, 528.0);
        assert_eq!(panel_left(&game, 0, true), 732.0);
        assert_eq!(panel_left(&game, 5, true) + PORTRAIT_WIDTH, 1260.0);

        let big = TopBarStyle {
            portrait_scale_pct: 130,
            portrait_gap_px: 24,
            purchases: true,
            ..game.clone()
        };
        let size = extent(&big);
        let panel = PORTRAIT_WIDTH * 1.3;
        let centre_free = panel_left(&big, 0, true) - (panel_left(&big, 5, false) + panel);
        assert!(centre_free >= 204.0 - 0.01, "centre keeps {centre_free}");
        assert!((panel_left(&big, 5, true) + panel - size.x).abs() < 0.01);
        assert!(size.y > 150.0 * 1.3, "room for the purchase popup");
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
