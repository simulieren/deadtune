//! HUD layout page: a 16:9 monitor you place HUD pieces on, over a game screenshot at
//! default settings (`game_shot`) or, with that off, as tiles (the minimap, top bar and health
//! bar drawn from the game's images when they load), an inspector for the selected piece,
//! layout presets and an element list. Edits live in the profile and go out with the
//! normal Apply.

use std::ops::RangeInclusive;
use std::sync::Arc;

use dt_core::addons::guard::PakState;
use dt_core::hud::art;
use dt_core::hud::elements::{self, ElementId, ElementSpec};
use dt_core::hud::layout::{self, ElementEdit, OFFSET_LIMIT, PreviewRect, Visibility};
use eframe::egui::epaint::Mesh;
use eframe::egui::text::{Galley, LayoutJob, TextWrapping};
use eframe::egui::{
    self, Align, Color32, CornerRadius, CursorIcon, FontId, Id, Key, Layout, Margin, Painter, Pos2,
    Rect, RichText, Sense, Shape, Stroke, StrokeKind, TextureHandle, Ui, Vec2, pos2, vec2,
};

use crate::game_shot;
use crate::hud_art::Images;
use crate::live_hud::{HELLO_WAIT, LiveHud};
use crate::state::{AppState, Backdrop, HudPage, HudPreset};
use crate::theme::{self, ACCENT, BAD, BORDER, CARD, CARD_HOVER, GOOD, RAIL, TEXT, WARN, WEAK};

const INSPECTOR_WIDTH: f32 = 250.0;
const BEZEL: f32 = 6.0;
const HANDLE: f32 = 8.0;
/// Screen px within which a dragged edge or centre sticks to the frame's.
const SNAP: f32 = 7.0;
/// `team2Color` and `team1Color` in the game's base styles; the player's team is on the left.
const TEAM_SAPPHIRE: Color32 = Color32::from_rgb(0x4D, 0x75, 0xC3);
const TEAM_AMBER: Color32 = Color32::from_rgb(0xD4, 0x86, 0x0B);
/// Narrower than `layout::SCALE_RANGE`: anything past this is unreadable or off screen.
pub const SIZE_RANGE: RangeInclusive<u16> = 50..=200;
const HINT: &str =
    "Drag to move. Corner handles or scroll to resize. Arrow keys nudge, Shift for 10 px.";
const LIVE_HINT: &str = "A small DeadTune script in the HUD pak restyles the running game as you \
     edit. Off in Vanilla. Needs one Apply and restart to install.";

enum Action {
    Select(Option<ElementId>),
    Set(ElementId, ElementEdit),
    Preset(HudPreset),
    ResetAll,
    Backdrop(Backdrop),
    LivePreview(bool),
}

/// Where a drag began, kept in egui's temp memory for the drag's duration. A move has
/// `reach == 0`; a corner handle scales by the pointer's distance from `anchor`
/// relative to `reach`, the handle's own starting distance.
#[derive(Clone)]
struct DragStart {
    edit: ElementEdit,
    anchor: Pos2,
    reach: f32,
}

enum Guide {
    V(f32),
    H(f32),
}

pub fn hud(ui: &mut Ui, state: &mut AppState) {
    let pages = [
        HudPage::Layout,
        HudPage::Colors,
        HudPage::TopBar,
        HudPage::Health,
        HudPage::Ingame,
        HudPage::Images,
    ];
    let selected = pages
        .iter()
        .position(|p| *p == state.ui.hud_page)
        .unwrap_or(0);
    if let Some(i) = crate::widgets::segmented(
        ui,
        &[
            "Layout",
            "Minimap",
            "Top bar",
            "Health bar",
            "In-game settings",
            "UI images",
        ],
        selected,
    ) {
        state.ui.hud_page = pages[i];
    }
    ui.add_space(8.0);
    match state.ui.hud_page {
        HudPage::Layout => layout_page(ui, state),
        HudPage::Colors => crate::minimap_view::page(ui, state),
        HudPage::TopBar => crate::topbar_view::page(ui, state),
        HudPage::Health => crate::health_view::page(ui, state),
        HudPage::Ingame => crate::ingame_view::page(ui, state),
        HudPage::Images => crate::images_view::page(ui, state),
    }
}

/// One line on what the live HUD preview is doing, at the top of every HUD page.
pub fn live_indicator(ui: &mut Ui, state: &mut AppState) {
    let mut undo = false;
    let line = |text: &str, color: Color32| RichText::new(text).small().color(color);
    match state.live_hud.state() {
        LiveHud::Off => return,
        LiveHud::NotInstalled => {
            ui.label(line(
                "Turn on Live HUD preview, Apply, and restart the game once",
                WEAK,
            ));
        }
        LiveHud::GameClosed => {
            ui.label(line("Live preview starts when the game runs", WEAK));
        }
        LiveHud::Waiting { since } => {
            let text = if since.elapsed() < HELLO_WAIT {
                "Looking for the live script in game\u{2026}"
            } else {
                "Looking for the live script in game\u{2026} If it doesn't show up, launch the \
                 game through DeadTune so -condebug is on."
            };
            ui.label(line(text, WEAK));
        }
        LiveHud::Stale { .. } => {
            ui.label(line(
                "The game is running an older HUD; restart it to preview live",
                WARN,
            ));
        }
        LiveHud::Live {
            base,
            acked,
            undone,
            ..
        } => {
            let answer = match acked {
                Some(at) => format!(
                    "Last answer from the game {} s ago.",
                    at.elapsed().as_secs()
                ),
                None => "No answer from the game yet.".to_string(),
            };
            let hover = format!("HUD {base} is running. {answer}");
            ui.horizontal(|ui| {
                crate::live_status::dot(ui, GOOD).on_hover_text(&hover);
                ui.label(line("Live in game", GOOD)).on_hover_text(&hover);
                let tip = if *undone {
                    "The game shows what Apply installed until your next edit"
                } else {
                    "Show what Apply installed, without your edits since"
                };
                undo = ui
                    .add_enabled(
                        !*undone,
                        egui::Button::new(RichText::new("Undo live changes").small()).small(),
                    )
                    .on_hover_text(tip)
                    .on_disabled_hover_text(tip)
                    .clicked();
            });
            let missing = state.live_hud.not_live();
            if !missing.is_empty() {
                let labels: Vec<&str> = missing.iter().map(|(label, _)| *label).collect();
                let why: Vec<String> = missing
                    .iter()
                    .map(|(label, why)| format!("{label}: {why}"))
                    .collect();
                ui.label(line(
                    &format!("Needs Apply and a restart: {}", labels.join(", ")),
                    WEAK,
                ))
                .on_hover_text(why.join("\n"));
            }
        }
        LiveHud::Error(message) => {
            ui.label(line(message, BAD));
        }
    }
    if undo {
        state.undo_live_hud();
    }
}

