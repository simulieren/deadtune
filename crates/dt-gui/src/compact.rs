//! The mini window: a narrow always-on-top strip for tweaking while the game runs borderless
//! windowed. One design for both views; the body follows the view it was opened from.

use dt_core::catalog::Kind;
use dt_core::preset::{self, PresetId};
use dt_core::profile::BaseRef;
use eframe::egui::{
    self, Align, CornerRadius, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, ViewportCommand,
    WindowLevel, vec2,
};

use crate::friendly::{self, human_error};
use crate::live::BridgeKind;
use crate::live_status;
use crate::settings::View;
use crate::state::{AppState, Mode, Pending, Status, Timing};
use crate::theme::{self, ACCENT, BAD, BORDER, CARD_HOVER, ON_ACCENT, RAIL, TEXT, WARN, WEAK};
use crate::{app, simple, views};

pub const SIZE: [f32; 2] = [340.0, 560.0];
/// From this many rows on, a filter box sits above the list.
const FILTER_FROM: usize = 7;

/// What the list holds, decided by the view the window was opened from. Expand goes back to
/// that same view, so `Settings::view` is the only record of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Body {
    /// The simple view's key settings, then its pins.
    Quick,
    /// The advanced view's starred convars.
    Favourites,
}

impl Body {
    pub fn of(view: View) -> Body {
        match view {
            View::Simple => Body::Quick,
            View::Advanced => Body::Favourites,
        }
    }
}

pub fn enter(ctx: &egui::Context, state: &mut AppState) {
    state.ui.mode = Mode::Compact;
    window(ctx, WindowLevel::AlwaysOnTop, SIZE);
}

pub fn expand(ctx: &egui::Context, state: &mut AppState) {
    state.ui.mode = Mode::Full;
    window(ctx, WindowLevel::Normal, app::FULL_SIZE);
}

fn window(ctx: &egui::Context, level: WindowLevel, size: [f32; 2]) {
    ctx.send_viewport_cmd(ViewportCommand::WindowLevel(level));
    ctx.send_viewport_cmd(ViewportCommand::InnerSize(size.into()));
}

#[derive(Debug, PartialEq, Eq)]
pub struct Group {
    pub title: &'static str,
    pub names: Vec<String>,
}

/// The rows the body lists, before the filter box.
pub fn groups(state: &AppState) -> Vec<Group> {
    let strings = |names: &[&str]| names.iter().map(|n| n.to_string()).collect();
    match Body::of(state.settings.view) {
        Body::Quick => {
            let quick = friendly::KEY_SETTINGS;
            let pinned: Vec<String> = state
                .settings
                .pinned
                .iter()
                .filter(|n| !quick.contains(&n.as_str()))
                .cloned()
                .collect();
            let mut out = vec![Group {
                title: "Quick",
                names: strings(quick),
            }];
            if !pinned.is_empty() {
                out.push(Group {
                    title: "Pinned",
                    names: pinned,
                });
            }
            out
        }
        Body::Favourites => vec![Group {
            title: "Favourites",
            names: state.settings.favourites.iter().cloned().collect(),
        }],
    }
}

/// Pins the key settings into whichever list the body shows.
pub fn use_recommended(state: &mut AppState) {
    let list = match Body::of(state.settings.view) {
        Body::Quick => &mut state.settings.pinned,
        Body::Favourites => &mut state.settings.favourites,
    };
    list.extend(friendly::KEY_SETTINGS.iter().map(|n| n.to_string()));
}

/// Live-class changes the running game would pick up now.
pub fn instant_count(state: &AppState) -> usize {
    if !state.ctx.game_running {
        return 0;
    }
    state.preview.as_ref().map_or(0, |p| p.live.len())
}

/// The footer's one-line summary: "3 changes · 1 instant".
pub fn headline(pending: &Pending, instant: usize) -> String {
    let changes = |n: usize| match n {
        1 => "1 change".to_string(),
        n => format!("{n} changes"),
    };
    let mut text = match pending {
        Pending::Nothing => return "Nothing to apply".into(),
        Pending::Preset { label, tweaks: 0 } => format!("Switch to {label}"),
        Pending::Preset { label, tweaks } => format!("{label} + {}", changes(*tweaks)),
        Pending::Tweaks(n) => changes(*n),
        Pending::Other => "HUD, addon or practice changes".into(),
    };
    if instant > 0 {
        text.push_str(&format!(" · {instant} instant"));
    }
    text
}

fn matches(name: &str, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    let label = friendly::row(name).map_or("", |r| r.label).to_lowercase();
    name.to_lowercase().contains(&query) || label.contains(&query)
}

