//! Player stats page (`hud::player_stats`): the lower-left cluster drawn from the game's
//! own pictures, every part clickable, draggable and resizable on the stage, with an
//! inspector for the selected part's placement and details. Edits go out with the HUD
//! addon on Apply.

use dt_core::hud::art::Art;
use dt_core::hud::elements::ElementId;
use dt_core::hud::minimap_colors::Color;
use dt_core::hud::player_stats::{
    NUMBER_PX_RANGE, OFFSET_RANGE, PartEdit, PlayerStatsStyle, SCALE_RANGE, SOULS_PX_RANGE, Sizing,
    StatFont, StatsPart, StatsPreset, TILE_GAP_RANGE, TILE_RADIUS_RANGE,
};
use eframe::egui::color_picker::{Alpha, color_edit_button_srgba};
use eframe::egui::epaint::TextShape;
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, CursorIcon, FontId, Id, Key, Layout, Painter, Pos2,
    Rect, RichText, Sense, Stroke, StrokeKind, Ui, Vec2, emath::Rot2, pos2, vec2,
};

use crate::hud_art::Images;
use crate::minimap_view::marked;
use crate::state::AppState;
use crate::theme::{self, ACCENT, BORDER, CARD, CARD_HOVER, RAIL, TEXT, WARN, WEAK};
use crate::widgets;

/// The stage: the screen's lower-left 500x480 game pixels at 1080p.
const STAGE: Vec2 = vec2(500.0, 480.0);
const INSPECTOR: f32 = 300.0;
const SIDE_BY_SIDE: f32 = 820.0;
const HANDLE: f32 = 8.0;

const OFF_WHITE: Color32 = Color32::from_rgb(0xFF, 0xEF, 0xD7);
const OFF_BLACK: Color32 = Color32::from_rgb(0x10, 0x13, 0x0D);
const SHARD: Color32 = Color32::from_rgb(0x99, 0xFF, 0xD6);
/// `courageBrightColor`, `spiritBrightColor`, `fortitudeBrightColor`.
const CATEGORY: [Color32; 3] = [
    Color32::from_rgb(0xEC, 0x97, 0x19),
    Color32::from_rgb(0xCE, 0x90, 0xFF),
    Color32::from_rgb(0x7B, 0xBA, 0x1D),
];
/// The empty part of each category's bar.
const BAR_EMPTY: [Color32; 3] = [
    Color32::from_rgb(0x22, 0x1C, 0x08),
    Color32::from_rgb(0x1A, 0x0A, 0x27),
    Color32::from_rgb(0x13, 0x2B, 0x09),
];
const POPUP: Color32 = Color32::from_rgba_unmultiplied_const(0x84, 0xE1, 0x84, 0xAA);
const DELTA: Color32 = Color32::from_rgb(0x8B, 0xF9, 0x8B);
const PIP_TEXT: Color32 = Color32::from_rgb(0xAD, 0xFF, 0x2F);

const fn art(path: &'static str, w: u16, h: u16) -> Art {
    Art { path, size: [w, h] }
}

const CORE_ICONS: [Art; 3] = [
    art(
        "panorama/images/hud/core/core_weapon_icon_psd.vtex_c",
        71,
        72,
    ),
    art(
        "panorama/images/hud/core/core_spirit_icon_psd.vtex_c",
        71,
        72,
    ),
    art(
        "panorama/images/hud/core/core_armor_icon_psd.vtex_c",
        71,
        72,
    ),
];
const JAR_FRAME: Art = art(
    "panorama/images/hud/core/spirit_jar_frame_png.vtex_c",
    240,
    244,
);
const JAR_FILL: Art = art(
    "panorama/images/hud/core/spirit_jar_fill_png.vtex_c",
    208,
    212,
);
const SOUL: Art = art("panorama/images/hud/icons/icon_soul.vsvg_c", 134, 256);
const TIER: Art = art("panorama/images/shop/tier_corner_cap.vsvg_c", 256, 256);
const PIP: Art = art("panorama/images/hud/hero_ability_pip_png.vtex_c", 23, 23);
/// Ten bought items, column by column as the game fills the grid, with their category.
const ITEMS: [(Art, usize); 10] = [
    (
        art(
            "panorama/images/items/weapon/headshot_booster_psd.vtex_c",
            200,
            200,
        ),
        0,
    ),
    (
        art(
            "panorama/images/items/vitality/extra_health_psd.vtex_c",
            200,
            200,
        ),
        2,
    ),
    (
        art(
            "panorama/images/items/spirit/mystic_reach_psd.vtex_c",
            200,
            200,
        ),
        1,
    ),
    (
        art(
            "panorama/images/items/weapon/active_reload_psd.vtex_c",
            200,
            200,
        ),
        0,
    ),
    (
        art(
            "panorama/images/items/vitality/battle_vest_psd.vtex_c",
            200,
            200,
        ),
        2,
    ),
    (
        art(
            "panorama/images/items/spirit/extra_charge_psd.vtex_c",
            200,
            200,
        ),
        1,
    ),
    (
        art(
            "panorama/images/items/weapon/fleetfoot_psd.vtex_c",
            200,
            200,
        ),
        0,
    ),
    (
        art(
            "panorama/images/items/vitality/healing_booster_psd.vtex_c",
            200,
            200,
        ),
        2,
    ),
    (
        art(
            "panorama/images/items/spirit/cold_front_psd.vtex_c",
            200,
            200,
        ),
        1,
    ),
    (
        art(
            "panorama/images/items/weapon/burst_fire_psd.vtex_c",
            200,
            200,
        ),
        0,
    ),
];
const NEXT_ITEM: Art = art(
    "panorama/images/items/vitality/divine_barrier_psd.vtex_c",
    200,
    200,
);
const STATUS: [(Art, Color32); 3] = [
    (
        art(
            "panorama/images/items/spirit/slowing_hex_psd.vtex_c",
            200,
            200,
        ),
        Color32::from_rgb(0xFC, 0x82, 0x82),
    ),
    (
        art(
            "panorama/images/items/vitality/fury_trance_psd.vtex_c",
            200,
            200,
        ),
        Color32::from_rgb(0x8B, 0xF9, 0x8B),
    ),
    (
        art(
            "panorama/images/items/weapon/lucky_shot_psd.vtex_c",
            200,
            200,
        ),
        Color32::from_rgb(0x8B, 0xF9, 0x8B),
    ),
];

/// Draw order on the stage, big and rarely edited first, so smaller parts win the pointer.
const STACK: [StatsPart; 7] = [
    StatsPart::Popups,
    StatsPart::Numbers,
    StatsPart::StatusEffects,
    StatsPart::Items,
    StatsPart::Level,
    StatsPart::Souls,
    StatsPart::Quickbuy,
];