/// Why HUD changes are not reaching the game, and what the launch guard knows about them.
pub fn hud_error(ui: &mut Ui, state: &mut AppState) {
    live_indicator(ui, state);
    if let Some(e) = state.hud_error() {
        ui.colored_label(
            WARN,
            "HUD changes can't be applied right now; other settings still apply.",
        )
        .on_hover_text(e);
    }
    if state.hud_held() {
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(
                WARN,
                "These HUD changes stopped the game from starting, so DeadTune is leaving them out. \
                 Change a HUD setting, or try them again.",
            );
            if !state.ctx.game_running && ui.small_button("Try again").clicked() {
                state.retry_hud();
            }
        });
        return;
    }
    let line = match state.hud_trial_state() {
        Some(PakState::OnTrial) => {
            "Deadlock is testing these HUD changes on this launch.".to_string()
        }
        Some(PakState::Untried) => {
            "Tested on the next launch; if Deadlock fails to start, DeadTune turns them off."
                .to_string()
        }
        Some(PakState::Verified { at }) if at > 0 => format!(
            "Deadlock started fine with these HUD changes on {}.",
            crate::addons_view::date(at)
        ),
        _ => return,
    };
    ui.label(RichText::new(line).small().color(WEAK));
}

/// What the HUD layout does to `id`, for the page that styles it: hidden there means the
/// page shows nothing in game; a layout scale or fade stacks with the page's own sizes.
pub fn layout_note(edit: &ElementEdit) -> Option<(String, bool)> {
    if edit.visibility == Visibility::Hidden {
        return Some((
            "Hidden on the HUD page, so nothing here shows in game until you show it again.".into(),
            true,
        ));
    }
    let mut parts = Vec::new();
    if edit.scale_pct != 100 {
        parts.push(format!("sizes it to {}%", edit.scale_pct));
    }
    if edit.opacity_pct != 100 {
        parts.push(format!("fades it to {}%", edit.opacity_pct));
    }
    (!parts.is_empty()).then(|| {
        (
            format!(
                "The HUD page also {}; that adds to the settings here.",
                parts.join(" and ")
            ),
            false,
        )
    })
}

pub fn show_layout_note(ui: &mut Ui, state: &AppState, id: ElementId) {
    if let Some((text, hidden)) = layout_note(&state.hud_edit(id)) {
        ui.colored_label(if hidden { WARN } else { WEAK }, text);
        ui.add_space(4.0);
    }
}

pub fn layout_page(ui: &mut Ui, state: &mut AppState) {
    hud_error(ui, state);
    let mut actions = Vec::new();
    toolbar(ui, state, &mut actions);
    ui.add_space(6.0);
    let ctx = ui.ctx().clone();
    crate::hud_art::with(&ctx, state, |state, images| {
        if ui.available_width() >= 720.0 {
            ui.horizontal_top(|ui| {
                let gap = 12.0;
                let left = ui.available_width() - INSPECTOR_WIDTH - gap;
                let width = monitor_width(ui, left);
                ui.vertical(|ui| {
                    ui.set_width(left);
                    canvas(ui, state, images, width, &mut actions);
                    backdrop_row(ui, state, &mut actions);
                    ui.add_space(4.0);
                    element_list(ui, state, &mut actions);
                });
                ui.add_space(gap - ui.spacing().item_spacing.x);
                ui.vertical(|ui| {
                    ui.set_width(INSPECTOR_WIDTH);
                    inspector(ui, state, monitor_height(width), &mut actions);
                });
            });
        } else {
            let width = monitor_width(ui, ui.available_width());
            canvas(ui, state, images, width, &mut actions);
            backdrop_row(ui, state, &mut actions);
            ui.add_space(4.0);
            element_list(ui, state, &mut actions);
            ui.add_space(8.0);
            inspector(ui, state, 0.0, &mut actions);
        }
    });
    nudge(ui, state, &mut actions);
    run(state, actions);
}

fn run(state: &mut AppState, actions: Vec<Action>) {
    for action in actions {
        match action {
            Action::Select(id) => state.ui.hud_selected = id,
            Action::Set(id, edit) => state.set_hud_element(id, edit),
            Action::Preset(p) => state.apply_hud_preset(p),
            Action::ResetAll => state.apply_hud_preset(HudPreset::Vanilla),
            Action::Backdrop(b) => state.ui.hud_backdrop = b,
            Action::LivePreview(on) => state.set_live_preview(on),
        }
    }
}

fn caption(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text.to_uppercase())
            .size(11.0)
            .strong()
            .color(WEAK),
    );
}

fn toolbar(ui: &mut Ui, state: &AppState, actions: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        caption(ui, "Layout");
        let current = state.hud_preset();
        for preset in HudPreset::ALL {
            if ui
                .selectable_label(current == Some(preset), preset.label())
                .on_hover_text(preset.blurb())
                .clicked()
            {
                actions.push(Action::Preset(preset));
            }
        }
        ui.add_space(10.0);
        let mut live = state.profile.hud.live;
        if ui
            .checkbox(&mut live, "Live HUD preview")
            .on_hover_text(LIVE_HINT)
            .changed()
        {
            actions.push(Action::LivePreview(live));
        }
        ui.add_space(10.0);
        let changed = state.hud_changed_count();
        let readout = RichText::new(format!("Changed: {changed}")).small();
        ui.label(if changed > 0 {
            readout.color(ACCENT)
        } else {
            readout.color(WEAK)
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add_enabled(changed > 0, egui::Button::new("Reset layout"))
                .on_hover_text("Back to the game's own layout")
                .clicked()
            {
                actions.push(Action::ResetAll);
            }
        });
    });
}

fn monitor_height(width: f32) -> f32 {
    (width - 2.0 * BEZEL) * 9.0 / 16.0 + 2.0 * BEZEL
}

/// As wide as `max`, unless that would push the element list under it off screen.
fn monitor_width(ui: &Ui, max: f32) -> f32 {
    let room = (ui.clip_rect().bottom() - ui.cursor().top() - 66.0).max(180.0);
    max.min((room - 2.0 * BEZEL) * 16.0 / 9.0 + 2.0 * BEZEL)
}