enum Edit {
    Set(String, String),
    Reset(String),
    Unlist(String),
    Base(PresetId),
    Recommended,
    Expand,
}

pub fn ui(ui: &mut Ui, state: &mut AppState) {
    let body = Body::of(state.settings.view);
    let mut edits = Vec::new();
    egui::Panel::top("mini_header")
        .frame(
            egui::Frame::new()
                .fill(RAIL)
                .inner_margin(Margin::symmetric(12, 8)),
        )
        .show(ui, |ui| header(ui, state, &mut edits));
    egui::Panel::bottom("mini_footer")
        .frame(
            egui::Frame::new()
                .fill(RAIL)
                .inner_margin(Margin::symmetric(12, 10)),
        )
        .show(ui, |ui| footer(ui, state));
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::BG).inner_margin(Margin {
            left: 12,
            right: 12,
            top: 10,
            bottom: 0,
        }))
        .show(ui, |ui| list(ui, state, body, &mut edits));
    for edit in edits {
        match edit {
            Edit::Set(name, value) => {
                if let Err(e) = state.set_convar(&name, value) {
                    state.status = Some(Status::Error(e.to_string()));
                }
            }
            Edit::Reset(name) => state.reset_convars([name.as_str()]),
            Edit::Unlist(name) => match body {
                Body::Quick => state.toggle_pin(&name),
                Body::Favourites => state.toggle_favourite(&name),
            },
            Edit::Base(id) => state.set_base(BaseRef::Preset(id)),
            Edit::Recommended => use_recommended(state),
            Edit::Expand => expand(ui.ctx(), state),
        }
    }
}

fn header(ui: &mut Ui, state: &mut AppState, edits: &mut Vec<Edit>) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        crate::icons::brand_mark(ui, 24.0, crate::theme::RAIL).on_hover_text("DeadTune");
        ui.add_space(10.0);
        preset_chip(ui, state, edits);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if expand_button(ui).clicked() {
                edits.push(Edit::Expand);
            }
            ui.add_space(8.0);
            ui.spacing_mut().item_spacing.x = 6.0;
            live_status::launch_control(ui, state, live_status::Fit::Dot);
        });
    });
}

/// The current goal (or preset) as a small chip; click for the goals and the other presets.
fn preset_chip(ui: &mut Ui, state: &AppState, edits: &mut Vec<Edit>) {
    let base = match &state.profile.base {
        BaseRef::Preset(id) => Some(*id),
        BaseRef::File(_) => None,
    };
    let goal = base.and_then(|id| friendly::GOALS.iter().find(|(g, _, _)| *g == id));
    let text = match goal {
        Some((_, title, _)) => title.to_string(),
        None => state.preset_label(),
    };
    let hover = match base {
        Some(id) => {
            let info = preset::info(id);
            format!("Preset: {} by {}", info.label, info.author)
        }
        None => "Your files as they were before DeadTune.".into(),
    };
    let button = egui::Button::new(RichText::new(text).size(12.0).color(TEXT))
        .fill(CARD_HOVER)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(255))
        .min_size(vec2(0.0, 22.0));
    let (response, _) = egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| {
        ui.set_min_width(220.0);
        for (id, title, sub) in friendly::GOALS {
            let selected = base == Some(*id);
            if ui
                .selectable_label(selected, format!("{title}  {sub}"))
                .clicked()
            {
                edits.push(Edit::Base(*id));
                ui.close();
            }
        }
        ui.separator();
        for info in preset::all() {
            if friendly::GOALS.iter().any(|(g, _, _)| *g == info.id) {
                continue;
            }
            let Some(blurb) = friendly::preset_blurb(info.id) else {
                continue;
            };
            if ui
                .selectable_label(base == Some(info.id), info.label)
                .on_hover_text(blurb)
                .clicked()
            {
                edits.push(Edit::Base(info.id));
                ui.close();
            }
        }
    });
    response.on_hover_text(hover);
}

/// Two arrows pointing to opposite corners, then "Expand"; icon fonts have no such glyph.
fn expand_button(ui: &mut Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(68.0, 22.0), Sense::click());
    let color = if response.hovered() { TEXT } else { WEAK };
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(4), CARD_HOVER);
    }
    let icon = rect.left_center() + vec2(10.0, 0.0);
    let stroke = Stroke::new(1.5, color);
    painter.arrow(icon + vec2(-1.0, 1.0), vec2(-4.5, 4.5), stroke);
    painter.arrow(icon + vec2(1.0, -1.0), vec2(4.5, -4.5), stroke);
    painter.text(
        rect.left_center() + vec2(22.0, 0.0),
        egui::Align2::LEFT_CENTER,
        "Expand",
        egui::FontId::proportional(12.5),
        color,
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Back to the full window")
}

