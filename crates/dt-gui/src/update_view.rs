//! The update banner and the Updates settings card, shared by the simple and advanced views.

use dt_core::update::Channel;
use eframe::egui::{self, Color32, Margin, RichText, Stroke, Ui, ViewportCommand};

use crate::state::{AppState, Status};
use crate::theme::{ACCENT, BAD, WEAK};
use crate::update::{self, Phase, Release, UpdateState};
use crate::{simple, views, widgets};

/// Whether the banner has something to say. It stays out of the way while the game
/// relaunches, and a failed background check only shows in Settings.
pub fn wants_banner(state: &AppState) -> bool {
    if state.relaunch.is_active() {
        return false;
    }
    match &state.update.state {
        UpdateState::Available(_) => !state.update.later,
        UpdateState::Downloading { .. } | UpdateState::Ready { .. } => true,
        UpdateState::Failed { during, .. } => *during != Phase::Check,
        UpdateState::Idle | UpdateState::Checking | UpdateState::UpToDate { .. } => false,
    }
}

pub fn banner_frame(margin: Margin) -> egui::Frame {
    egui::Frame::new()
        .fill(ACCENT.gamma_multiply(0.14))
        .inner_margin(margin)
}

pub fn banner(ui: &mut Ui, state: &mut AppState) {
    let game_running = state.ctx.game_running;
    ui.horizontal(|ui| match state.update.state.clone() {
        UpdateState::Available(release) => {
            ui.colored_label(
                ACCENT,
                RichText::new(format!("DeadTune {} is available.", release.version)).strong(),
            );
            let mut update = ui.button("Update and restart");
            if game_running {
                update = update.on_hover_text("Only DeadTune restarts. Deadlock keeps running.");
            }
            if update.clicked() {
                state.install_update(release.clone());
            }
            if ui.button("What's new").clicked() {
                open_notes(state, &release);
            }
            if ui.button("Skip this version").clicked() {
                state.skip_update(&release);
            }
            if close_button(ui).on_hover_text("Later").clicked() {
                state.update.later = true;
            }
        }
        UpdateState::Downloading { .. } | UpdateState::Ready { .. } => {
            let (text, _) = progress_text(&state.update.state).expect("busy states have text");
            ui.colored_label(ACCENT, RichText::new(text).strong());
            if let UpdateState::Downloading { done, total, .. } = state.update.state {
                ui.add(
                    egui::ProgressBar::new(fraction(done, total))
                        .desired_width(160.0)
                        .show_percentage(),
                );
            }
        }
        UpdateState::Failed {
            message,
            during,
            release,
        } => {
            ui.colored_label(
                BAD,
                RichText::new(format!("Update failed while {}: {message}", during.verb())).strong(),
            );
            if ui.button("Retry").clicked() {
                retry(ui.ctx(), state, during, release);
            }
            if close_button(ui).clicked() {
                state.update.state = UpdateState::Idle;
            }
        }
        UpdateState::Idle | UpdateState::Checking | UpdateState::UpToDate { .. } => {}
    });
}

/// The Updates card body. `plain` picks the simple view's controls over the advanced view's.
pub fn settings(ui: &mut Ui, state: &mut AppState, plain: bool) {
    ui.label(RichText::new(format!("DeadTune {}", update::this_version())).strong());
    ui.horizontal(|ui| {
        let on = state.settings.start_with_windows;
        let toggle = ui.add_enabled_ui(cfg!(windows), |ui| {
            if plain {
                simple::switch(ui, on)
            } else {
                widgets::switch(ui, on)
            }
        });
        if toggle.inner.clicked()
            && let Err(e) = state.set_start_with_windows(!on)
        {
            state.status = Some(Status::Error(e));
        }
        ui.label("Start with Windows");
    });
    ui.label(
        RichText::new(
            "Runs quietly without a window and rebuilds DeadTune's game changes right after \
             Steam updates Deadlock, so a game update cannot leave them broken.",
        )
        .small()
        .color(WEAK),
    );
    if !update::AVAILABLE {
        ui.label(RichText::new(update::UNAVAILABLE).color(WEAK));
        return;
    }
    ui.horizontal(|ui| {
        let on = state.settings.update.enabled;
        let toggle = if plain {
            simple::switch(ui, on)
        } else {
            widgets::switch(ui, on)
        };
        if toggle.clicked() {
            state.settings.update.enabled = !on;
        }
        ui.label("Check for updates automatically");
    });
    ui.horizontal(|ui| {
        ui.label("Channel");
        let channels = [Channel::Stable, Channel::Testing];
        let selected = channels
            .iter()
            .position(|c| *c == state.settings.update.channel)
            .unwrap_or(0);
        if let Some(i) = widgets::segmented(ui, &["Stable", "Testing"], selected)
            && channels[i] != state.settings.update.channel
        {
            state.settings.update.channel = channels[i];
            if !state.update.state.busy() {
                state.update.state = UpdateState::Idle;
            }
        }
        ui.label(
            RichText::new("Testing gets early builds, which may break.")
                .small()
                .color(WEAK),
        );
    });
    ui.horizontal(|ui| {
        let busy = state.update.state.busy();
        if ui
            .add_enabled(!busy, egui::Button::new("Check now"))
            .clicked()
        {
            state.check_update(true);
        }
        if state.update.state == UpdateState::Checking {
            ui.spinner();
        }
        match &state.update.state {
            UpdateState::Available(release) => {
                let release = release.clone();
                ui.colored_label(
                    ACCENT,
                    format!("DeadTune {} is available.", release.version),
                );
                if ui.button("Update and restart").clicked() {
                    state.install_update(release);
                }
            }
            UpdateState::Failed {
                message,
                during,
                release,
            } => {
                let (message, during, release) = (message.clone(), *during, release.clone());
                ui.colored_label(BAD, format!("Failed while {}: {message}", during.verb()));
                if during != Phase::Check && ui.button("Retry").clicked() {
                    retry(ui.ctx(), state, during, release);
                }
            }
            other => {
                let text = progress_text(other).or_else(|| {
                    let at = state.settings.update.last_check?;
                    Some((format!("Last checked {}", local_time(at)), WEAK))
                });
                if let Some((text, color)) = text {
                    ui.colored_label(color, text);
                }
            }
        }
    });
}