/// Draws the monitor and handles every pointer interaction on it. Returns the
/// screen rect (the 16:9 area inside the bezel).
fn canvas(
    ui: &mut Ui,
    state: &AppState,
    images: &mut Images,
    width: f32,
    actions: &mut Vec<Action>,
) -> Rect {
    let outer_size = vec2(width, monitor_height(width));
    let (outer, _) = ui.allocate_exact_size(outer_size, Sense::hover());
    let screen = outer.shrink(BEZEL);
    let painter = ui.painter();
    painter.rect(
        outer,
        CornerRadius::same(10),
        RAIL,
        Stroke::new(1.0, BORDER),
        StrokeKind::Inside,
    );
    backdrop(&painter.with_clip_rect(screen), screen);
    let painter = painter.with_clip_rect(screen);

    if ui
        .interact(screen, ui.id().with("hud_bg"), Sense::click())
        .clicked()
    {
        actions.push(Action::Select(None));
    }
    let k = screen.height() / 1080.0;
    let items = layout::preview(&state.profile.hud, [screen.width(), screen.height()]);
    let backdrop = state.ui.hud_backdrop;
    let shot = if backdrop.shown {
        game_shot::texture(ui.ctx())
    } else {
        None
    };
    if let Some(texture) = &shot {
        let tint = Color32::WHITE.gamma_multiply(backdrop.opacity);
        game_shot::paint(&painter, texture, [0.0, 0.0, 1.0, 1.0], screen, tint);
        for item in &items {
            if let Some(crop) = layout::reference_crop(item.id)
                && state.hud_edit(item.id) != ElementEdit::default()
            {
                let spot = crop_rect(screen, crop);
                let shade = Color32::from_black_alpha((220.0 * backdrop.opacity) as u8);
                painter.rect_filled(spot, CornerRadius::same(3), shade);
            }
        }
    }
    let selected = state.ui.hud_selected;
    let drag_key = ui.id().with("hud_drag");
    let mut guides = Vec::new();
    // Selected last, so its outline and handles sit on top of neighbours.
    let order = items
        .iter()
        .filter(|i| Some(i.id) != selected)
        .chain(items.iter().filter(|i| Some(i.id) == selected));
    for item in order {
        let spec = elements::spec(item.id);
        let [x, y, w, h] = item.rect;
        let rect = Rect::from_min_size(screen.min + vec2(x, y), vec2(w, h));
        let is_selected = selected == Some(item.id);
        let response = ui
            .interact(
                rect,
                ui.id().with(("hud_el", item.id)),
                Sense::click_and_drag(),
            )
            .on_hover_cursor(CursorIcon::Grab);
        let look = if is_selected {
            Look::Selected
        } else if response.hovered() {
            Look::Hovered
        } else {
            Look::Plain
        };
        match (&shot, layout::reference_crop(item.id)) {
            (Some(texture), Some(crop)) => {
                let shot = Shot {
                    texture,
                    crop,
                    opacity: backdrop.opacity,
                    edited: state.hud_edit(item.id) != ElementEdit::default(),
                };
                shot_tile(ui, &painter, screen, rect, spec, item, look, shot);
            }
            _ => tile(
                ui,
                &painter,
                images,
                screen,
                rect,
                spec,
                item,
                look,
                shot.is_some(),
            ),
        }
        if response.clicked() || response.drag_started() {
            actions.push(Action::Select(Some(item.id)));
        }
        if response.hovered() {
            wheel(ui, state, item.id, actions);
        }
        if response.drag_started() {
            let start = DragStart {
                edit: state.hud_edit(item.id),
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
            let current = state.hud_edit(item.id);
            let mut edit = start.edit.clone();
            let delta = (pos - origin) / k;
            edit.offset_x = start.edit.offset_x + delta.x.round() as i32;
            edit.offset_y = start.edit.offset_y + delta.y.round() as i32;
            let unmoved = rect.min - vec2(current.offset_x as f32, current.offset_y as f32) * k;
            let candidate = Rect::from_min_size(
                unmoved + vec2(edit.offset_x as f32, edit.offset_y as f32) * k,
                rect.size(),
            );
            if let Some((shift, line)) = snap(
                [candidate.left(), candidate.center().x, candidate.right()],
                [screen.left(), screen.center().x, screen.right()],
            ) {
                edit.offset_x += (shift / k).round() as i32;
                guides.push(Guide::V(line));
            }
            if let Some((shift, line)) = snap(
                [candidate.top(), candidate.center().y, candidate.bottom()],
                [screen.top(), screen.center().y, screen.bottom()],
            ) {
                edit.offset_y += (shift / k).round() as i32;
                guides.push(Guide::H(line));
            }
            edit.offset_x = edit.offset_x.clamp(-OFFSET_LIMIT, OFFSET_LIMIT);
            edit.offset_y = edit.offset_y.clamp(-OFFSET_LIMIT, OFFSET_LIMIT);
            actions.push(Action::Set(item.id, edit));
        }
        if is_selected && item.visible {
            handles(ui, &painter, rect, item.id, state, drag_key, actions);
        }
    }
    let guide_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.7));
    for guide in guides {
        match guide {
            Guide::V(x) => painter.vline(x, screen.y_range(), guide_stroke),
            Guide::H(y) => painter.hline(screen.x_range(), y, guide_stroke),
        };
    }
    screen
}

/// The smallest edge-or-centre shift under `SNAP`, and the line it lands on.
fn snap(edges: [f32; 3], lines: [f32; 3]) -> Option<(f32, f32)> {
    edges
        .iter()
        .flat_map(|e| lines.iter().map(move |l| (l - e, *l)))
        .filter(|(d, _)| d.abs() <= SNAP)
        .min_by(|a, b| a.0.abs().total_cmp(&b.0.abs()))
}

fn wheel(ui: &mut Ui, state: &AppState, id: ElementId, actions: &mut Vec<Action>) {
    let delta = ui.input(|i| i.smooth_scroll_delta.y);
    if delta == 0.0 {
        return;
    }
    // Taken here so the page's scroll area does not also move.
    ui.input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
    let step = (delta / 8.0).abs().ceil().min(10.0) * delta.signum();
    let mut edit = state.hud_edit(id);
    edit.scale_pct = clamp_size(i32::from(edit.scale_pct) + step as i32);
    actions.push(Action::Set(id, edit));
}

fn clamp_size(pct: i32) -> u16 {
    pct.clamp(i32::from(*SIZE_RANGE.start()), i32::from(*SIZE_RANGE.end())) as u16
}

fn handles(
    ui: &mut Ui,
    painter: &Painter,
    rect: Rect,
    id: ElementId,
    state: &AppState,
    drag_key: Id,
    actions: &mut Vec<Action>,
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
            .interact(handle, ui.id().with(("hud_handle", id, i)), Sense::drag())
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
                edit: state.hud_edit(id),
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
            let mut edit = start.edit.clone();
            edit.scale_pct = clamp_size((f32::from(start.edit.scale_pct) * factor).round() as i32);
            actions.push(Action::Set(id, edit));
        }
    }
}