#[derive(Clone, Copy, Default, PartialEq)]
enum View {
    #[default]
    Yours,
    Game,
}

#[derive(Clone, Copy)]
struct DragStart {
    edit: PartEdit,
    anchor: Pos2,
    /// 0 for a move, the corner's distance from `anchor` for a resize.
    reach: f32,
}

pub fn page(ui: &mut Ui, state: &mut AppState) {
    crate::hud_view::hud_error(ui, state);
    crate::hud_view::show_layout_note(ui, state, ElementId::PlayerStats);
    let mut style = state.profile.hud.player_stats.clone();
    let mut selected = state.ui.stats_selected;
    let ctx = ui.ctx().clone();
    crate::hud_art::with(&ctx, state, |_, images| {
        presets(ui, &mut style, &mut selected, images);
        let width = ui.available_width();
        if width >= SIDE_BY_SIDE {
            ui.horizontal_top(|ui| {
                let gap = 12.0;
                let left = width - INSPECTOR - gap;
                ui.vertical(|ui| {
                    ui.set_width(left);
                    stage_card(ui, &mut style, &mut selected, images, left);
                });
                ui.add_space(gap - ui.spacing().item_spacing.x);
                ui.vertical(|ui| {
                    ui.set_width(INSPECTOR);
                    inspector(ui, &mut style, &mut selected);
                });
            });
        } else {
            stage_card(ui, &mut style, &mut selected, images, width);
            inspector(ui, &mut style, &mut selected);
        }
    });
    credits(ui);
    state.ui.stats_selected = selected;
    if style != state.profile.hud.player_stats {
        state.set_player_stats_style(style);
    }
}

fn presets(
    ui: &mut Ui,
    style: &mut PlayerStatsStyle,
    selected: &mut Option<StatsPart>,
    images: &mut Images,
) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Player stats style")
                .size(15.0)
                .family(theme::semibold())
                .color(TEXT),
        );
        widgets::badge(ui, "Experimental, untested in game", WARN);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let n = style.changed_count();
            if n > 0
                && widgets::reset_pill(ui)
                    .on_hover_text("Back to the game's own player stats")
                    .clicked()
            {
                *style = PlayerStatsStyle::default();
                *selected = None;
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
    widgets::hint(
        ui,
        "Goes into the HUD addon on Apply. Moving the whole cluster is on the HUD page.",
    );
    ui.add_space(8.0);
    let current = style.preset();
    let gap = 10.0;
    let count = ((ui.available_width() + gap) / (170.0 + gap))
        .floor()
        .clamp(3.0, StatsPreset::ALL.len() as f32);
    let width = ((ui.available_width() - gap * (count - 1.0)) / count).min(220.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(gap, gap);
        for preset in StatsPreset::ALL {
            let on = current == Some(preset);
            let stage_w = width - 12.0;
            let stage_h = (stage_w * 0.62).round();
            let (rect, response) =
                ui.allocate_exact_size(vec2(width, stage_h + 34.0), Sense::click());
            ui.painter().rect(
                rect,
                CornerRadius::same(10),
                if response.hovered() { CARD_HOVER } else { CARD },
                Stroke::new(if on { 2.0 } else { 1.0 }, if on { ACCENT } else { BORDER }),
                StrokeKind::Inside,
            );
            let window = Rect::from_min_size(rect.min + vec2(6.0, 6.0), vec2(stage_w, stage_h));
            thumbnail(ui, window, &preset.style(), images);
            let painter = ui.painter();
            painter.text(
                pos2(rect.left() + 11.0, window.bottom() + 14.0),
                Align2::LEFT_CENTER,
                preset.label(),
                FontId::new(12.5, theme::semibold()),
                if on { ACCENT } else { TEXT },
            );
            if on {
                let c = pos2(rect.right() - 17.0, rect.top() + 17.0);
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
                .on_hover_cursor(CursorIcon::PointingHand)
                .clicked()
            {
                *style = preset.style();
                *selected = None;
            }
        }
    });
    ui.add_space(6.0);
}

/// The lower part of the stage (numbers, level, souls, items) for `style`, filling
/// `window`, over a patch of the game.
fn thumbnail(ui: &Ui, window: Rect, style: &PlayerStatsStyle, images: &mut Images) {
    crate::game_shot::backdrop(ui, window);
    let shown = Rect::from_min_max(pos2(10.0, 165.0), pos2(480.0, 470.0));
    let k = window.width() / shown.width();
    let k = k.max(window.height() / shown.height());
    let stage = Rect::from_min_size(window.center() - shown.center().to_vec2() * k, STAGE * k);
    let painter = ui.painter().with_clip_rect(window);
    for part in STACK {
        let spec = part.spec();
        let edit = style.part(part);
        if edit.hidden {
            continue;
        }
        let (_, pivot) = natural(&painter, style, part);
        let pen = Pen {
            p: &painter,
            stage,
            k,
            xf: Xf::new(edit, pivot),
            alpha: f32::from(edit.opacity_pct.unwrap_or(spec.vanilla_opacity)) / 100.0,
        };
        paint_part(&pen, images, style, part);
    }
}

fn stage_card(
    ui: &mut Ui,
    style: &mut PlayerStatsStyle,
    selected: &mut Option<StatsPart>,
    images: &mut Images,
    width: f32,
) {
    let view_id = Id::new("stats-view");
    let ghost_id = Id::new("stats-ghost");
    let mut view: View = ui.data(|d| d.get_temp(view_id)).unwrap_or_default();
    let mut ghost: bool = ui.data(|d| d.get_temp(ghost_id)).unwrap_or(true);
    ui.allocate_ui(vec2(width, 0.0), |ui| {
        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                widgets::caption(ui, "Preview");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let labels = ["Yours", "Game's"];
                    if let Some(i) = widgets::segmented(ui, &labels, view as usize) {
                        view = [View::Yours, View::Game][i];
                    }
                    if view == View::Yours {
                        ui.add_space(8.0);
                        if widgets::chip(ui, "Game's HUD underneath", ACCENT, Some(ghost))
                            .on_hover_text(
                                "The game's own lower left, faint, to line parts up with.",
                            )
                            .clicked()
                        {
                            ghost = !ghost;
                        }
                    }
                });
            });
            ui.add_space(4.0);
            let w = ui.available_width();
            let h = (w * STAGE.y / STAGE.x).min(620.0);
            let w = h * STAGE.x / STAGE.y;
            let (outer, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
            let stage = Rect::from_center_size(outer.center(), vec2(w, h));
            match view {
                View::Yours => {
                    stage_view(ui, stage, style, selected, images, ghost);
                    widgets::hint(
                        ui,
                        "Click a part to edit it. Drag to move; scroll or drag a corner to \
                         resize. Arrow keys nudge the selected part, Shift for 10 px.",
                    );
                }
                View::Game => {
                    backdrop(ui.painter(), stage);
                    if let Some(texture) = crate::game_shot::texture(ui.ctx()) {
                        crate::game_shot::paint(
                            ui.painter(),
                            &texture,
                            shot_crop(),
                            stage,
                            Color32::WHITE,
                        );
                    }
                    widgets::hint(ui, "The game's own lower left at default settings.");
                }
            }
            parts_bar(ui, style, selected);
        });
    });
    ui.data_mut(|d| {
        d.insert_temp(view_id, view);
        d.insert_temp(ghost_id, ghost);
    });
}

