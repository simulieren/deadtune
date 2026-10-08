//! The "Check live preview" button and the card it opens, on the HUD pages and in
//! System check. State and wording live in `live_check` and `dt_core::hud::live_check`.

use std::time::{Duration, Instant};

use dt_core::doctor::CheckStatus;
use dt_core::hud::live_check::Tone;
use eframe::egui::{self, RichText, Ui};

use crate::checks_view::status_glyph;
use crate::live_check::LiveCheck;
use crate::state::{AppState, Status};
use crate::theme::{self, ACCENT, BAD, GOOD, TEXT, WARN, WEAK};

pub fn button(ui: &mut Ui, state: &mut AppState) {
    let busy = state.live_check.active();
    if ui
        .add_enabled(
            !busy,
            egui::Button::new(RichText::new("Check live preview").small()).small(),
        )
        .on_hover_text(
            "Shows a bigger minimap in game for a few seconds. If you don't see it, DeadTune \
             finds out why and tells you what to do.",
        )
        .clicked()
    {
        state.start_live_check(Instant::now());
    }
}

/// The check's card, while it runs or has a result.
pub fn card(ui: &mut Ui, state: &mut AppState) {
    let now = Instant::now();
    if matches!(state.live_check, LiveCheck::Idle) {
        return;
    }
    if state.live_check.active() {
        ui.ctx().request_repaint_after(Duration::from_millis(200));
    }
    ui.add_space(6.0);
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        match state.live_check.clone() {
            LiveCheck::Idle => {}
            LiveCheck::Showing { .. } => showing(ui, state, now),
            LiveCheck::Asking { .. } => asking(ui, state),
            LiveCheck::Done(done) => result(ui, state, &done),
        }
    });
    ui.add_space(6.0);
}

fn title(ui: &mut Ui, text: &str, color: egui::Color32) {
    ui.label(
        RichText::new(text)
            .size(17.0)
            .family(theme::semibold())
            .color(color),
    );
}

fn showing(ui: &mut Ui, state: &mut AppState, now: Instant) {
    let flash = state.live_hud.flash_state();
    let Some((secs, shown)) = state.live_check.countdown(flash, now) else {
        return;
    };
    ui.horizontal(|ui| {
        ui.add(egui::Spinner::new().size(28.0).color(ACCENT));
        ui.add_space(6.0);
        ui.vertical(|ui| {
            if shown {
                title(ui, &format!("Look at your minimap now \u{b7} {secs}"), TEXT);
                ui.label(
                    RichText::new(
                        "It is half again as big and moved toward the centre for a few seconds.",
                    )
                    .color(WEAK),
                );
            } else {
                title(
                    ui,
                    &format!("Sending a bigger minimap to the game \u{b7} {secs}"),
                    TEXT,
                );
                ui.label(RichText::new("Look at your minimap in game.").color(WEAK));
            }
        });
    });
    ui.add_space(4.0);
    if ui.small_button("Cancel").clicked() {
        state.close_live_check();
    }
}

fn asking(ui: &mut Ui, state: &mut AppState) {
    title(ui, "Did the minimap get bigger for a few seconds?", TEXT);
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.button("Yes").clicked() {
            state.answer_live_check(true);
        }
        if ui.button("No").clicked() {
            state.answer_live_check(false);
        }
        if ui.small_button("Cancel").clicked() {
            state.close_live_check();
        }
    });
}

fn tone_status(tone: Tone) -> Option<CheckStatus> {
    match tone {
        Tone::Good => Some(CheckStatus::Pass),
        Tone::Warn => Some(CheckStatus::Warn),
        Tone::Bad => Some(CheckStatus::Fail),
        Tone::Skip => None,
    }
}

fn result(ui: &mut Ui, state: &mut AppState, done: &crate::live_check::Done) {
    let d = &done.diagnosis;
    let (status, headline) = if d.works() {
        (CheckStatus::Pass, "Live preview works".to_string())
    } else if d.rows.iter().any(|r| r.tone == Tone::Bad) {
        (
            CheckStatus::Fail,
            "Live preview isn't working yet".to_string(),
        )
    } else {
        (
            CheckStatus::Warn,
            "Live preview needs one more step".to_string(),
        )
    };
    ui.horizontal_top(|ui| {
        status_glyph(ui, status, 30.0);
        ui.add_space(6.0);
        ui.vertical(|ui| {
            let color = match status {
                CheckStatus::Pass => GOOD,
                CheckStatus::Warn => WARN,
                CheckStatus::Fail => BAD,
            };
            title(ui, &headline, color);
            ui.label(RichText::new(d.step.text()).color(TEXT).size(14.0));
        });
    });
    ui.add_space(6.0);
    for row in &d.rows {
        ui.horizontal_top(|ui| {
            match tone_status(row.tone) {
                Some(s) => {
                    status_glyph(ui, s, 16.0);
                }
                None => {
                    ui.add_space(16.0 + ui.spacing().item_spacing.x);
                }
            }
            let color = if row.tone == Tone::Skip { WEAK } else { TEXT };
            let label = ui.add(egui::Label::new(RichText::new(&row.text).color(color)).wrap());
            if let Some(detail) = &row.detail {
                label.on_hover_text(detail);
            }
        });
    }
    if !d.errors.is_empty() {
        ui.add_space(4.0);
        egui::CollapsingHeader::new(
            RichText::new(format!(
                "Error lines from the console log ({})",
                d.errors.len()
            ))
            .color(BAD),
        )
        .id_salt("live_check_errors")
        .default_open(true)
        .show(ui, |ui| {
            let text: Vec<&str> = d.errors.iter().take(5).map(String::as_str).collect();
            ui.add(
                egui::Label::new(
                    RichText::new(text.join("\n"))
                        .monospace()
                        .size(11.5)
                        .color(WEAK),
                )
                .wrap(),
            );
        });
    }
    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        let windows =
            cfg!(windows) || std::env::var_os("DEADTUNE_FAKE_WINDOWS").is_some_and(|v| v == "1");
        if d.step.restarts() && windows && state.ctx.game_running {
            let restart =
                egui::Button::new(RichText::new("Restart Deadlock").color(theme::ON_ACCENT))
                    .fill(ACCENT);
            if ui
                .add_enabled(!state.relaunch.is_active(), restart)
                .on_hover_text("Closes Deadlock and starts it again through DeadTune")
                .clicked()
            {
                state.close_live_check();
                crate::views::restart_game(state);
            }
        }
        if ui
            .button("Copy report")
            .on_hover_text("The checklist and the console log lines that matter, as text")
            .clicked()
        {
            ui.ctx().copy_text(done.report.clone());
            state.status = Some(Status::Info(
                "Report copied. Paste it to us in one message.".into(),
            ));
        }
        if ui.button("Open folder").clicked()
            && let Err(e) = crate::views::open_external(state.reports_dir())
        {
            state.status = Some(Status::Error(format!("opening the reports folder: {e}")));
        }
        if ui.button("Check again").clicked() {
            state.start_live_check(Instant::now());
        }
        if ui.small_button("Close").clicked() {
            state.close_live_check();
        }
    });
    let saved = match &done.saved {
        Ok(path) => format!("Saved to {}", path.display()),
        Err(e) => format!("Couldn't save the report: {e}"),
    };
    let c = done.facts.web.counters;
    ui.label(
        RichText::new(format!(
            "{saved} \u{b7} bridge page waits: {}, wakes: {}, sleeps: {}",
            c.waits, c.wakes, c.sleeps
        ))
        .small()
        .color(WEAK),
    );
}