fn backdrop(p: &Painter, r: Rect) {
    let top = Color32::from_rgb(40, 43, 52);
    let bottom = Color32::from_rgb(19, 20, 25);
    let mut sky = Mesh::default();
    sky.colored_vertex(r.left_top(), top);
    sky.colored_vertex(r.right_top(), top);
    sky.colored_vertex(r.right_bottom(), bottom);
    sky.colored_vertex(r.left_bottom(), bottom);
    sky.add_triangle(0, 1, 2);
    sky.add_triangle(0, 2, 3);
    p.add(sky);
    let mut glow = Mesh::default();
    glow.colored_vertex(r.center(), ACCENT.gamma_multiply(0.07));
    let n = 24u32;
    for i in 0..n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        let rim = r.center() + vec2(a.cos() * r.width() * 0.55, a.sin() * r.height() * 0.7);
        glow.colored_vertex(rim, Color32::TRANSPARENT);
    }
    for i in 0..n {
        glow.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    p.add(glow);
    let grid = Stroke::new(1.0, TEXT.gamma_multiply(0.06));
    for i in 1..3 {
        let t = i as f32 / 3.0;
        p.vline(r.left() + r.width() * t, r.y_range(), grid);
        p.hline(r.x_range(), r.top() + r.height() * t, grid);
    }
    let c = r.center();
    let cross = Stroke::new(1.0, TEXT.gamma_multiply(0.4));
    for dir in [
        vec2(1.0, 0.0),
        vec2(-1.0, 0.0),
        vec2(0.0, 1.0),
        vec2(0.0, -1.0),
    ] {
        p.line_segment([c + dir * 3.0, c + dir * 9.0], cross);
    }
    p.circle_filled(c, 1.0, TEXT.gamma_multiply(0.4));
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Look {
    Plain,
    Hovered,
    Selected,
}

#[allow(clippy::too_many_arguments)]
fn tile(
    ui: &mut Ui,
    p: &Painter,
    images: &mut Images,
    screen: Rect,
    r: Rect,
    spec: &ElementSpec,
    item: &PreviewRect,
    look: Look,
    over_shot: bool,
) {
    let selected = look == Look::Selected;
    let hovered = look == Look::Hovered;
    let radius = CornerRadius::same(3);
    let ink = if selected {
        ACCENT
    } else {
        TEXT.gamma_multiply(if hovered { 0.75 } else { 0.5 })
    };
    let color = if item.visible {
        if selected { ACCENT } else { TEXT }
    } else {
        WEAK
    };
    let inner = r.shrink2(vec2(5.0, 3.0));
    let inside = fit_label(ui, inner, spec.label, color);
    if item.visible {
        let fill = if selected {
            ACCENT.gamma_multiply(0.12 + 0.12 * item.opacity)
        } else if over_shot {
            Color32::from_black_alpha((60.0 + 60.0 * item.opacity) as u8)
        } else {
            TEXT.gamma_multiply(0.07 + 0.11 * item.opacity)
        };
        p.rect_filled(r, radius, fill);
        // The sketch goes under the label when the tile has the height for both.
        let mut area = r.shrink(4.0);
        if let Some(galley) = &inside {
            let below = inner.top() + galley.size().y + 2.0;
            if r.bottom() - 4.0 - below >= 8.0 {
                area.min.y = below;
            }
        }
        let tint = Color32::WHITE.gamma_multiply(0.35 + 0.65 * item.opacity);
        if !picture(p, images, area, spec.id, tint) {
            glyph(p, area, spec.id, ink.gamma_multiply(0.55));
        }
        let stroke = if selected {
            Stroke::new(1.5, ACCENT)
        } else {
            Stroke::new(1.0, ink)
        };
        p.rect_stroke(r, radius, stroke, StrokeKind::Inside);
    } else {
        let stroke = Stroke::new(
            1.0,
            if selected {
                ACCENT
            } else {
                WEAK.gamma_multiply(0.6)
            },
        );
        let path = [
            r.left_top(),
            r.right_top(),
            r.right_bottom(),
            r.left_bottom(),
            r.left_top(),
        ];
        p.extend(Shape::dashed_line(&path, stroke, 4.0, 3.0));
    }
    match inside {
        Some(galley) => p.galley(inner.left_top(), galley, color),
        None => label_beside(ui, p, screen, r, spec.label, color),
    }
}

/// An element the game screenshot shows, drawn as its own crop of the screenshot.
struct Shot<'a> {
    texture: &'a TextureHandle,
    crop: [f32; 4],
    opacity: f32,
    edited: bool,
}

fn crop_rect(screen: Rect, [x, y, w, h]: [f32; 4]) -> Rect {
    Rect::from_min_size(
        screen.min + vec2(x * screen.width(), y * screen.height()),
        vec2(w * screen.width(), h * screen.height()),
    )
}

/// Over the screenshot the element is its own pixels: untouched it is already in the
/// backdrop, so only the outline is drawn; edited, its crop moves and scales with it while
/// its vanilla spot is shaded.
#[allow(clippy::too_many_arguments)]
fn shot_tile(
    ui: &Ui,
    p: &Painter,
    screen: Rect,
    r: Rect,
    spec: &ElementSpec,
    item: &PreviewRect,
    look: Look,
    shot: Shot,
) {
    let radius = CornerRadius::same(3);
    if !item.visible {
        let color = if look == Look::Selected { ACCENT } else { WEAK };
        let path = [
            r.left_top(),
            r.right_top(),
            r.right_bottom(),
            r.left_bottom(),
            r.left_top(),
        ];
        p.extend(Shape::dashed_line(&path, Stroke::new(1.0, color), 4.0, 3.0));
        shot_label(ui, p, screen, r, spec.label, color);
        return;
    }
    if shot.edited {
        let under = Color32::from_black_alpha((255.0 * shot.opacity) as u8);
        p.rect_filled(r, CornerRadius::ZERO, under);
        let tint = Color32::WHITE.gamma_multiply(shot.opacity * item.opacity);
        game_shot::paint(p, shot.texture, shot.crop, r, tint);
    }
    let stroke = match look {
        Look::Selected => Stroke::new(1.5, ACCENT),
        Look::Hovered => Stroke::new(1.0, TEXT.gamma_multiply(0.9)),
        Look::Plain => Stroke::new(1.0, TEXT.gamma_multiply(0.45)),
    };
    p.rect_stroke(r, radius, stroke, StrokeKind::Outside);
    if look != Look::Plain {
        let color = if look == Look::Selected { ACCENT } else { TEXT };
        shot_label(ui, p, screen, r, spec.label, color);
    }
}

/// The label on a dark chip at the box's top-left corner, or above it when the box
/// touches the top of the screen, so it reads over the screenshot.
fn shot_label(ui: &Ui, p: &Painter, screen: Rect, r: Rect, text: &str, color: Color32) {
    let galley = ui
        .ctx()
        .fonts_mut(|f| f.layout_no_wrap(text.to_string(), FontId::proportional(10.0), color));
    let pad = vec2(4.0, 1.0);
    let size = galley.size() + pad * 2.0;
    let mut min = pos2(r.left(), r.top() - size.y - 2.0);
    if min.y < screen.top() {
        min.y = r.bottom() + 2.0;
    }
    if min.y + size.y > screen.bottom() {
        min.y = r.top() + 2.0;
    }
    min.x = min
        .x
        .clamp(screen.left(), (screen.right() - size.x).max(screen.left()));
    let chip = Rect::from_min_size(min, size);
    p.rect_filled(chip, CornerRadius::same(3), Color32::from_black_alpha(200));
    p.galley(min + pad, galley, color);
}