/// The stage's area of the 1920x1080 reference screenshot.
fn shot_crop() -> [f32; 4] {
    [
        0.0,
        1.0 - STAGE.y / 1080.0,
        STAGE.x / 1920.0,
        STAGE.y / 1080.0,
    ]
}

fn backdrop(p: &Painter, r: Rect) {
    p.rect(
        r,
        CornerRadius::same(6),
        Color32::from_rgb(24, 22, 24),
        Stroke::new(1.0, BORDER),
        StrokeKind::Outside,
    );
}

/// Where a part sits before its edit, in stage pixels (y down), and the point its size
/// grows from.
fn natural(p: &Painter, style: &PlayerStatsStyle, part: StatsPart) -> (Rect, Pos2) {
    let bounds = match part {
        StatsPart::Numbers => Rect::from_min_max(pos2(20.0, 170.0), pos2(290.0, 260.0)),
        StatsPart::Popups => Rect::from_min_max(pos2(30.0, 25.0), pos2(290.0, 170.0)),
        StatsPart::Level => Rect::from_min_max(pos2(20.0, 282.0), pos2(80.0, 342.0)),
        StatsPart::Souls => souls_layout(p, style).bounds,
        StatsPart::Items => items_layout(style).bounds,
        StatsPart::Quickbuy => Rect::from_min_max(pos2(394.0, 390.0), pos2(464.0, 455.0)),
        StatsPart::StatusEffects => Rect::from_min_max(pos2(20.0, 248.0), pos2(116.0, 280.0)),
    };
    let pivot = match part.spec().sizing {
        Sizing::UiScale { .. } => pos2(22.0, STAGE.y - 16.0),
        Sizing::Middle => bounds.center(),
        Sizing::BottomLeft => bounds.left_bottom(),
    };
    (bounds, pivot)
}

/// A part's edit applied to stage points: size about `pivot`, then the move.
#[derive(Clone, Copy)]
struct Xf {
    pivot: Pos2,
    s: f32,
    off: Vec2,
}

impl Xf {
    fn new(edit: PartEdit, pivot: Pos2) -> Xf {
        let s = f32::from(edit.scale()) / 100.0;
        Xf {
            pivot,
            s,
            off: vec2(f32::from(edit.offset_x), f32::from(edit.offset_y)),
        }
    }

    fn at(self, p: Pos2) -> Pos2 {
        self.pivot + (p - self.pivot) * self.s + self.off
    }
}

/// Paints one part in stage points through its `Xf` onto the stage on screen.
struct Pen<'a> {
    p: &'a Painter,
    stage: Rect,
    k: f32,
    xf: Xf,
    alpha: f32,
}

impl Pen<'_> {
    fn pt(&self, g: Pos2) -> Pos2 {
        self.stage.min + self.xf.at(g).to_vec2() * self.k
    }

    fn rect(&self, r: Rect) -> Rect {
        Rect::from_two_pos(self.pt(r.min), self.pt(r.max))
    }

    fn len(&self, l: f32) -> f32 {
        l * self.k * self.xf.s
    }

    fn c(&self, c: Color32) -> Color32 {
        c.gamma_multiply(self.alpha)
    }

    fn fill(&self, r: Rect, radius: f32, c: Color32) {
        self.p.rect_filled(
            self.rect(r),
            CornerRadius::same(self.len(radius).round().clamp(0.0, 255.0) as u8),
            self.c(c),
        );
    }

    fn image(&self, images: &mut Images, art: Art, r: Rect, tint: Color32) -> bool {
        images.paint(self.p, art, self.rect(r), self.c(tint))
    }

    /// Text in game px with the game's offBlack outline, `align` at `at`, tilted `degrees`.
    fn text(&self, at: Pos2, align: Align2, text: &str, px: f32, color: Color32, degrees: f32) {
        self.text_in(StatFont::Game, at, align, text, px, color, degrees);
    }

    #[allow(clippy::too_many_arguments)]
    fn text_in(
        &self,
        family: StatFont,
        at: Pos2,
        align: Align2,
        text: &str,
        px: f32,
        color: Color32,
        degrees: f32,
    ) {
        let size = self.len(px).max(1.0);
        let font = match family {
            StatFont::Game | StatFont::Block => FontId::new(size, theme::semibold()),
            StatFont::Sans => FontId::proportional(size),
            StatFont::Mono => FontId::monospace(size * 0.9),
        };
        let galley = self.p.layout_no_wrap(text.to_owned(), font, color);
        let corner = align.anchor_size(self.pt(at), galley.size()).min;
        let angle = degrees.to_radians();
        let outline = self.len(1.6).max(0.8);
        let shade = self.c(OFF_BLACK);
        for step in 0..8 {
            let offset =
                Rot2::from_angle(step as f32 * std::f32::consts::FRAC_PI_4) * vec2(outline, 0.0);
            self.p.add(
                TextShape::new(corner + offset, galley.clone(), shade)
                    .with_override_text_color(shade)
                    .with_angle(angle),
            );
        }
        self.p.add(
            TextShape::new(corner, galley, self.c(color))
                .with_override_text_color(self.c(color))
                .with_angle(angle),
        );
    }
}

