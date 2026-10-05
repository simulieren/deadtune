//! The two widgets every surface shares for the live path: the Launch / running / Restart
//! control, and the push status line driven by the ack tracker.

use std::time::{Duration, Instant, SystemTime};

use dt_core::bridge::ack::{Outcome, PushStatus};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Rect, Response, RichText, Sense, Stroke, Ui, Vec2,
    vec2,
};

use crate::icons::{self, Icon};
use crate::state::{AppState, Status};
use crate::theme::{
    self, ACCENT, BAD, BORDER, CARD, CARD_HOVER, GOOD, ON_ACCENT, TEXT, WARN, WEAK,
};
use crate::views;

pub fn clock(at: SystemTime) -> String {
    chrono::DateTime::<chrono::Local>::from(at)
        .format("%H:%M")
        .to_string()
}

fn dot(ui: &mut Ui, color: egui::Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(10.0, 16.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
    response
}

/// How much room the launch control has.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// The sidebar: a full-width button, a labelled dot while running.
    Wide,
    /// A header: a small button, a labelled dot while running.
    Chip,
    /// The mini window header: a small button, a bare dot while running.
    Dot,
}

/// Launch Deadlock when it is closed; while it runs, the status plus Restart once file changes
/// wait for one. In a right-to-left header the parts are added in reverse so they read the
/// same way everywhere.
pub fn launch_control(ui: &mut Ui, state: &mut AppState, fit: Fit) {
    let rtl = ui.layout().prefer_right_to_left();
    if let Some(elapsed) = state.relaunch.elapsed(Instant::now()) {
        let text = format!("Restarting Deadlock ({}s)", elapsed.as_secs());
        let label = |ui: &mut Ui| {
            ui.spinner();
            if fit != Fit::Dot {
                ui.label(RichText::new(&text).small().color(WEAK));
            }
        };
        if rtl {
            label(ui);
            dot(ui, WARN).on_hover_text(&text);
        } else {
            dot(ui, WARN).on_hover_text(&text);
            label(ui);
        }
        ui.ctx().request_repaint_after(Duration::from_millis(500));
        return;
    }
    if state.ctx.game_running {
        let restart = |ui: &mut Ui, state: &mut AppState| {
            let Some(pending) = &state.pending_restart else {
                return;
            };
            let tip = format!(
                "Close Deadlock and start it again through Steam to load: {}",
                pending.names.join(", ")
            );
            let button = egui::Button::new(RichText::new("Restart").size(12.0).color(TEXT))
                .min_size(vec2(0.0, 22.0));
            if ui.add(button).on_hover_text(tip).clicked() {
                views::run_apply_relaunch(ui.ctx(), state);
            }
        };
        let label = |ui: &mut Ui| {
            if fit != Fit::Dot {
                ui.label(RichText::new("Deadlock is running").small().color(WEAK));
            }
        };
        if rtl {
            restart(ui, state);
            label(ui);
            dot(ui, GOOD).on_hover_text("Deadlock is running");
        } else {
            dot(ui, GOOD).on_hover_text("Deadlock is running");
            label(ui);
            restart(ui, state);
        }
        return;
    }
    let safe = state.settings.safe_mode;
    let menu_width = 26.0;
    let wide = ui.available_width() - menu_width - ui.spacing().item_spacing.x;
    let (text, size, min) = match (fit, safe) {
        (Fit::Wide, false) => ("Launch Deadlock", 14.0, vec2(wide, 34.0)),
        (Fit::Wide, true) => ("Launch (safe mode)", 14.0, vec2(wide, 34.0)),
        (Fit::Chip, false) => ("Launch Deadlock", 12.0, vec2(0.0, 24.0)),
        (Fit::Chip, true) => ("Launch (safe mode)", 12.0, vec2(0.0, 24.0)),
        (Fit::Dot, _) => ("Launch", 12.0, vec2(0.0, 22.0)),
    };
    let fill = if safe { WARN } else { ACCENT };
    let hover = if safe {
        "Safe mode: every DeadTune pak is out of the game folder. Starts Deadlock through Steam \
         with DeadTune's boot cfg."
            .to_string()
    } else {
        format!(
            "Starts Deadlock through Steam with DeadTune's boot cfg: {}",
            state.launch_args().args.join(" ")
        )
    };
    let parts = |ui: &mut Ui, state: &mut AppState| {
        let response = if fit == Fit::Wide {
            launch_button(ui, text, fill, min)
        } else {
            ui.add(
                egui::Button::new(RichText::new(text).size(size).strong().color(ON_ACCENT))
                    .fill(fill)
                    .min_size(min),
            )
        };
        if response.on_hover_text(&hover).clicked()
            && let Err(e) = state.launch_game()
        {
            state.status = Some(Status::Error(format!("launch: {e}")));
        }
        launch_menu(ui, state, fit);
    };
    if rtl {
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            parts(ui, state);
        });
    } else {
        parts(ui, state);
    }
}