/// The label at 11, 10 or 9 px, wrapped onto as many rows as `inner` is tall with
/// the last one elided. `None` when the tile is too small for even that.
fn fit_label(ui: &Ui, inner: Rect, text: &str, color: Color32) -> Option<Arc<Galley>> {
    if inner.width() < 24.0 || inner.height() < 10.0 {
        return None;
    }
    for size in [11.0, 10.0, 9.0] {
        let rows = ((inner.height() / (size * 1.25)).floor() as usize).max(1);
        let mut job = LayoutJob::simple(
            text.to_string(),
            FontId::proportional(size),
            color,
            inner.width(),
        );
        job.wrap = TextWrapping {
            max_width: inner.width(),
            max_rows: rows,
            break_anywhere: rows == 1,
            overflow_character: Some('…'),
        };
        let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
        if !galley.elided || size <= 9.0 {
            return Some(galley);
        }
    }
    None
}

fn label_beside(ui: &Ui, p: &Painter, screen: Rect, r: Rect, text: &str, color: Color32) {
    let galley = ui
        .ctx()
        .fonts_mut(|f| f.layout_no_wrap(text.to_string(), FontId::proportional(10.0), color));
    let y = r.center().y - galley.size().y / 2.0;
    let x = if r.right() + 6.0 + galley.size().x <= screen.right() {
        r.right() + 6.0
    } else {
        r.left() - 6.0 - galley.size().x
    };
    p.galley(pos2(x, y), galley, color);
}

/// The element drawn from the game's images, for the pieces that have them; false (and
/// nothing drawn) otherwise or while they load.
fn picture(p: &Painter, images: &mut Images, r: Rect, id: ElementId, tint: Color32) -> bool {
    if r.width() < 16.0 || r.height() < 8.0 {
        return false;
    }
    match id {
        ElementId::Minimap => {
            let side = r.width().min(r.height());
            let frame = Rect::from_center_size(r.center(), Vec2::splat(side));
            let map = frame.shrink(side * 20.0 / 400.0);
            if images.get(art::MINIMAP_MAP, map.width()).is_none() {
                return false;
            }
            crate::minimap_view::base(p, images, frame, map, tint, true);
            true
        }
        ElementId::TopBar => {
            let n = art::ALLIES.len() + art::ENEMIES.len();
            let gap = r.width() * 0.2 / n as f32;
            let disc = ((r.width() * 0.8 - gap * n as f32) / n as f32).min(r.height() * 0.7);
            if images.get(art::ALLIES[0].portrait, disc).is_none() {
                return false;
            }
            let row = n as f32 * (disc + gap) + r.width() * 0.2;
            let mut x = r.center().x - row / 2.0 + disc / 2.0;
            let [w, h] = art::ALLIES[0].portrait.size.map(f32::from);
            let bottom = (r.top() + disc * h / w).min(r.bottom());
            for (i, hero) in art::ALLIES.iter().chain(&art::ENEMIES).enumerate() {
                if i == art::ALLIES.len() {
                    x += r.width() * 0.2;
                }
                let team = if i < art::ALLIES.len() {
                    TEAM_SAPPHIRE
                } else {
                    TEAM_AMBER
                };
                p.circle_filled(
                    pos2(x, bottom - disc / 2.0),
                    disc / 2.0,
                    team.gamma_multiply(tint.a() as f32 / 255.0),
                );
                let image = Rect::from_min_max(
                    pos2(x - disc / 2.0, bottom - disc * h / w),
                    pos2(x + disc / 2.0, bottom),
                );
                images.paint_shape(
                    p,
                    hero.portrait,
                    image,
                    &crate::hud_art::badge_mask(image),
                    tint,
                );
                x += disc + gap;
            }
            true
        }
        ElementId::HealthAndAmmo => {
            let [w, h] = art::HEALTH_FRAME.size.map(f32::from);
            let k = (r.height() / h).min(r.width() / w);
            let frame = Rect::from_center_size(r.center(), vec2(w, h) * k);
            let [bw, bh] = art::HEALTH_FILL.size.map(f32::from);
            let bar = Rect::from_center_size(frame.center(), vec2(bw, bh) * k);
            let ruler = [
                pos2(bar.left(), bar.top() + bar.height() * 0.03),
                pos2(bar.right(), bar.top()),
                pos2(bar.left() + bar.width() * 0.26, bar.bottom()),
                pos2(bar.left(), bar.bottom()),
            ];
            let frame_tint =
                Color32::from_rgb(0x14, 0x23, 0x04).gamma_multiply(tint.a() as f32 / 255.0);
            images.paint_shape(p, art::HEALTH_FILL, bar, &ruler, tint)
                && images.paint(p, art::HEALTH_FRAME, frame, frame_tint)
        }
        _ => false,
    }
}