fn to32(c: Color) -> Color32 {
    let [r, g, b, a] = c.0;
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

fn category(style: &PlayerStatsStyle, i: usize) -> Color32 {
    [style.weapon_color, style.spirit_color, style.vitality_color][i].map_or(CATEGORY[i], to32)
}

fn stage_view(
    ui: &mut Ui,
    stage: Rect,
    style: &mut PlayerStatsStyle,
    selected: &mut Option<StatsPart>,
    images: &mut Images,
    ghost: bool,
) {
    let painter = ui.painter().with_clip_rect(stage);
    backdrop(ui.painter(), stage);
    if ghost && let Some(texture) = crate::game_shot::texture(ui.ctx()) {
        let tint = Color32::WHITE.gamma_multiply(0.22);
        crate::game_shot::paint(&painter, &texture, shot_crop(), stage, tint);
    }
    if ui
        .interact(stage, ui.id().with("stats-bg"), Sense::click())
        .clicked()
    {
        *selected = None;
    }
    let k = stage.width() / STAGE.x;
    let drag_key = ui.id().with("stats-drag");
    let on_top = *selected;
    let order: Vec<StatsPart> = STACK
        .iter()
        .copied()
        .filter(|&p| Some(p) != on_top)
        .chain(on_top)
        .collect();
    let mut edits = Vec::new();
    for part in order {
        let spec = part.spec();
        let edit = style.part(part);
        let (bounds, pivot) = natural(&painter, style, part);
        let pen = Pen {
            p: &painter,
            stage,
            k,
            xf: Xf::new(edit, pivot),
            alpha: f32::from(edit.opacity_pct.unwrap_or(spec.vanilla_opacity)) / 100.0,
        };
        if !edit.hidden {
            paint_part(&pen, images, style, part);
        }
        let rect = pen.rect(bounds);
        let is_selected = *selected == Some(part);
        if edit.hidden && !is_selected {
            continue;
        }
        let response = ui
            .interact(
                rect,
                ui.id().with(("stats-part", part)),
                Sense::click_and_drag(),
            )
            .on_hover_cursor(CursorIcon::Grab);
        let outline = if is_selected {
            Some(Stroke::new(1.5, ACCENT))
        } else if response.hovered() {
            Some(Stroke::new(1.0, TEXT.gamma_multiply(0.6)))
        } else {
            None
        };
        if let Some(stroke) = outline {
            painter.rect_stroke(rect, CornerRadius::same(3), stroke, StrokeKind::Outside);
            let label = if edit.hidden {
                format!("{} (hidden)", spec.label)
            } else {
                spec.label.to_string()
            };
            tag(&painter, rect, &label, is_selected);
        }
        if response.clicked() || response.drag_started() {
            *selected = Some(part);
        }
        if response.drag_started() {
            let start = DragStart {
                edit,
                anchor: Pos2::ZERO,
                reach: 0.0,
            };
            ui.ctx().data_mut(|d| d.insert_temp(drag_key, start));
        }
        if response.dragged()
            && let Some(start) = ui.ctx().data(|d| d.get_temp::<DragStart>(drag_key))
            && start.reach == 0.0
            && let (Some(origin), Some(pos)) = (
                ui.input(|i| i.pointer.press_origin()),
                response.interact_pointer_pos(),
            )
        {
            let delta = (pos - origin) / k;
            let mut next = start.edit;
            next.offset_x = clamp_offset(i32::from(start.edit.offset_x) + delta.x.round() as i32);
            next.offset_y = clamp_offset(i32::from(start.edit.offset_y) + delta.y.round() as i32);
            edits.push((part, next));
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                ui.input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
                let step = (scroll / 8.0).abs().ceil().min(10.0) * scroll.signum();
                let mut next = edit;
                next.scale_pct = clamp_scale(i32::from(edit.scale()) + step as i32);
                edits.push((part, next));
            }
        }
        if is_selected && !edit.hidden {
            handles(ui, &painter, rect, part, edit, drag_key, &mut edits);
        }
    }
    if let Some(part) = *selected
        && !ui.memory(|m| m.focused().is_some())
    {
        let (step, mut dx, mut dy) = ui.input(|i| {
            let step = if i.modifiers.shift { 10 } else { 1 };
            let x = i32::from(i.key_pressed(Key::ArrowRight))
                - i32::from(i.key_pressed(Key::ArrowLeft));
            let y =
                i32::from(i.key_pressed(Key::ArrowDown)) - i32::from(i.key_pressed(Key::ArrowUp));
            (step, x, y)
        });
        dx *= step;
        dy *= step;
        if dx != 0 || dy != 0 {
            let mut next = style.part(part);
            next.offset_x = clamp_offset(i32::from(next.offset_x) + dx);
            next.offset_y = clamp_offset(i32::from(next.offset_y) + dy);
            edits.push((part, next));
        }
    }
    for (part, edit) in edits {
        style.set_part(part, edit);
    }
}

fn clamp_offset(v: i32) -> i16 {
    v.clamp(
        i32::from(*OFFSET_RANGE.start()),
        i32::from(*OFFSET_RANGE.end()),
    ) as i16
}

fn clamp_scale(v: i32) -> u16 {
    v.clamp(
        i32::from(*SCALE_RANGE.start()),
        i32::from(*SCALE_RANGE.end()),
    ) as u16
}

/// The part's name on a small plate above its outline.
fn tag(p: &Painter, rect: Rect, text: &str, selected: bool) {
    let galley = p.layout_no_wrap(text.to_owned(), FontId::proportional(11.0), Color32::WHITE);
    let size = galley.size() + vec2(10.0, 4.0);
    let mut at = rect.left_top() - vec2(0.0, size.y + 2.0);
    if at.y < p.clip_rect().top() {
        at.y = rect.top() + 2.0;
    }
    let plate = Rect::from_min_size(at, size);
    let (fill, ink) = if selected {
        (ACCENT, theme::ON_ACCENT)
    } else {
        (Color32::from_black_alpha(200), TEXT)
    };
    p.rect_filled(plate, CornerRadius::same(3), fill);
    p.galley(plate.min + vec2(5.0, 2.0), galley, ink);
}

fn handles(
    ui: &mut Ui,
    painter: &Painter,
    rect: Rect,
    part: StatsPart,
    edit: PartEdit,
    drag_key: egui::Id,
    edits: &mut Vec<(StatsPart, PartEdit)>,
) {
    let corners = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ];
    for (i, corner) in corners.into_iter().enumerate() {
        let handle = Rect::from_center_size(corner, Vec2::splat(HANDLE));
        let cursor = if i % 2 == 0 {
            CursorIcon::ResizeNwSe
        } else {
            CursorIcon::ResizeNeSw
        };
        let response = ui
            .interact(
                handle,
                ui.id().with(("stats-handle", part, i)),
                Sense::drag(),
            )
            .on_hover_cursor(cursor);
        painter.rect(
            handle,
            CornerRadius::same(1),
            ACCENT,
            Stroke::new(1.0, RAIL),
            StrokeKind::Inside,
        );
        if response.drag_started() {
            let anchor = corners[(i + 2) % 4];
            let start = DragStart {
                edit,
                anchor,
                reach: (corner - anchor).length().max(1.0),
            };
            ui.ctx().data_mut(|d| d.insert_temp(drag_key, start));
        }
        if response.dragged()
            && let Some(start) = ui.ctx().data(|d| d.get_temp::<DragStart>(drag_key))
            && start.reach > 0.0
            && let Some(pos) = response.interact_pointer_pos()
        {
            let factor = (pos - start.anchor).length() / start.reach;
            let mut next = start.edit;
            next.scale_pct = clamp_scale((f32::from(start.edit.scale()) * factor).round() as i32);
            edits.push((part, next));
        }
    }
}