/// The sidebar's Launch: a full-width accent button with a play icon.
fn launch_button(ui: &mut Ui, text: &str, fill: Color32, size: Vec2) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let t = ui
        .ctx()
        .animate_bool_responsive(response.id.with("hover"), response.hovered());
    let fill = fill.lerp_to_gamma(Color32::WHITE, 0.12 * t);
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(8), fill);
    let galley = painter.layout_no_wrap(
        text.to_string(),
        FontId::new(14.0, theme::semibold()),
        ON_ACCENT,
    );
    let content = 12.0 + 8.0 + galley.size().x;
    let left = rect.center().x - content / 2.0;
    icons::paint(
        painter,
        Rect::from_center_size(egui::pos2(left + 6.0, rect.center().y), vec2(12.0, 12.0)),
        Icon::Play,
        ON_ACCENT,
    );
    painter.galley(
        egui::pos2(left + 20.0, rect.center().y - galley.size().y / 2.0),
        galley,
        ON_ACCENT,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The small menu beside Launch: safe mode on (removes every DeadTune pak now, keeps the
/// profile) and off (puts them back through the normal Apply).
fn launch_menu(ui: &mut Ui, state: &mut AppState, fit: Fit) {
    let safe = state.settings.safe_mode;
    let height = match fit {
        Fit::Wide => 34.0,
        Fit::Chip => 24.0,
        Fit::Dot => 22.0,
    };
    let (rect, response) = ui.allocate_exact_size(vec2(26.0, height), Sense::click());
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Launch options and safe mode");
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&response));
    let lit = response.hovered() || open;
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(if fit == Fit::Wide { 8 } else { 4 }),
        if lit { CARD_HOVER } else { CARD },
        Stroke::new(1.0, BORDER),
        egui::StrokeKind::Inside,
    );
    icons::paint(
        painter,
        Rect::from_center_size(rect.center(), vec2(14.0, 14.0)),
        Icon::ChevronDown,
        if lit { TEXT } else { WEAK },
    );
    egui::Popup::menu(&response).show(|ui| {
        if ui
            .button("Launch options")
            .on_hover_text("Renderer, intro video, extra options, and the text to paste into Steam")
            .clicked()
        {
            ui.close();
            state.ui.launch_options_open = true;
        }
        if safe {
            if ui
                .button("Restore addons")
                .on_hover_text("Puts every addon the profile has on back into the game folder.")
                .clicked()
            {
                ui.close();
                state.status = Some(match state.toggle_safe_mode() {
                    Ok(_) => Status::Info(
                        "Addons restored. They load the next time Deadlock starts.".into(),
                    ),
                    Err(e) => Status::Error(e),
                });
            }
        } else if ui
            .button("Safe mode: launch without addons")
            .on_hover_text(
                "Removes every DeadTune pak from game/citadel/addons right now (other mods stay), \
                 remembers what was on, and starts Deadlock. Restore addons from this menu later.",
            )
            .clicked()
        {
            ui.close();
            state.status = Some(match state.toggle_safe_mode() {
                Ok(_) => match state.launch_game() {
                    Ok(()) => Status::Info(
                        "Safe mode: DeadTune's addons are out of the game folder until you restore them."
                            .into(),
                    ),
                    Err(e) => Status::Error(format!("launch: {e}")),
                },
                Err(e) => Status::Error(e),
            });
        }
    });
}