/// A thumbtack, amber when pinned; used by the simple view's rows and the mini window.
pub fn pin_button(ui: &mut Ui, pinned: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
    let color = if pinned {
        ACCENT
    } else if response.hovered() {
        TEXT
    } else {
        WEAK
    };
    let c = rect.center();
    let painter = ui.painter();
    painter.line_segment(
        [c + vec2(-5.0, -2.0), c + vec2(2.0, 5.0)],
        Stroke::new(2.5, color),
    );
    painter.line_segment(
        [c + vec2(-1.5, 1.5), c + vec2(-6.0, 6.0)],
        Stroke::new(1.2, color),
    );
    painter.circle_filled(c + vec2(2.5, -2.5), 3.5, color);
    if !pinned {
        painter.circle_filled(c + vec2(2.5, -2.5), 2.0, ui.visuals().panel_fill);
    }
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(if pinned {
            "Unpin from the mini window"
        } else {
            "Pin to the mini window"
        })
}

fn list(ui: &mut Ui, state: &mut AppState, body: Body, edits: &mut Vec<Edit>) {
    let groups = groups(state);
    let total: usize = groups.iter().map(|g| g.names.len()).sum();
    if total == 0 {
        empty(ui, body, edits);
        return;
    }
    if total >= FILTER_FROM {
        simple::search_box(ui, &mut state.ui.mini_filter);
        ui.add_space(8.0);
    }
    let query = state.ui.mini_filter.clone();
    let mut shown = 0;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            for group in &groups {
                let names: Vec<&String> =
                    group.names.iter().filter(|n| matches(n, &query)).collect();
                if names.is_empty() {
                    continue;
                }
                shown += names.len();
                simple::caption(ui, group.title);
                ui.add_space(2.0);
                for (i, name) in names.iter().enumerate() {
                    if i > 0 {
                        let y = ui.cursor().top() + 1.0;
                        ui.painter().hline(
                            ui.max_rect().x_range(),
                            y,
                            Stroke::new(1.0, BORDER.gamma_multiply(0.8)),
                        );
                        ui.add_space(6.0);
                    }
                    row(ui, state, name, body, group.title == "Quick", edits);
                    ui.add_space(4.0);
                }
                ui.add_space(10.0);
            }
            if shown == 0 {
                ui.label(RichText::new("Nothing matches.").color(WEAK));
            }
            ui.add_space(8.0);
        });
}

fn empty(ui: &mut Ui, body: Body, edits: &mut Vec<Edit>) {
    let hint = match body {
        Body::Quick => {
            "Pin settings with the pin button next to each row in the main window; they show up here."
        }
        Body::Favourites => "Star settings in the main window's ConVars tab; they show up here.",
    };
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(
            RichText::new("Nothing pinned yet")
                .size(15.0)
                .strong()
                .family(theme::semibold()),
        );
        ui.add_space(2.0);
        ui.label(RichText::new(hint).color(WEAK));
        ui.add_space(8.0);
        if ui
            .add(
                egui::Button::new(
                    RichText::new("Use the recommended quick set")
                        .strong()
                        .color(ON_ACCENT),
                )
                .fill(ACCENT)
                .min_size(vec2(0.0, 30.0)),
            )
            .on_hover_text("The five settings with the biggest FPS impact")
            .clicked()
        {
            edits.push(Edit::Recommended);
        }
    });
}