fn paint_part(pen: &Pen, images: &mut Images, style: &PlayerStatsStyle, part: StatsPart) {
    match part {
        StatsPart::Numbers => numbers(pen, images, style),
        StatsPart::Popups => popups(pen),
        StatsPart::Level => level(pen, images, style),
        StatsPart::Souls => souls(pen, images, style),
        StatsPart::Items => items(pen, images, style),
        StatsPart::Quickbuy => quickbuy(pen, images),
        StatsPart::StatusEffects => status_effects(pen, images),
    }
}

/// `.core_stat` cells: 80 px, a 90 px pitch, 5 px in from the block.
fn numbers(pen: &Pen, images: &mut Images, style: &PlayerStatsStyle) {
    let icon_alpha = f32::from(style.icon_opacity_pct) / 100.0;
    let tilt = if style.straight_numbers { 0.0 } else { -3.0 };
    let color = style.number_color.map_or(OFF_WHITE, to32);
    for (i, value) in ["79", "161", "4557"].into_iter().enumerate() {
        let cell = Rect::from_min_size(pos2(25.0 + 90.0 * i as f32, 175.0), vec2(80.0, 80.0));
        let tint = category(style, i).gamma_multiply(icon_alpha);
        if !pen.image(images, CORE_ICONS[i], cell, tint) {
            pen.p
                .circle_filled(pen.pt(cell.center()), pen.len(30.0), pen.c(tint));
        }
        if !style.hide_bars {
            let bar = Rect::from_min_max(
                pos2(cell.left() + 57.0, 185.0),
                pos2(cell.left() + 66.0, 225.0),
            );
            pen.fill(bar, 1.0, BAR_EMPTY[i]);
            let level = Rect::from_min_max(pos2(bar.left(), 201.0), bar.max);
            pen.fill(level, 1.0, category(style, i));
        }
        pen.text_in(
            style.number_font,
            cell.center() + vec2(-4.0, 6.0),
            Align2::CENTER_CENTER,
            value,
            f32::from(style.number_px),
            color,
            tilt,
        );
        if i == 0 && !style.hide_deltas {
            pen.text(
                pos2(cell.right() - 6.0, 230.0),
                Align2::RIGHT_CENTER,
                "18",
                14.0,
                DELTA,
                0.0,
            );
        }
    }
}

/// `#StatList` gain popups as the screenshot shows them after buying.
fn popups(pen: &Pen) {
    for (min, max, text) in [
        (pos2(30.0, 62.0), pos2(107.0, 95.0), "+18"),
        (pos2(30.0, 133.0), pos2(107.0, 168.0), "+7"),
        (pos2(202.0, 30.0), pos2(283.0, 60.0), "+1.5"),
    ] {
        let r = Rect::from_min_max(min, max);
        pen.fill(r, 2.0, POPUP);
        let galley = pen.p.layout_no_wrap(
            text.to_owned(),
            FontId::new(pen.len(16.0), theme::semibold()),
            pen.c(OFF_BLACK),
        );
        let at = Align2::LEFT_CENTER
            .anchor_size(pen.pt(pos2(r.left() + 8.0, r.center().y)), galley.size())
            .min;
        pen.p.galley(at, galley, pen.c(OFF_BLACK));
    }
}

/// The 60 px soul jar and the level in it.
fn level(pen: &Pen, images: &mut Images, style: &PlayerStatsStyle) {
    let jar = Rect::from_min_size(pos2(20.0, 282.0), vec2(60.0, 60.0));
    let tint = style.jar_color.map_or(Color32::WHITE, to32);
    let fill = Rect::from_min_max(pos2(25.0, 289.0), pos2(77.0, 340.0));
    let framed = style.hide_jar
        || pen.image(images, JAR_FILL, fill, tint.gamma_multiply(0.4))
            & pen.image(images, JAR_FRAME, jar, tint);
    if !framed {
        let ring = style.jar_color.map_or(SHARD, to32);
        pen.p.circle_stroke(
            pen.pt(jar.center()),
            pen.len(26.0),
            Stroke::new(pen.len(4.0), pen.c(ring)),
        );
    }
    pen.text_in(
        style.level_font,
        jar.center() + vec2(2.5, 2.5),
        Align2::CENTER_CENTER,
        "30",
        f32::from(style.level_px),
        style.level_color.map_or(OFF_WHITE, to32),
        0.0,
    );
}

struct SoulsLayout {
    icon: Option<Rect>,
    number: Pos2,
    label: Option<Pos2>,
    bounds: Rect,
}

/// `#hudGoldContainer`: icon, number, then the SOULS label, left to right from x 91.
fn souls_layout(p: &Painter, style: &PlayerStatsStyle) -> SoulsLayout {
    let px = f32::from(style.souls_px);
    let middle = 311.0;
    let mut x = 91.0;
    let icon = (!style.hide_souls_icon).then(|| {
        let r = Rect::from_center_size(pos2(x + 10.0, middle), vec2(20.0, 36.0) * px / 32.0);
        x = r.right() + 3.0;
        r
    });
    let width = p
        .layout_no_wrap("4,748".into(), FontId::new(px, theme::semibold()), TEXT)
        .size()
        .x;
    let number = pos2(x, middle);
    x += width + 5.0;
    let label = (!style.hide_souls_label).then(|| {
        let at = pos2(x, middle + px * 0.35);
        x += 44.0;
        at
    });
    let top = middle - (px * 0.6).max(18.0);
    let bottom = middle + (px * 0.6).max(18.0);
    SoulsLayout {
        icon,
        number,
        label,
        bounds: Rect::from_min_max(pos2(88.0, top), pos2(x + 2.0, bottom)),
    }
}

fn souls(pen: &Pen, images: &mut Images, style: &PlayerStatsStyle) {
    let layout = souls_layout(pen.p, style);
    let color = style.souls_color.map_or(SHARD, to32);
    if let Some(icon) = layout.icon
        && !pen.image(images, SOUL, icon, color)
    {
        pen.fill(icon.shrink2(vec2(4.0, 8.0)), 6.0, color);
    }
    pen.text_in(
        style.souls_font,
        layout.number,
        Align2::LEFT_CENTER,
        "4,748",
        f32::from(style.souls_px),
        color,
        0.0,
    );
    if let Some(at) = layout.label {
        pen.text(
            at,
            Align2::LEFT_BOTTOM,
            "SOULS",
            14.0,
            OFF_WHITE.gamma_multiply(0.4),
            0.0,
        );
    }
}

struct ItemsLayout {
    tile: f32,
    pitch: f32,
    origin: Pos2,
    bounds: Rect,
}

