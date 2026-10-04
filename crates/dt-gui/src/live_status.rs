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

fn dot(ui: &mut Ui, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(10.0, 16.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
}

/// Launch Deadlock when it is closed; while it runs, the status plus Restart once file changes
/// wait for one. `wide` fills the available width (the sidebar); otherwise it is a header chip.
pub fn launch_control(ui: &mut Ui, state: &mut AppState, wide: bool) {
    let height = if wide { 34.0 } else { 24.0 };
    let width = if wide { ui.available_width() } else { 0.0 };
    if let Some(elapsed) = state.relaunch.elapsed(Instant::now()) {
        ui.horizontal(|ui| {
            dot(ui, WARN);
            ui.spinner();
            ui.label(
                RichText::new(format!("Restarting Deadlock ({}s)", elapsed.as_secs()))
                    .small()
                    .color(WEAK),
            );
        });
        ui.ctx().request_repaint_after(Duration::from_millis(500));
        return;
    }
    if state.ctx.game_running {
        ui.horizontal(|ui| {
            dot(ui, GOOD);
            ui.label(RichText::new("Deadlock is running").small().color(WEAK));
            if let Some(pending) = &state.pending_restart {
                let tip = format!(
                    "Close Deadlock and start it again through Steam to load: {}",
                    pending.names.join(", ")
                );
                let restart = egui::Button::new(RichText::new("Restart").size(12.0).color(TEXT))
                    .min_size(vec2(0.0, height - 8.0));
                if ui.add(restart).on_hover_text(tip).clicked() {
                    views::run_apply_relaunch(ui.ctx(), state);
                }
            }
        });
        return;
    }
    ui.horizontal(|ui| {
        if !wide {
            dot(ui, WEAK);
        }
        let launch = egui::Button::new(
            RichText::new("Launch Deadlock")
                .size(if wide { 14.0 } else { 12.0 })
                .strong()
                .color(ON_ACCENT),
        )
        .fill(ACCENT)
        .min_size(vec2(width, height));
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