fn results_tip(results: &[(String, Outcome)], transcript: &[String]) -> String {
    let mut tip: Vec<String> = results
        .iter()
        .map(|(name, outcome)| match outcome {
            Outcome::Applied(v) => format!("{name} = {v}"),
            Outcome::Rejected(why) => format!("{name}: {why}"),
            Outcome::NoEcho => format!("{name}: no reply in the console log"),
        })
        .collect();
    if !transcript.is_empty() {
        tip.push(String::new());
        tip.push("Console log:".into());
        tip.extend(transcript.iter().cloned());
    }
    tip.join("\n")
}

const CHECKLIST: [&str; 4] = [
    "Is Deadlock running?",
    "Was it started from DeadTune, or with +exec deadtune_boot in Steam's launch options?",
    "Press the key with the console closed.",
    "Check the bind: the Safety & setup page has it.",
];

/// The live-status line. Returns false when there is nothing to show (no push yet).
/// `dense` keeps the timeout checklist in a tooltip instead of lines below.
pub fn push_status(ui: &mut Ui, state: &AppState, dense: bool) -> bool {
    let key = &state.settings.bind_key;
    match state.ack.status() {
        PushStatus::Idle => false,
        PushStatus::Waiting { since, .. } => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new(format!(
                        "Waiting for Deadlock: press {key} in game ({}s)",
                        since.elapsed().as_secs()
                    ))
                    .small()
                    .color(ACCENT),
                );
            });
            ui.ctx().request_repaint_after(Duration::from_millis(250));
            true
        }
        PushStatus::Confirmed { at, results } => {
            let applied = state.ack.status().applied();
            let rejected: Vec<&str> = results
                .iter()
                .filter(|(_, o)| !matches!(o, Outcome::Applied(_)))
                .map(|(n, _)| n.as_str())
                .collect();
            let (color, text) = if rejected.is_empty() {
                (
                    GOOD,
                    format!(
                        "\u{2713} Deadlock applied {applied} of {} at {}",
                        results.len(),
                        clock(*at)
                    ),
                )
            } else {
                (
                    WARN,
                    format!(
                        "{applied} applied, {} need{} a restart ({})",
                        rejected.len(),
                        if rejected.len() == 1 { "s" } else { "" },
                        rejected.join(", ")
                    ),
                )
            };
            ui.label(RichText::new(text).small().color(color))
                .on_hover_text(
                    RichText::new(results_tip(results, &state.ack.transcript))
                        .monospace()
                        .size(11.0),
                );
            true
        }
        PushStatus::TimedOut { after, .. } => {
            let text = format!("No reply from Deadlock after {} s", after.as_secs());
            let checklist = CHECKLIST.join("\n");
            if dense {
                ui.label(RichText::new(text).small().color(BAD))
                    .on_hover_text(checklist);
            } else {
                ui.label(RichText::new(text).small().color(BAD));
                for item in CHECKLIST {
                    ui.label(
                        RichText::new(format!("\u{2022} {item}"))
                            .small()
                            .color(WEAK),
                    );
                }
            }
            true
        }
    }
}

/// Numbered step marker that turns into a green tick when done.
pub fn step_mark(ui: &mut Ui, number: usize, done: bool) {
    let (rect, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
    let painter = ui.painter();
    let (fill, text, color) = if done {
        (GOOD.gamma_multiply(0.2), "\u{2713}".to_string(), GOOD)
    } else {
        (WEAK.gamma_multiply(0.18), number.to_string(), WEAK)
    };
    painter.circle_filled(rect.center(), 11.0, fill);
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(12.5),
        color,
    );
}
