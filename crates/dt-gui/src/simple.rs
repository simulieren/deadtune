//! The default, plain-language screens: find the game, welcome flow, and the simple view.

use dt_core::preset;
use eframe::egui::{self, Color32, RichText};

use crate::friendly::{self, human_error};
use crate::state::{AppState, StartChoice, Status, Welcome};
use dt_core::doctor::CheckStatus;

const ACCENT: Color32 = crate::theme::ACCENT;
const GREEN: Color32 = crate::theme::OK;
const YELLOW: Color32 = crate::theme::WARN;
const RED: Color32 = crate::theme::BAD;

fn big_button(ui: &mut egui::Ui, enabled: bool, text: &str) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text).size(18.0).strong())
            .min_size(egui::vec2(180.0, 40.0)),
    )
}

pub fn status_line(ui: &mut egui::Ui, status: &Option<Status>) {
    match status {
        Some(Status::Info(m)) => {
            ui.label(m);
        }
        Some(Status::Error(raw)) => {
            ui.colored_label(RED, human_error(raw)).on_hover_text(raw);
        }
        None => {}
    }
}

/// Returns true when the player asks to try again with `input` (empty = search again).
pub fn find_game(ui: &mut egui::Ui, input: &mut String, error: Option<&str>) -> bool {
    ui.add_space(24.0);
    ui.heading(RichText::new("Welcome to DeadTune").size(26.0));
    ui.add_space(12.0);
    ui.colored_label(YELLOW, RichText::new("Couldn't find Deadlock").size(18.0));
    ui.label("Paste the folder where Deadlock is installed. It usually ends in steamapps\\common\\Deadlock.");
    ui.weak(
        "In Steam: right-click Deadlock, Manage, Browse local files, then copy the address bar.",
    );
    let response = ui.add(
        egui::TextEdit::singleline(input)
            .desired_width(560.0)
            .hint_text("C:\\Program Files (x86)\\Steam\\steamapps\\common\\Deadlock"),
    );
    if let Some(raw) = error
        && !input.trim().is_empty()
    {
        ui.colored_label(
            RED,
            "That folder doesn't look like a Deadlock install. Pick the folder named Deadlock.",
        )
        .on_hover_text(raw);
    }
    let submit = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    ui.add_space(8.0);
    big_button(ui, true, "Retry").clicked() || submit
}

const CARD_WIDTH: f32 = 250.0;
const CARD_HEIGHT: f32 = 84.0;

fn card(ui: &mut egui::Ui, selected: bool, title: &str, author: &str, blurb: &str) -> bool {
    let stroke = if selected {
        egui::Stroke::new(2.0, ACCENT)
    } else {
        egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color)
    };
    let fill = if selected {
        ACCENT.gamma_multiply(0.15)
    } else {
        Color32::TRANSPARENT
    };
    let frame = egui::Frame::group(ui.style())
        .stroke(stroke)
        .fill(fill)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.set_width(CARD_WIDTH);
                ui.set_height(CARD_HEIGHT);
                ui.label(RichText::new(title).size(16.0).strong());
                if !author.is_empty() {
                    ui.weak(format!("by {author}"));
                }
                ui.label(blurb);
            });
        });
    frame.response.interact(egui::Sense::click()).clicked()
}

pub fn welcome(ui: &mut egui::Ui, state: &mut AppState) {
    let Some(step) = state.welcome.clone() else {
        return;
    };
    egui::CentralPanel::default().show(ui, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(16.0);
            ui.heading(RichText::new("Welcome to DeadTune").size(26.0));
            ui.add_space(8.0);
            match step {
                Welcome::PickStart { choice } => pick_start(ui, state, choice),
                Welcome::Done { needs_restart } => {
                    ui.colored_label(GREEN, RichText::new("All set").size(20.0));
                    if needs_restart {
                        ui.label(RichText::new("Takes effect next time you start Deadlock.").size(16.0));
                        if state.ctx.game_running {
                            ui.label("Deadlock is running right now: restart it to see the difference.");
                        }
                    } else {
                        ui.label("Nothing was changed. Your current settings are kept as your starting point.");
                    }
                    ui.add_space(12.0);
                    if big_button(ui, true, "Continue").clicked() {
                        state.finish_welcome();
                    }
                }
            }
            status_line(ui, &state.status);
        });
    });
}