/// `.ModsContainer` at the game's 120 %: 45 px tiles, `tile_gap_px` padding round each,
/// two rows filled column by column, 22 px in and 16 px up.
fn items_layout(style: &PlayerStatsStyle) -> ItemsLayout {
    let u = 1.2;
    let gap = f32::from(style.tile_gap_px) * u;
    let tile = 45.0 * u;
    let pitch = tile + 2.0 * gap;
    let origin = pos2(22.0 + gap, STAGE.y - 16.0 - gap - tile - pitch);
    let bounds = Rect::from_min_max(
        pos2(22.0, origin.y - gap),
        pos2(22.0 + 6.0 * pitch, STAGE.y - 16.0),
    );
    ItemsLayout {
        tile,
        pitch,
        origin,
        bounds,
    }
}

fn items(pen: &Pen, images: &mut Images, style: &PlayerStatsStyle) {
    let l = items_layout(style);
    let u = l.tile / 45.0;
    let at = |column: usize, row: usize| {
        Rect::from_min_size(
            l.origin + vec2(column as f32 * l.pitch, row as f32 * l.pitch),
            Vec2::splat(l.tile),
        )
    };
    for slot in 0..12 {
        let r = at(slot / 2, slot % 2);
        let Some(&(item, cat)) = ITEMS.get(slot) else {
            let empty = f32::from(style.empty_opacity_pct) / 100.0;
            pen.fill(
                r,
                3.0 * u,
                Color32::from_white_alpha(10).gamma_multiply(empty),
            );
            pen.p.rect_stroke(
                pen.rect(r),
                CornerRadius::same(2),
                Stroke::new(
                    1.0,
                    pen.c(Color32::from_white_alpha(28).gamma_multiply(empty)),
                ),
                StrokeKind::Inside,
            );
            continue;
        };
        let round = f32::from(style.tile_radius_px).max(3.0) * u;
        pen.fill(r, round, Color32::from_black_alpha(48));
        let picture = r.shrink(f32::from(style.tile_radius_px) * 0.3 * u);
        if !pen.image(images, item, picture, Color32::WHITE) {
            pen.fill(picture.shrink(4.0), 2.0, CATEGORY[cat].gamma_multiply(0.5));
        }
        if style.mono_items {
            pen.fill(
                picture,
                round,
                Color32::from_rgba_unmultiplied(120, 120, 120, 165),
            );
        }
        if slot == 6 {
            let cover = f32::from(style.cooldown_pct) / 100.0;
            let mask = Rect::from_min_max(pos2(r.left(), r.top() + r.height() * 0.35), r.max);
            pen.fill(mask, 0.0, OFF_BLACK.gamma_multiply(cover));
        }
        if !style.hide_tiers {
            let corner =
                Rect::from_min_size(pos2(r.right() - 22.0 * u, r.top()), Vec2::splat(22.0 * u));
            if !pen.image(images, TIER, corner, category(style, cat)) {
                let tip = [
                    pen.pt(corner.left_top()),
                    pen.pt(corner.right_top()),
                    pen.pt(corner.right_bottom()),
                ];
                pen.p.add(egui::Shape::convex_polygon(
                    tip.to_vec(),
                    pen.c(category(style, cat)),
                    Stroke::NONE,
                ));
            }
        }
        if slot == 3 && !style.hide_upgrades {
            let pip = Rect::from_min_size(
                pos2(r.left() - 5.0 * u, r.bottom() - 25.0 * u),
                Vec2::splat(28.0 * u),
            );
            if !pen.image(images, PIP, pip, Color32::WHITE) {
                pen.fill(pip.shrink(4.0 * u), 14.0 * u, OFF_BLACK);
            }
            pen.text(
                pip.center(),
                Align2::CENTER_CENTER,
                "+2",
                14.0 * u,
                PIP_TEXT,
                0.0,
            );
        }
    }
}

/// `#HudMini`: the next item in a 70 px box with an offWhite 30 % border.
fn quickbuy(pen: &Pen, images: &mut Images) {
    let r = Rect::from_min_max(pos2(394.0, 390.0), pos2(464.0, 455.0));
    pen.p.rect_stroke(
        pen.rect(r),
        CornerRadius::same(pen.len(5.0) as u8),
        Stroke::new(pen.len(3.0), pen.c(OFF_WHITE.gamma_multiply(0.3))),
        StrokeKind::Inside,
    );
    if !pen.image(images, NEXT_ITEM, r.shrink(8.0), Color32::WHITE) {
        pen.fill(r.shrink(10.0), 3.0, CATEGORY[2].gamma_multiply(0.5));
    }
}

/// Three sample effects, 28 px each, the way `CitadelStatusEffect` lines them up.
fn status_effects(pen: &Pen, images: &mut Images) {
    for (i, (icon, ring)) in STATUS.into_iter().enumerate() {
        let r = Rect::from_min_size(pos2(22.0 + 32.0 * i as f32, 250.0), vec2(28.0, 28.0));
        pen.fill(r, 4.0, Color32::from_black_alpha(150));
        pen.image(images, icon, r.shrink(3.0), Color32::WHITE);
        pen.p.rect_stroke(
            pen.rect(r),
            CornerRadius::same(pen.len(4.0) as u8),
            Stroke::new(pen.len(1.5), pen.c(ring)),
            StrokeKind::Inside,
        );
    }
}

fn parts_bar(ui: &mut Ui, style: &mut PlayerStatsStyle, selected: &mut Option<StatsPart>) {
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for part in StatsPart::ALL {
            chip(ui, style, selected, part);
        }
    });
}