/// Label line (name, Reset, unlist) over a full-width control.
fn row(ui: &mut Ui, state: &AppState, name: &str, body: Body, fixed: bool, edits: &mut Vec<Edit>) {
    let entry = state.catalog.get(name);
    let friendly = friendly::row(name);
    let denied = state.catalog.is_denied(name);
    let value = state.current_value(name).unwrap_or_default();
    let preset = state.preset_value(name).unwrap_or_default();
    let changed = state.is_changed(name);
    let top = ui.cursor().top();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let label = match friendly {
            Some(r) => RichText::new(r.label).size(13.5).strong(),
            None => RichText::new(name).monospace().size(12.5),
        };
        ui.label(label.color(if changed { ACCENT } else { TEXT }))
            .on_hover_text(friendly.map_or(name, |r| r.help));
        if changed {
            let was = match friendly {
                Some(r) => friendly::display(r.control, &preset),
                None => preset.clone(),
            };
            if ui
                .add(
                    egui::Button::new(RichText::new("Reset").small().color(ACCENT))
                        .fill(ACCENT.gamma_multiply(0.14))
                        .corner_radius(CornerRadius::same(255))
                        .min_size(vec2(0.0, 18.0)),
                )
                .on_hover_text(format!("Back to your preset: {was}"))
                .clicked()
            {
                edits.push(Edit::Reset(name.to_string()));
            }
        }
        if !fixed {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let clicked = match body {
                    Body::Quick => pin_button(ui, true).clicked(),
                    Body::Favourites => ui
                        .add(egui::Button::new(RichText::new("★").color(ACCENT)).frame(false))
                        .on_hover_text("Unstar")
                        .clicked(),
                };
                if clicked {
                    edits.push(Edit::Unlist(name.to_string()));
                }
            });
        }
    });
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), 26.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.set_width(ui.available_width());
            ui.add_enabled_ui(!denied, |ui| {
                let edited = match (friendly, entry) {
                    (Some(r), Some(entry)) => {
                        simple::control(ui, r.control, entry, &value, &preset)
                    }
                    _ => {
                        let edited = views::control(ui, name, entry, Some(&value));
                        // A bare checkbox says nothing; the raw text does.
                        if entry.is_some_and(|e| matches!(e.kind, Kind::Bool)) {
                            let text = if value.is_empty() {
                                "game default"
                            } else {
                                &value
                            };
                            ui.label(RichText::new(text).monospace().small().color(WEAK));
                        }
                        edited
                    }
                };
                if let Some(v) = edited {
                    edits.push(Edit::Set(name.to_string(), v));
                }
            });
        },
    );
    if changed {
        let x = ui.max_rect().left() - 8.0;
        ui.painter().rect_filled(
            Rect::from_x_y_ranges(x..=x + 3.0, top..=ui.cursor().top() - 2.0),
            CornerRadius::same(2),
            ACCENT,
        );
    }
}

/// The button that sends pending live changes now, the same in every view: its label and
/// tooltip for the bridge in use, and whether there is anything to send.
pub(crate) fn send_now(state: &AppState) -> (String, String, bool) {
    let key = &state.settings.bind_key;
    let (label, hint) = match state.settings.bridge {
        BridgeKind::ExecFile => (
            format!("Send now ({key})"),
            format!(
                "Writes the instant changes to a file the game loads when you press {key} in Deadlock. Set up once under Safety & setup."
            ),
        ),
        BridgeKind::Netcon => (
            "Send now".to_string(),
            "Sends the instant changes to the game's console right away.".to_string(),
        ),
        BridgeKind::Clipboard => (
            "Copy commands".to_string(),
            "Copies the console commands; paste them into the Deadlock console (F7).".to_string(),
        ),
    };
    let can_send =
        state.ctx.game_running && (instant_count(state) > 0 || state.live_push.is_pending());
    (label, hint, can_send)
}