fn pick_start(ui: &mut egui::Ui, state: &mut AppState, choice: Option<StartChoice>) {
    ui.colored_label(GREEN, RichText::new("Found Deadlock").size(18.0))
        .on_hover_text(state.paths.game_root.display().to_string());
    ui.add_space(12.0);
    ui.label(RichText::new("Pick a starting preset").size(18.0).strong());
    ui.weak("You can fine-tune everything afterwards.");
    let mut picked = None;
    let columns = ((ui.available_width() / (CARD_WIDTH + 36.0)) as usize).max(1);
    egui::Grid::new("start_cards")
        .spacing([12.0, 12.0])
        .show(ui, |ui| {
            let mut col = 0;
            let mut next = |ui: &mut egui::Ui| {
                col += 1;
                if col % columns == 0 {
                    ui.end_row();
                }
            };
            let keep = choice == Some(StartChoice::KeepCurrent);
            if card(
                ui,
                keep,
                "Keep my current settings",
                "",
                "Change nothing now; start from what you have.",
            ) {
                picked = Some(StartChoice::KeepCurrent);
            }
            next(ui);
            for info in preset::all() {
                let Some(blurb) = friendly::preset_blurb(info.id) else {
                    continue;
                };
                let selected = choice == Some(StartChoice::Preset(info.id));
                if card(ui, selected, info.label, info.author, blurb) {
                    picked = Some(StartChoice::Preset(info.id));
                }
                next(ui);
            }
        });
    if let Some(choice) = picked
        && let Err(e) = state.choose_start(choice)
    {
        state.status = Some(Status::Error(e.to_string()));
    }
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        if big_button(ui, choice.is_some(), "Apply").clicked()
            && let Err(e) = state.welcome_apply()
        {
            state.status = Some(Status::Error(e));
        }
        if choice.is_none() {
            ui.label("Pick a preset above.");
        }
    });
    ui.weak("Takes effect next time you start Deadlock. Your original files are backed up first.");
}

/// Apply with a plain-language result.
pub fn apply(ctx: &egui::Context, state: &mut AppState) {
    match state.apply() {
        Ok(applied) => {
            let mut msg = String::from("Saved.");
            if applied.report.needs_restart {
                msg.push_str(" Takes effect next time you start Deadlock.");
            }
            if applied.report.pushed_live > 0 {
                msg.push_str(&format!(
                    " Some changes are ready now: press {} in game.",
                    state.settings.bind_key
                ));
            }
            if let Some(text) = applied.copy {
                ctx.copy_text(text);
            }
            state.status = Some(Status::Info(msg));
        }
        Err(e) => state.status = Some(Status::Error(e)),
    }
}

/// `plain` hides the technical detail behind a tooltip.
pub fn check_setup(ui: &mut egui::Ui, state: &mut AppState, plain: bool) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Check setup").size(17.0).strong());
        if ui
            .button(if state.checks.is_some() {
                "Check again"
            } else {
                "Run checks"
            })
            .clicked()
        {
            state.run_checks();
        }
    });
    let Some(checks) = &state.checks else { return };
    for check in checks {
        let (color, mark) = match check.status {
            CheckStatus::Pass => (GREEN, "OK"),
            CheckStatus::Warn => (YELLOW, "Warning"),
            CheckStatus::Fail => (RED, "Problem"),
        };
        ui.horizontal(|ui| {
            ui.colored_label(color, RichText::new(mark).strong());
            let name = ui.label(RichText::new(check.name).strong());
            if plain {
                name.on_hover_text(&check.detail);
            } else {
                ui.weak(&check.detail);
            }
        });
        if check.status != CheckStatus::Pass
            && let Some(fix) = &check.fix
        {
            ui.label(format!("    What to do: {fix}"));
        }
    }
}