fn chip(
    ui: &mut Ui,
    style: &mut PlayerStatsStyle,
    selected: &mut Option<StatsPart>,
    part: StatsPart,
) {
    let spec = part.spec();
    let edit = style.part(part);
    let visible = !edit.hidden;
    let is_selected = *selected == Some(part);
    let changed = style.part_changed_count(part) > 0;
    let galley = ui
        .ctx()
        .fonts_mut(|f| f.layout_no_wrap(spec.label.to_string(), FontId::proportional(12.0), TEXT));
    let dot = if changed { 10.0 } else { 0.0 };
    let width = 6.0 + 16.0 + 4.0 + galley.size().x + dot + 8.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, 22.0), Sense::hover());
    let eye_rect = Rect::from_min_size(rect.min + vec2(6.0, 3.0), Vec2::splat(16.0));
    let name_rect = Rect::from_min_max(pos2(eye_rect.right(), rect.top()), rect.max);
    let eye = ui
        .interact(eye_rect, ui.id().with(("stats-eye", part)), Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(if visible { "Hide" } else { "Show" });
    let name = ui
        .interact(
            name_rect,
            ui.id().with(("stats-chip", part)),
            Sense::click(),
        )
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(spec.blurb);
    let hovered = eye.hovered() || name.hovered();
    let (fill, stroke) = if is_selected {
        (ACCENT.gamma_multiply(0.16), Stroke::new(1.0, ACCENT))
    } else if hovered {
        (CARD_HOVER, Stroke::new(1.0, BORDER))
    } else {
        (CARD, Stroke::new(1.0, BORDER))
    };
    let p = ui.painter();
    p.rect(
        rect,
        CornerRadius::same(255),
        fill,
        stroke,
        StrokeKind::Inside,
    );
    crate::hud_view::eye_glyph(p, eye_rect.center(), visible, eye.hovered());
    p.galley(
        pos2(
            name_rect.left() + 4.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        if visible { TEXT } else { WEAK },
    );
    if changed {
        p.circle_filled(pos2(rect.right() - 11.0, rect.center().y), 3.0, ACCENT);
    }
    if eye.clicked() {
        style.set_part(
            part,
            PartEdit {
                hidden: visible,
                ..edit
            },
        );
    }
    if name.clicked() {
        *selected = Some(part);
    }
}

fn inspector(ui: &mut Ui, style: &mut PlayerStatsStyle, selected: &mut Option<StatsPart>) {
    egui::Frame::new()
        .fill(RAIL)
        .corner_radius(CornerRadius::same(theme::RADIUS))
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let Some(part) = *selected else {
                widgets::caption(ui, "Parts");
                ui.add_space(2.0);
                widgets::hint(ui, "Click a part on the preview, or pick one here.");
                ui.add_space(6.0);
                for part in StatsPart::ALL {
                    let n = style.part_changed_count(part);
                    let text = if n > 0 {
                        RichText::new(format!("{}  ·  {n} changed", part.spec().label))
                            .color(ACCENT)
                    } else {
                        RichText::new(part.spec().label)
                    };
                    if ui.selectable_label(false, text).clicked() {
                        *selected = Some(part);
                    }
                }
                return;
            };
            part_inspector(ui, style, part);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Done").clicked() {
                    *selected = None;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            style.part_changed_count(part) > 0,
                            egui::Button::new("Reset this part"),
                        )
                        .clicked()
                    {
                        style.reset_part(part);
                    }
                });
            });
        });
}

fn part_inspector(ui: &mut Ui, style: &mut PlayerStatsStyle, part: StatsPart) {
    let spec = part.spec();
    widgets::caption(ui, "Part");
    ui.label(RichText::new(spec.label).size(17.0).strong().color(TEXT));
    widgets::hint(ui, spec.blurb);
    ui.add_space(6.0);
    caption(ui, "Look");
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(5.0, 5.0);
        for look in part.looks() {
            let on = look.is_on(style, part);
            if widgets::chip(ui, look.label, ACCENT, Some(on))
                .on_hover_text(look.blurb)
                .clicked()
                && !on
            {
                *style = look.applied(style, part);
            }
        }
    });
    ui.add_space(4.0);
    let mut edit = style.part(part);
    caption(ui, "Show");
    if let Some(i) = widgets::segmented(ui, &["Visible", "Hidden"], usize::from(edit.hidden)) {
        edit.hidden = i == 1;
    }
    ui.add_enabled_ui(!edit.hidden, |ui| {
        caption(ui, "Size");
        let mut scale = edit.scale();
        if slider(ui, &mut scale, SCALE_RANGE, "%", 100) {
            edit.scale_pct = scale;
        }
        caption(ui, "Opacity");
        let mut opacity = edit.opacity_pct.unwrap_or(spec.vanilla_opacity);
        if slider(ui, &mut opacity, 0..=100, "%", spec.vanilla_opacity) {
            edit.opacity_pct = (opacity != spec.vanilla_opacity).then_some(opacity);
        }
        caption(ui, "Position");
        ui.horizontal(|ui| {
            for (axis, value) in [("X", &mut edit.offset_x), ("Y", &mut edit.offset_y)] {
                ui.label(RichText::new(axis).color(WEAK));
                ui.add(
                    egui::DragValue::new(value)
                        .range(OFFSET_RANGE)
                        .speed(1.0)
                        .suffix(" px"),
                );
                ui.add_space(6.0);
            }
            if (edit.offset_x != 0 || edit.offset_y != 0) && widgets::reset_pill(ui).clicked() {
                edit.offset_x = 0;
                edit.offset_y = 0;
            }
        });
        widgets::hint(ui, "Game pixels at 1080p from where the game puts it.");
    });
    style.set_part(part, edit);
    ui.add_space(6.0);
    ui.add_enabled_ui(!edit.hidden, |ui| details(ui, style, part));
}

fn details(ui: &mut Ui, style: &mut PlayerStatsStyle, part: StatsPart) {
    let d = PlayerStatsStyle::default();
    match part {
        StatsPart::Numbers => {
            caption(ui, "Numbers");
            px_row(
                ui,
                "Size",
                &mut style.number_px,
                NUMBER_PX_RANGE,
                d.number_px,
            );
            font_row(ui, &mut style.number_font);
            color_row(ui, "Colour", &mut style.number_color, OFF_WHITE);
            switch_row(
                ui,
                "Straight",
                &mut style.straight_numbers,
                "The game tilts them 3 degrees.",
            );
            caption(ui, "Icons");
            ui.horizontal(|ui| {
                ui.label(RichText::new("Opacity").color(WEAK));
                slider(
                    ui,
                    &mut style.icon_opacity_pct,
                    0..=100,
                    "%",
                    d.icon_opacity_pct,
                );
            });
            color_row(ui, "Weapon", &mut style.weapon_color, CATEGORY[0]);
            color_row(ui, "Spirit", &mut style.spirit_color, CATEGORY[1]);
            color_row(ui, "Vitality", &mut style.vitality_color, CATEGORY[2]);
            widgets::hint(
                ui,
                "Category colours also tint the corners of your item tiles.",
            );
            caption(ui, "Extras");
            switch_row(
                ui,
                "Hide change numbers",
                &mut style.hide_deltas,
                "The small +18 after a stat changes.",
            );
            switch_row(
                ui,
                "Hide bars",
                &mut style.hide_bars,
                "The thin bar beside each number.",
            );
            switch_row(
                ui,
                "Hide glow",
                &mut style.hide_glow,
                "The flare behind a number that spikes or maxes out.",
            );
        }
        StatsPart::Level => {
            caption(ui, "Level");
            px_row(ui, "Size", &mut style.level_px, NUMBER_PX_RANGE, d.level_px);
            font_row(ui, &mut style.level_font);
            color_row(ui, "Colour", &mut style.level_color, OFF_WHITE);
            color_row(ui, "Jar", &mut style.jar_color, Color32::WHITE);
            switch_row(
                ui,
                "Hide the jar",
                &mut style.hide_jar,
                "The soul jar round the level; the number stays.",
            );
        }
        StatsPart::Souls => {
            caption(ui, "Souls");
            px_row(ui, "Size", &mut style.souls_px, SOULS_PX_RANGE, d.souls_px);
            font_row(ui, &mut style.souls_font);
            color_row(ui, "Colour", &mut style.souls_color, SHARD);
            switch_row(
                ui,
                "Hide icon",
                &mut style.hide_souls_icon,
                "The soul symbol before the number.",
            );
            switch_row(
                ui,
                "Hide SOULS label",
                &mut style.hide_souls_label,
                "The faint word after the number.",
            );
        }
        StatsPart::Items => {
            caption(ui, "Tiles");
            px_row(
                ui,
                "Gap",
                &mut style.tile_gap_px,
                TILE_GAP_RANGE,
                d.tile_gap_px,
            );
            px_row(
                ui,
                "Corners",
                &mut style.tile_radius_px,
                TILE_RADIUS_RANGE,
                d.tile_radius_px,
            );
            switch_row(
                ui,
                "Black and white items",
                &mut style.mono_items,
                "Item pictures without colour; tier corners keep theirs.",
            );
            ui.horizontal(|ui| {
                ui.label(RichText::new("Empty slots").color(WEAK));
                slider(
                    ui,
                    &mut style.empty_opacity_pct,
                    0..=100,
                    "%",
                    d.empty_opacity_pct,
                );
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Cooldown cover").color(WEAK));
                slider(ui, &mut style.cooldown_pct, 0..=100, "%", d.cooldown_pct);
            });
            switch_row(
                ui,
                "Hide tier corners",
                &mut style.hide_tiers,
                "The coloured corner with the tier numeral.",
            );
            switch_row(
                ui,
                "Hide upgrade pips",
                &mut style.hide_upgrades,
                "The green +N on upgraded items.",
            );
            switch_row(
                ui,
                "Hide flex slots",
                &mut style.hide_flex,
                "The list of locked flex slot items beside the grid; empty in most games.",
            );
        }
        StatsPart::Popups | StatsPart::Quickbuy | StatsPart::StatusEffects => {}
    }
}