/// A sketch of what the element looks like in game, so the tile reads as a HUD piece.
fn glyph(p: &Painter, r: Rect, id: ElementId, ink: Color32) {
    if r.width() < 16.0 || r.height() < 8.0 {
        return;
    }
    let stroke = Stroke::new(1.0, ink);
    let row = |p: &Painter, r: Rect, n: usize, round: bool| {
        let gap = 2.0;
        let cell = ((r.width() - gap * (n as f32 - 1.0)) / n as f32)
            .min(r.height())
            .max(2.0);
        let total = cell * n as f32 + gap * (n as f32 - 1.0);
        let x0 = r.center().x - total / 2.0;
        let y = r.center().y;
        for i in 0..n {
            let c = pos2(x0 + i as f32 * (cell + gap) + cell / 2.0, y);
            if round {
                p.circle_stroke(c, cell / 2.0, stroke);
            } else {
                p.rect_stroke(
                    Rect::from_center_size(c, Vec2::splat(cell)),
                    CornerRadius::same(2),
                    stroke,
                    StrokeKind::Inside,
                );
            }
        }
    };
    let lines = |p: &Painter, r: Rect, widths: &[f32]| {
        let step = r.height() / widths.len() as f32;
        for (i, w) in widths.iter().enumerate() {
            let y = r.top() + step * (i as f32 + 0.5);
            p.hline(r.left()..=r.left() + r.width() * w, y, stroke);
        }
    };
    match id {
        ElementId::TopBar => {
            let cell = (r.height() * 0.6).min(r.width() / 18.0);
            let strip = Rect::from_center_size(
                pos2(r.center().x, r.top() + cell / 2.0 + 1.0),
                vec2(r.width(), cell),
            );
            row(p, strip.split_left_right_at_fraction(0.4).0, 6, false);
            row(p, strip.split_left_right_at_fraction(0.6).1, 6, false);
            p.rect_stroke(
                Rect::from_center_size(strip.center(), vec2(cell * 2.2, cell * 0.8)),
                CornerRadius::same(255),
                stroke,
                StrokeKind::Inside,
            );
        }
        ElementId::Minimap => {
            let radius = r.width().min(r.height()) / 2.0 - 1.0;
            p.circle_stroke(r.center(), radius, stroke);
            p.circle_stroke(
                r.center(),
                radius * 0.55,
                Stroke::new(1.0, ink.gamma_multiply(0.5)),
            );
            p.circle_filled(r.center(), 2.0, ink);
        }
        ElementId::HealthAndAmmo => {
            let bar = Rect::from_center_size(r.center(), vec2(r.width() * 0.4, r.height()));
            p.rect_stroke(bar, CornerRadius::same(2), stroke, StrokeKind::Inside);
            let fill = Rect::from_min_max(
                pos2(bar.left(), bar.top() + bar.height() * 0.3),
                bar.right_bottom(),
            )
            .shrink(2.0);
            p.rect_filled(fill, CornerRadius::same(1), ink.gamma_multiply(0.6));
        }
        ElementId::AbilitySlots => row(p, r, 4, true),
        ElementId::ItemSlots => row(p, r, 4, false),
        ElementId::PassiveItems => row(p, r, 6, false),
        ElementId::PlayerStats => {
            let (top, grid) = r.split_top_bottom_at_fraction(0.3);
            lines(p, top, &[0.35]);
            let rows = 3;
            let step = grid.height() / rows as f32;
            for i in 0..rows {
                let band = Rect::from_min_size(
                    pos2(grid.left(), grid.top() + step * i as f32),
                    vec2(grid.width(), step),
                )
                .shrink2(vec2(0.0, 1.0));
                row(p, band, 4, false);
            }
        }
        ElementId::AmmoCounter => {}
        ElementId::KillFeed => lines(
            p,
            r.shrink2(vec2(0.0, r.height() * 0.15)),
            &[0.45, 0.3, 0.5],
        ),
        ElementId::Chat => {
            let (body, input) = r.split_top_bottom_at_fraction(0.75);
            lines(p, body, &[0.6, 0.4, 0.7]);
            p.rect_stroke(
                input.shrink2(vec2(0.0, 2.0)),
                CornerRadius::same(2),
                stroke,
                StrokeKind::Inside,
            );
        }
    }
}

fn backdrop_row(ui: &mut Ui, state: &AppState, actions: &mut Vec<Action>) {
    let mut backdrop = state.ui.hud_backdrop;
    ui.horizontal(|ui| {
        ui.checkbox(&mut backdrop.shown, "Game screenshot")
            .on_hover_text(
                "A real match at the game's default HUD settings, so you see where each \
                 piece sits and what your changes move",
            );
        ui.add_enabled_ui(backdrop.shown, |ui| {
            ui.label(RichText::new("Opacity").small().color(WEAK));
            ui.add(
                egui::Slider::new(&mut backdrop.opacity, 0.1..=1.0)
                    .show_value(false)
                    .clamping(egui::SliderClamping::Always),
            );
        });
    });
    if backdrop != state.ui.hud_backdrop {
        actions.push(Action::Backdrop(backdrop));
    }
}

fn element_list(ui: &mut Ui, state: &AppState, actions: &mut Vec<Action>) {
    let items = layout::preview(&state.profile.hud, [1920.0, 1080.0]);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for item in &items {
            chip(ui, state, item, actions);
        }
    });
}

fn chip(ui: &mut Ui, state: &AppState, item: &PreviewRect, actions: &mut Vec<Action>) {
    let spec = elements::spec(item.id);
    let selected = state.ui.hud_selected == Some(item.id);
    let changed = state.profile.hud.elements.contains_key(&item.id);
    let galley = ui
        .ctx()
        .fonts_mut(|f| f.layout_no_wrap(spec.label.to_string(), FontId::proportional(12.0), TEXT));
    let dot = if changed { 10.0 } else { 0.0 };
    let width = 6.0 + 16.0 + 4.0 + galley.size().x + dot + 8.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, 22.0), Sense::hover());
    let eye_rect = Rect::from_min_size(rect.min + vec2(6.0, 3.0), Vec2::splat(16.0));
    let name_rect = Rect::from_min_max(pos2(eye_rect.right(), rect.top()), rect.max);
    let eye = ui
        .interact(eye_rect, ui.id().with(("hud_eye", item.id)), Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(if item.visible { "Hide" } else { "Show" });
    let name = ui
        .interact(
            name_rect,
            ui.id().with(("hud_chip", item.id)),
            Sense::click(),
        )
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(spec.notes);
    let hovered = eye.hovered() || name.hovered();
    let (fill, stroke) = if selected {
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
    eye_glyph(p, eye_rect.center(), item.visible, eye.hovered());
    let text = if item.visible { TEXT } else { WEAK };
    p.galley(
        pos2(
            name_rect.left() + 4.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        text,
    );
    if changed {
        p.circle_filled(pos2(rect.right() - 11.0, rect.center().y), 3.0, ACCENT);
    }
    if eye.clicked() {
        let mut edit = state.hud_edit(item.id);
        edit.visibility = if item.visible {
            Visibility::Hidden
        } else if spec.vanilla.collapsed {
            Visibility::Shown
        } else {
            Visibility::Vanilla
        };
        actions.push(Action::Set(item.id, edit));
    }
    if name.clicked() {
        actions.push(Action::Select(Some(item.id)));
    }
}

pub(crate) fn eye_glyph(p: &Painter, c: Pos2, open: bool, hovered: bool) {
    let ink = if open {
        TEXT.gamma_multiply(if hovered { 1.0 } else { 0.8 })
    } else {
        WEAK.gamma_multiply(if hovered { 1.0 } else { 0.7 })
    };
    let stroke = Stroke::new(1.2, ink);
    let (w, h) = (6.0, 3.5);
    let n = 10;
    let mut upper = Vec::with_capacity(n + 1);
    let mut lower = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let t = i as f32 / n as f32 * std::f32::consts::PI;
        let x = c.x - w * t.cos();
        upper.push(pos2(x, c.y - h * t.sin()));
        lower.push(pos2(x, c.y + h * t.sin()));
    }
    p.add(Shape::line(upper, stroke));
    p.add(Shape::line(lower, stroke));
    if open {
        p.circle_filled(c, 1.8, ink);
    } else {
        p.line_segment([c + vec2(-5.0, 5.0), c + vec2(5.0, -5.0)], stroke);
    }
}

fn inspector(ui: &mut Ui, state: &AppState, min_height: f32, actions: &mut Vec<Action>) {
    egui::Frame::new()
        .fill(RAIL)
        .corner_radius(CornerRadius::same(theme::RADIUS))
        .inner_margin(Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(min_height - 24.0);
            ui.spacing_mut().item_spacing.y = 4.0;
            let Some(id) = state.ui.hud_selected else {
                caption(ui, "Element");
                ui.label(RichText::new("Click a HUD piece on the screen to edit it.").color(WEAK));
                ui.add_space(8.0);
                ui.label(RichText::new(HINT).small().color(WEAK));
                return;
            };
            let spec = elements::spec(id);
            let edit = state.hud_edit(id);
            let mut next = edit.clone();
            caption(ui, "Element");
            ui.label(
                RichText::new(spec.label)
                    .size(16.0)
                    .strong()
                    .family(theme::semibold()),
            );
            ui.label(RichText::new(blurb(id)).small().color(WEAK))
                .on_hover_text(spec.notes);
            ui.add_space(8.0);
            caption(ui, "Show");
            ui.horizontal(|ui| {
                for (mode, name) in modes(spec) {
                    if ui
                        .selectable_label(next.visibility == *mode, *name)
                        .clicked()
                    {
                        next.visibility = *mode;
                    }
                }
            });
            ui.add_space(6.0);
            caption(ui, "Size");
            percent_slider(ui, &mut next.scale_pct, SIZE_RANGE);
            caption(ui, "Opacity");
            percent_slider(ui, &mut next.opacity_pct, 0..=100);
            ui.add_space(2.0);
            caption(ui, "Position");
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(position_text(&next)).size(12.5));
                if (next.offset_x, next.offset_y) != (0, 0) && small_button(ui, "Reset position") {
                    next.offset_x = 0;
                    next.offset_y = 0;
                }
            });
            ui.add_space(10.0);
            if edit != ElementEdit::default() && ui.button("Reset this element").clicked() {
                next = ElementEdit::default();
            }
            ui.add_space(6.0);
            ui.label(RichText::new(HINT).small().color(WEAK));
            if next != edit {
                actions.push(Action::Set(id, next));
            }
        });
}

