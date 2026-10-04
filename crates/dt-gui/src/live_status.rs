//! The two widgets every surface shares for the live path: the Launch / running / Restart
//! control, and the push status line driven by the ack tracker.

use std::time::{Duration, Instant, SystemTime};

use dt_core::bridge::ack::{Outcome, PushStatus};
use eframe::egui::{self, Align2, FontId, RichText, Sense, Ui, vec2};

use crate::state::{AppState, Status};
use crate::theme::{ACCENT, BAD, GOOD, ON_ACCENT, TEXT, WARN, WEAK};
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
    let (text, size, min) = match fit {
        Fit::Wide => ("Launch Deadlock", 14.0, vec2(ui.available_width(), 34.0)),
        Fit::Chip => ("Launch Deadlock", 12.0, vec2(0.0, 24.0)),
        Fit::Dot => ("Launch", 12.0, vec2(0.0, 22.0)),
    };
    let launch = egui::Button::new(RichText::new(text).size(size).strong().color(ON_ACCENT))
        .fill(ACCENT)
        .min_size(min);
    if ui
        .add(launch)
        .on_hover_text(format!(
            "Starts Deadlock through Steam with DeadTune's boot cfg: {}",
            state.launch_args().args.join(" ")
        ))
        .clicked()
        && let Err(e) = state.launch_game()
    {
        state.status = Some(Status::Error(format!("launch: {e}")));
    }
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