fn caption(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    widgets::caption(ui, text);
}

/// A slider with its value and unit; highlighted away from `default`. True when moved.
fn slider<T>(
    ui: &mut Ui,
    value: &mut T,
    range: std::ops::RangeInclusive<T>,
    unit: &str,
    default: T,
) -> bool
where
    T: egui::emath::Numeric + PartialEq + std::fmt::Display,
{
    let before = *value;
    ui.horizontal(|ui| {
        ui.spacing_mut().slider_width = (ui.available_width() - 56.0).max(80.0);
        ui.add(egui::Slider::new(value, range).show_value(false));
        ui.label(
            RichText::new(format!("{value}{unit}"))
                .size(12.5)
                .color(if *value != default { ACCENT } else { TEXT }),
        );
    });
    *value != before
}

fn font_row(ui: &mut Ui, value: &mut StatFont) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            vec2(70.0, 20.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.set_min_width(70.0);
                ui.label(RichText::new("Font").color(WEAK));
            },
        );
        let labels: Vec<&str> = StatFont::ALL.iter().map(|f| f.label()).collect();
        let current = StatFont::ALL.iter().position(|f| f == value).unwrap_or(0);
        if let Some(i) = widgets::segmented(ui, &labels, current) {
            *value = StatFont::ALL[i];
        }
    });
}

fn px_row(
    ui: &mut Ui,
    label: &str,
    value: &mut u8,
    range: std::ops::RangeInclusive<u8>,
    default: u8,
) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            vec2(70.0, 20.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.set_min_width(70.0);
                ui.label(RichText::new(label).color(WEAK));
            },
        );
        slider(ui, value, range, " px", default);
    });
}

fn color_row(ui: &mut Ui, label: &str, value: &mut Option<Color>, game: Color32) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            vec2(70.0, 20.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.set_min_width(70.0);
                ui.label(marked(label, value.is_some()).color(if value.is_some() {
                    ACCENT
                } else {
                    WEAK
                }));
            },
        );
        let before = value.map_or(game, to32);
        let mut picked = before;
        ui.spacing_mut().interact_size = vec2(40.0, 18.0);
        color_edit_button_srgba(ui, &mut picked, Alpha::Opaque);
        if picked != before {
            let [r, g, b, a] = picked.to_srgba_unmultiplied();
            *value = Some(Color([r, g, b, a]));
        }
        if value.is_some() {
            if widgets::reset_pill(ui)
                .on_hover_text("Back to the game's colour")
                .clicked()
            {
                *value = None;
            }
        } else {
            ui.label(RichText::new("game's").size(11.0).color(WEAK));
        }
    });
}

fn switch_row(ui: &mut Ui, label: &str, value: &mut bool, help: &str) {
    ui.horizontal(|ui| {
        if widgets::switch(ui, *value).clicked() {
            *value = !*value;
        }
        ui.label(marked(label, *value)).on_hover_text(help);
    });
}

fn credits(ui: &mut Ui) {
    ui.add_space(4.0);
    ui.label(
        RichText::new(
            "Built from the game's own panels: the category numbers, souls and level, the \
             item grid, quickbuy and status effects keep working as the game made them; \
             DeadTune only places and styles them.",
        )
        .size(11.5)
        .color(WEAK),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(state: &mut AppState, width: f32) {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 900.0))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| page(ui, state));
        output.textures_delta.clear();
    }

    #[test]
    fn page_renders_wide_and_narrow_with_and_without_a_selection() {
        let (_dir, mut state) = crate::state::testutil::state();
        for width in [1300.0, 600.0] {
            render(&mut state, width);
        }
        assert!(!state.is_dirty(), "drawing alone changes nothing");
        for part in StatsPart::ALL {
            state.ui.stats_selected = Some(part);
            render(&mut state, 1300.0);
        }
        assert!(!state.is_dirty());
        state.set_player_stats_style(StatsPreset::Compact.style());
        render(&mut state, 1300.0);
        assert_eq!(
            state.profile.hud.player_stats.preset(),
            Some(StatsPreset::Compact)
        );
    }

    #[test]
    fn a_part_moves_and_sizes_about_its_pivot() {
        let xf = Xf {
            pivot: pos2(100.0, 100.0),
            s: 2.0,
            off: vec2(10.0, -5.0),
        };
        assert_eq!(xf.at(pos2(100.0, 100.0)), pos2(110.0, 95.0));
        assert_eq!(xf.at(pos2(110.0, 100.0)), pos2(130.0, 95.0));
    }

    #[test]
    fn the_item_grid_widens_with_the_gap() {
        let base = items_layout(&PlayerStatsStyle::default());
        assert!((base.pitch - 61.2).abs() < 0.01);
        assert!(
            (base.origin.x - 25.6).abs() < 0.01,
            "first tile at 25.6 like the screenshot"
        );
        let wide = items_layout(&PlayerStatsStyle {
            tile_gap_px: 8,
            ..PlayerStatsStyle::default()
        });
        assert!(wide.bounds.width() > base.bounds.width());
        assert_eq!(base.bounds.bottom(), STAGE.y - 16.0);
    }
}