fn footer(ui: &mut Ui, state: &mut AppState) {
    let pending = state.pending();
    let ready = pending != Pending::Nothing;
    let instant = instant_count(state);
    let key = state.settings.bind_key.clone();
    let title = RichText::new(headline(&pending, instant))
        .size(14.0)
        .strong();
    ui.label(if ready {
        title.color(ACCENT)
    } else {
        title.color(WEAK)
    });
    let when = match state.timing() {
        Timing::Nothing => match &state.pending_restart {
            Some(p) => format!("Restart Deadlock to load {} saved changes.", p.names.len()),
            None => "Change a setting above, then Apply.".into(),
        },
        Timing::NextLaunch => "Takes effect next time you start Deadlock.".into(),
        Timing::Instant => match state.settings.bridge {
            BridgeKind::ExecFile => format!("Right away: press {key} in game."),
            _ => "Right away.".into(),
        },
        Timing::Mixed { now, later } => {
            format!("{now} right away, {later} next time you start Deadlock.")
        }
    };
    ui.label(RichText::new(when).small().color(WEAK));
    live_status::push_status(ui, state, true);
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let apply = egui::Button::new(RichText::new("Apply").strong().color(ON_ACCENT))
                .fill(ACCENT)
                .min_size(vec2(96.0, 30.0));
            if ui.add_enabled(ready, apply).clicked() {
                simple::apply(ui.ctx(), state);
            }
            let (label, hint, can_send) = send_now(state);
            let send = egui::Button::new(label).min_size(vec2(0.0, 30.0));
            let response = ui.add_enabled(can_send, send);
            let response = if state.ctx.game_running {
                response.on_hover_text(hint)
            } else {
                response.on_disabled_hover_text("Deadlock is closed; Apply saves for next launch.")
            };
            if response.clicked() {
                let result = state.push_now();
                app::report_push(ui.ctx(), state, result);
            }
        });
    });
    match &state.status {
        Some(Status::Info(m)) => {
            ui.label(RichText::new(m).small());
        }
        Some(Status::Warn(raw)) => {
            ui.label(RichText::new(human_error(raw)).small().color(WARN))
                .on_hover_text(raw);
        }
        Some(Status::Error(raw)) => {
            ui.label(RichText::new(human_error(raw)).small().color(BAD))
                .on_hover_text(raw);
        }
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::testutil::state;
    use dt_core::profile::BaseRef;

    const LIVE: &str = "fps_max";
    const RESTART: &str = "ai_foot_sweep_enable";

    fn listed(state: &AppState) -> Vec<String> {
        groups(state).into_iter().flat_map(|g| g.names).collect()
    }

    #[test]
    fn body_follows_the_view_and_expand_returns_to_it() {
        let ctx = egui::Context::default();
        let (_dir, mut state) = state();
        for view in [View::Simple, View::Advanced] {
            state.settings.view = view;
            enter(&ctx, &mut state);
            assert_eq!(state.ui.mode, Mode::Compact);
            assert_eq!(Body::of(state.settings.view), Body::of(view));
            expand(&ctx, &mut state);
            assert_eq!(state.ui.mode, Mode::Full);
            assert_eq!(
                state.settings.view, view,
                "expand lands on the view it came from"
            );
        }
    }

    #[test]
    fn quick_body_is_key_settings_then_pins_without_repeats() {
        let (_dir, mut state) = state();
        state.settings.view = View::Simple;
        assert_eq!(groups(&state).len(), 1);
        assert_eq!(listed(&state), friendly::KEY_SETTINGS);
        state.toggle_pin("r_shadows");
        state.toggle_pin(LIVE);
        let pinned = &groups(&state)[1];
        assert_eq!(pinned.title, "Pinned");
        assert_eq!(
            pinned.names,
            vec!["r_shadows"],
            "a pinned key setting is not repeated"
        );
        state.toggle_pin("r_shadows");
        assert_eq!(groups(&state).len(), 1, "unpinned");
    }

    #[test]
    fn favourites_body_is_empty_until_starred_or_recommended() {
        let (_dir, mut state) = state();
        state.settings.view = View::Advanced;
        assert!(listed(&state).is_empty());
        state.toggle_favourite("r_farz");
        assert_eq!(listed(&state), vec!["r_farz"]);
        use_recommended(&mut state);
        let names = listed(&state);
        for key in friendly::KEY_SETTINGS {
            assert!(names.iter().any(|n| n == key), "{key}");
        }
        assert!(names.iter().any(|n| n == "r_farz"), "stars kept");
        assert!(
            state.settings.pinned.is_empty(),
            "advanced recommends into favourites"
        );
        state.settings.view = View::Simple;
        use_recommended(&mut state);
        assert_eq!(state.settings.pinned.len(), friendly::KEY_SETTINGS.len());
    }

    #[test]
    fn headline_counts_changes_and_instant_ones() {
        let (_dir, mut state) = state();
        assert_eq!(
            headline(&state.pending(), instant_count(&state)),
            "Nothing to apply"
        );
        state.set_convar(LIVE, "144".into()).unwrap();
        state.set_convar(RESTART, "true".into()).unwrap();
        assert_eq!(
            headline(&state.pending(), instant_count(&state)),
            "2 changes"
        );
        state.observe_game(true, None);
        assert_eq!(
            headline(&state.pending(), instant_count(&state)),
            "2 changes · 1 instant"
        );
        state.set_base(BaseRef::Preset(PresetId::OptilockPotato));
        assert_eq!(headline(&state.pending(), 0), "OptiLock potato + 2 changes");
        assert_eq!(
            headline(
                &Pending::Preset {
                    label: "Balanced".into(),
                    tweaks: 0
                },
                0
            ),
            "Switch to Balanced"
        );
        assert_eq!(headline(&Pending::Tweaks(1), 1), "1 change · 1 instant");
        assert_eq!(
            headline(&Pending::Other, 0),
            "HUD, addon or practice changes"
        );
    }

    #[test]
    fn filter_matches_label_or_convar_name() {
        assert!(matches("fps_max", ""));
        assert!(matches("fps_max", "FPS lim"));
        assert!(matches("fps_max", "fps_"));
        assert!(!matches("fps_max", "shadow"));
        assert!(
            matches("ai_foot_sweep_enable", "sweep"),
            "no friendly row, name still matches"
        );
    }
}