/// Text for the states that need no buttons.
fn progress_text(state: &UpdateState) -> Option<(String, Color32)> {
    let text = match state {
        UpdateState::Checking => "Checking…".to_string(),
        UpdateState::UpToDate { checked_at } => {
            format!("You're up to date (checked {}).", local_time(*checked_at))
        }
        UpdateState::Downloading {
            release,
            done,
            total,
        } if *total > 0 && done >= total => {
            format!("Verifying and installing DeadTune {}…", release.version)
        }
        UpdateState::Downloading { release, .. } => {
            format!("Downloading DeadTune {}…", release.version)
        }
        UpdateState::Ready { version } => format!("DeadTune {version} is installed. Restarting…"),
        UpdateState::Idle | UpdateState::Available(_) | UpdateState::Failed { .. } => {
            return None;
        }
    };
    Some((text, WEAK))
}

fn fraction(done: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (done as f64 / total as f64).min(1.0) as f32
    }
}

fn local_time(at: chrono::DateTime<chrono::Utc>) -> String {
    at.with_timezone(&chrono::Local)
        .format("%-d %b %H:%M")
        .to_string()
}

fn close_button(ui: &mut Ui) -> egui::Response {
    widgets::icon_button(ui, true, WEAK, |painter, rect, color| {
        let stroke = Stroke::new(1.5, color);
        painter.line_segment([rect.left_top(), rect.right_bottom()], stroke);
        painter.line_segment([rect.right_top(), rect.left_bottom()], stroke);
    })
}

fn open_notes(state: &mut AppState, release: &Release) {
    if let Err(e) = views::open_external(&release.notes_url) {
        state.status = Some(Status::Error(format!(
            "couldn't open {}: {e}",
            release.notes_url
        )));
    }
}

fn retry(ctx: &egui::Context, state: &mut AppState, during: Phase, release: Option<Release>) {
    match (during, release) {
        (Phase::Restart, _) => restart(ctx, state),
        (_, Some(release)) => state.install_update(release),
        (_, None) => state.check_update(true),
    }
}

/// Starts the installed version with the same arguments, then closes this window. DeadTune
/// never touches the game process, so this is safe while Deadlock runs.
pub fn restart(ctx: &egui::Context, state: &mut AppState) {
    match update::spawn_new_exe() {
        Ok(()) => ctx.send_viewport_cmd(ViewportCommand::Close),
        Err(e) => {
            state.update.state = UpdateState::Failed {
                message: format!(
                    "the new version is installed but didn't start ({e}); close and reopen DeadTune"
                ),
                during: Phase::Restart,
                release: None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update::fake_release;
    use dt_core::update::Version;
    use eframe::egui::epaint::Shape;
    use eframe::egui::{Pos2, Rect, vec2};

    fn texts(shape: &Shape, out: &mut Vec<String>) {
        match shape {
            Shape::Text(t) => out.push(t.galley.text().to_string()),
            Shape::Vec(shapes) => shapes.iter().for_each(|s| texts(s, out)),
            _ => {}
        }
    }

    /// One frame of the whole simple view; returns every string it drew.
    fn simple_frame(state: &mut AppState) -> Vec<String> {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 820.0))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| simple::simple(ui, state));
        output.textures_delta.clear();
        let mut out = Vec::new();
        for clipped in &output.shapes {
            texts(&clipped.shape, &mut out);
        }
        out
    }

    #[test]
    fn simple_view_shows_the_banner_for_an_available_update() {
        let (_dir, mut state) = crate::state::testutil::state();
        assert!(
            !simple_frame(&mut state)
                .iter()
                .any(|t| t.contains("is available")),
            "no banner while idle"
        );
        state.update.state = UpdateState::Available(fake_release(Version(0, 9, 0)));
        let drawn = simple_frame(&mut state);
        for text in [
            "DeadTune 0.9.0 is available.",
            "Update and restart",
            "What's new",
            "Skip this version",
        ] {
            assert!(drawn.iter().any(|t| t == text), "{text:?} in {drawn:?}");
        }
        state.update.later = true;
        assert!(
            !simple_frame(&mut state)
                .iter()
                .any(|t| t.contains("is available")),
            "later hides it"
        );
    }

    #[test]
    fn banner_shows_progress_and_failures_but_not_a_failed_background_check() {
        let (_dir, mut state) = crate::state::testutil::state();
        let release = fake_release(Version(0, 9, 0));
        state.update.state = UpdateState::Downloading {
            release: release.clone(),
            done: 5,
            total: 10,
        };
        assert!(wants_banner(&state));
        state.update.state = UpdateState::Failed {
            message: "offline".into(),
            during: Phase::Check,
            release: None,
        };
        assert!(!wants_banner(&state));
        state.update.state = UpdateState::Failed {
            message: "signature: bad".into(),
            during: Phase::Verify,
            release: Some(release),
        };
        assert!(wants_banner(&state));
        state.relaunch = crate::relaunch::Relaunch::WaitingExit {
            since: std::time::Instant::now(),
        };
        assert!(!wants_banner(&state), "quiet during a game relaunch");
    }
}