fn small_button(ui: &mut Ui, text: &str) -> bool {
    ui.add(
        egui::Button::new(RichText::new(text).small().color(ACCENT))
            .fill(ACCENT.gamma_multiply(0.14))
            .corner_radius(CornerRadius::same(255))
            .min_size(vec2(0.0, 18.0)),
    )
    .clicked()
}

fn percent_slider<T: egui::emath::Numeric>(ui: &mut Ui, value: &mut T, range: RangeInclusive<T>) {
    ui.horizontal(|ui| {
        let readout = 40.0;
        ui.spacing_mut().slider_width = ui.available_width() - readout - 8.0;
        ui.add(
            egui::Slider::new(value, range)
                .show_value(false)
                .clamping(egui::SliderClamping::Edits),
        );
        ui.label(
            RichText::new(format!("{}%", value.to_f64().round()))
                .size(12.5)
                .color(TEXT),
        );
    });
}

fn modes(spec: &ElementSpec) -> &'static [(Visibility, &'static str)] {
    if spec.vanilla.collapsed {
        &[
            (Visibility::Vanilla, "Game decides"),
            (Visibility::Shown, "Always"),
            (Visibility::Hidden, "Hidden"),
        ]
    } else {
        &[
            (Visibility::Vanilla, "Visible"),
            (Visibility::Hidden, "Hidden"),
        ]
    }
}

fn position_text(edit: &ElementEdit) -> String {
    let axis = |n: i32, pos: &str, neg: &str| {
        (n != 0).then(|| format!("{}px {}", n.abs(), if n > 0 { pos } else { neg }))
    };
    let parts: Vec<String> = [
        axis(edit.offset_x, "right", "left"),
        axis(edit.offset_y, "down", "up"),
    ]
    .into_iter()
    .flatten()
    .collect();
    if parts.is_empty() {
        "Where the game puts it".to_string()
    } else {
        format!("Moved {}", parts.join(", "))
    }
}

fn blurb(id: ElementId) -> &'static str {
    match id {
        ElementId::TopBar => "Hero portraits, souls and the match clock.",
        ElementId::Minimap => "The round map in the bottom-right corner.",
        ElementId::HealthAndAmmo => "Your health bar beside the hero.",
        ElementId::AbilitySlots => "The four ability icons under the hero.",
        ElementId::ItemSlots => "Active items 1 to 4.",
        ElementId::PassiveItems => "Passive item slots the game normally hides.",
        ElementId::PlayerStats => "Souls, item value and your bought items.",
        ElementId::AmmoCounter => "Clip count next to the crosshair.",
        ElementId::KillFeed => "Kills and objectives, upper left.",
        ElementId::Chat => "Team and all chat.",
    }
}

/// Arrow keys move the selected element while no text box has focus.
fn nudge(ui: &mut Ui, state: &AppState, actions: &mut Vec<Action>) {
    let Some(id) = state.ui.hud_selected else {
        return;
    };
    if ui.memory(|m| m.focused().is_some()) {
        return;
    }
    let (dx, dy) = ui.input_mut(|i| {
        let step = if i.modifiers.shift { 10 } else { 1 };
        let mods = i.modifiers;
        let mut d = (0, 0);
        for (key, (x, y)) in [
            (Key::ArrowLeft, (-1, 0)),
            (Key::ArrowRight, (1, 0)),
            (Key::ArrowUp, (0, -1)),
            (Key::ArrowDown, (0, 1)),
        ] {
            if i.consume_key(mods, key) {
                d.0 += x * step;
                d.1 += y * step;
            }
        }
        d
    });
    if (dx, dy) == (0, 0) {
        return;
    }
    let mut edit = state.hud_edit(id);
    edit.offset_x = (edit.offset_x + dx).clamp(-OFFSET_LIMIT, OFFSET_LIMIT);
    edit.offset_y = (edit.offset_y + dy).clamp(-OFFSET_LIMIT, OFFSET_LIMIT);
    actions.push(Action::Set(id, edit));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_note_explains_hidden_and_stacked_edits() {
        assert_eq!(layout_note(&ElementEdit::default()), None);
        let hidden = ElementEdit {
            visibility: Visibility::Hidden,
            scale_pct: 150,
            ..ElementEdit::default()
        };
        assert!(layout_note(&hidden).unwrap().1);
        let scaled = ElementEdit {
            scale_pct: 125,
            opacity_pct: 80,
            ..ElementEdit::default()
        };
        let (text, warn) = layout_note(&scaled).unwrap();
        assert!(!warn);
        assert!(text.contains("125%") && text.contains("80%"), "{text}");
    }

    const WIDTH: f32 = 720.0;

    /// Runs one frame of the canvas alone and applies what it asked for. Returns the
    /// screen rect so tests can place the pointer in reference pixels.
    fn frame(ctx: &egui::Context, state: &mut AppState, events: Vec<egui::Event>) -> Rect {
        // Held modifiers reach the input state as their own event, as eframe sends them.
        let modifiers = events
            .iter()
            .find_map(|e| match e {
                egui::Event::Key { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default();
        let mut events = events;
        events.insert(0, egui::Event::ModifiersChanged(modifiers));
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(WIDTH, 600.0))),
            events,
            ..Default::default()
        };
        let mut screen = Rect::NOTHING;
        let mut output = ctx.run_ui(input, |ui| {
            let mut actions = Vec::new();
            let ctx = ui.ctx().clone();
            screen = crate::hud_art::with(&ctx, state, |state, images| {
                canvas(ui, state, images, WIDTH, &mut actions)
            });
            nudge(ui, state, &mut actions);
            run(state, actions);
        });
        output.textures_delta.clear();
        screen
    }

    fn button(pos: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn key(key: Key, shift: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: if shift {
                egui::Modifiers::SHIFT
            } else {
                egui::Modifiers::NONE
            },
        }
    }

    fn setup() -> (tempfile::TempDir, AppState, egui::Context, Rect) {
        let (dir, mut state) = crate::state::testutil::state();
        let ctx = egui::Context::default();
        let screen = frame(&ctx, &mut state, vec![]);
        (dir, state, ctx, screen)
    }

    fn tile_center(state: &AppState, screen: Rect, id: ElementId) -> Pos2 {
        let rect = layout::preview(&state.profile.hud, [screen.width(), screen.height()])
            .into_iter()
            .find(|r| r.id == id)
            .unwrap()
            .rect;
        screen.min + vec2(rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0)
    }

    fn drag(ctx: &egui::Context, state: &mut AppState, from: Pos2, to: Pos2) {
        frame(
            ctx,
            state,
            vec![egui::Event::PointerMoved(from), button(from, true)],
        );
        let steps = 6;
        for i in 1..=steps {
            let pos = from + (to - from) * (i as f32 / steps as f32);
            frame(ctx, state, vec![egui::Event::PointerMoved(pos)]);
        }
        frame(ctx, state, vec![button(to, false)]);
        frame(ctx, state, vec![]);
    }

    #[test]
    fn click_selects_and_background_click_clears() {
        let (_dir, mut state, ctx, screen) = setup();
        let minimap = tile_center(&state, screen, ElementId::Minimap);
        frame(
            &ctx,
            &mut state,
            vec![egui::Event::PointerMoved(minimap), button(minimap, true)],
        );
        frame(&ctx, &mut state, vec![button(minimap, false)]);
        assert_eq!(state.ui.hud_selected, Some(ElementId::Minimap));
        let empty = screen.min + vec2(screen.width() * 0.75, screen.height() * 0.3);
        frame(
            &ctx,
            &mut state,
            vec![egui::Event::PointerMoved(empty), button(empty, true)],
        );
        frame(&ctx, &mut state, vec![button(empty, false)]);
        assert_eq!(state.ui.hud_selected, None);
        assert!(!state.is_dirty(), "clicks alone change nothing");
    }

    #[test]
    fn dragging_moves_in_reference_pixels_and_snaps_to_the_edge() {
        let (_dir, mut state, ctx, screen) = setup();
        let k = screen.height() / 1080.0;
        // 40px keeps every edge and the centre well clear of the snap lines.
        let start = tile_center(&state, screen, ElementId::Chat);
        let to = start + vec2(-40.0, 0.0);
        drag(&ctx, &mut state, start, to);
        assert_eq!(state.ui.hud_selected, Some(ElementId::Chat));
        let edit = state.hud_edit(ElementId::Chat);
        let expected = (-40.0 / k).round() as i32;
        assert!(
            (edit.offset_x - expected).abs() <= 2,
            "offset {} vs {expected}",
            edit.offset_x
        );
        assert_eq!(edit.offset_y, 0);
        assert!(state.is_dirty());

        // Dragging the minimap 4px short of the right edge lands exactly on it.
        let vanilla = elements::spec(ElementId::Minimap).vanilla;
        let minimap = tile_center(&state, screen, ElementId::Minimap);
        let gap = -vanilla.dx * k;
        drag(&ctx, &mut state, minimap, minimap + vec2(gap - 4.0, 0.0));
        let edit = state.hud_edit(ElementId::Minimap);
        assert_eq!(
            edit.offset_x, -vanilla.dx as i32,
            "snapped to the frame edge"
        );
    }

    #[test]
    fn arrow_keys_nudge_the_selection() {
        let (_dir, mut state, ctx, _screen) = setup();
        state.ui.hud_selected = Some(ElementId::TopBar);
        frame(&ctx, &mut state, vec![key(Key::ArrowDown, false)]);
        frame(&ctx, &mut state, vec![key(Key::ArrowRight, true)]);
        frame(&ctx, &mut state, vec![key(Key::ArrowUp, false)]);
        let edit = state.hud_edit(ElementId::TopBar);
        assert_eq!((edit.offset_x, edit.offset_y), (10, 0));
    }

    #[test]
    fn scrolling_over_a_tile_resizes_it_within_the_slider_range() {
        let (_dir, mut state, ctx, screen) = setup();
        let minimap = tile_center(&state, screen, ElementId::Minimap);
        frame(&ctx, &mut state, vec![egui::Event::PointerMoved(minimap)]);
        let wheel = |delta: f32| egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, delta),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        };
        frame(&ctx, &mut state, vec![wheel(40.0)]);
        let up = state.hud_edit(ElementId::Minimap).scale_pct;
        assert!(up > 100 && up <= 110, "one notch up: {up}");
        for _ in 0..40 {
            frame(&ctx, &mut state, vec![wheel(-80.0)]);
        }
        assert_eq!(
            state.hud_edit(ElementId::Minimap).scale_pct,
            *SIZE_RANGE.start()
        );
    }

    #[test]
    fn corner_handle_scales_the_selected_tile() {
        let (_dir, mut state, ctx, screen) = setup();
        state.ui.hud_selected = Some(ElementId::Minimap);
        frame(&ctx, &mut state, vec![]);
        let rect = layout::preview(&state.profile.hud, [screen.width(), screen.height()])
            .into_iter()
            .find(|r| r.id == ElementId::Minimap)
            .unwrap()
            .rect;
        let corner = screen.min + vec2(rect[0], rect[1]);
        let anchor = screen.min + vec2(rect[0] + rect[2], rect[1] + rect[3]);
        let half = anchor + (corner - anchor) * 0.5;
        drag(&ctx, &mut state, corner, half);
        let edit = state.hud_edit(ElementId::Minimap);
        assert!(
            (48..=52).contains(&edit.scale_pct),
            "half the reach: {}",
            edit.scale_pct
        );
        assert_eq!(
            (edit.offset_x, edit.offset_y),
            (0, 0),
            "a handle never moves"
        );
    }

    #[test]
    fn position_text_is_friendly() {
        assert_eq!(
            position_text(&ElementEdit::default()),
            "Where the game puts it"
        );
        let e = ElementEdit {
            offset_x: -24,
            offset_y: 10,
            ..ElementEdit::default()
        };
        assert_eq!(position_text(&e), "Moved 24px left, 10px down");
    }
}
