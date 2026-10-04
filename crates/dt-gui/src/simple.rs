//! The default, plain-language screens: find the game, welcome flow, and the simple view.

use dt_core::bridge::execfile::ExecFileBridge;
use dt_core::preset;
use eframe::egui::{self, Color32, RichText};

use crate::friendly::{self, human_error};
use crate::live::BridgeKind;
use crate::settings::{TargetSource, View};
use crate::state::{AppState, Setting, StartChoice, Status, Welcome};
use crate::views;
use dt_core::doctor::CheckStatus;

const GREEN: Color32 = Color32::from_rgb(90, 190, 110);
const YELLOW: Color32 = Color32::from_rgb(230, 180, 60);
const RED: Color32 = Color32::from_rgb(230, 100, 100);

fn big_button(ui: &mut egui::Ui, enabled: bool, text: &str) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text).size(18.0).strong())
            .min_size(egui::vec2(180.0, 40.0)),
    )
}

fn status_line(ui: &mut egui::Ui, status: &Option<Status>) {
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
        egui::Stroke::new(2.0, GREEN)
    } else {
        egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color)
    };
    let fill = if selected {
        GREEN.gamma_multiply(0.15)
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

pub fn simple(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Panel::top("simple_top").show(ui, |ui| {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.heading("DeadTune");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(egui::Button::new("⋯").min_size(egui::vec2(32.0, 32.0)))
                    .on_hover_text("Tools menu")
                    .clicked()
                    .then(|| state.menu_open = !state.menu_open);
                ui.add(egui::Button::new("Advanced").min_size(egui::vec2(70.0, 32.0)))
                    .clicked()
                    .then(|| state.settings.view = View::Advanced);
            });
        });
        ui.add_space(6.0);
    });

    if let Some(banner) = state.banner.clone() {
        egui::Panel::top("simple_banner").show(ui, |ui| {
            ui.horizontal(|ui| {
                let text = if banner.build.is_some() {
                    "Deadlock updated and reset your settings."
                } else {
                    "Something else changed the game's settings."
                };
                ui.colored_label(YELLOW, RichText::new(text).strong());
                if ui.button("Put my settings back").clicked() {
                    apply(ui.ctx(), state);
                }
                if ui.button("Ignore").clicked() {
                    state.banner = None;
                }
            });
        });
    }

    egui::Panel::bottom("simple_apply").show(ui, |ui| apply_bar(ui, state));

    egui::CentralPanel::default().show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            centered_column(ui, |ui| {
                ui.add_space(12.0);

                if state.menu_open {
                    safety_menu(ui, state);
                    ui.add_space(16.0);
                }

                preset_selector(ui, state);
                ui.add_space(16.0);

                settings_search_and_list(ui, state);
                ui.add_space(16.0);

                if state.settings.bind_helper_dismissed == false
                    && state.settings.bridge == BridgeKind::ExecFile
                {
                    bind_helper_compact(ui, state);
                    ui.add_space(16.0);
                }

                ui.add_space(4.0);
                ui.weak("DeadTune is free software (GPL-3.0). Presets by their authors, credited in Advanced > Settings.");
                ui.add_space(8.0);
            });
        });
    });
}

/// Center content in a column with max width ~760px
fn centered_column(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    let max_width = 760.0;
    let available = ui.available_width();
    if available <= max_width + 40.0 {
        add_contents(ui);
    } else {
        let margin = (available - max_width) / 2.0;
        ui.horizontal(|ui| {
            ui.add_space(margin);
            ui.vertical(|ui| {
                ui.allocate_ui(egui::vec2(max_width, 0.0), add_contents);
            });
        });
    }
}

fn apply_bar(ui: &mut egui::Ui, state: &mut AppState) {
    ui.add_space(6.0);
    let changes = state
        .preview
        .as_ref()
        .map(|p| p.live.len() + p.queued_cheat.len() + p.restart.len() + p.video_changes.len())
        .map_err(Clone::clone);
    ui.horizontal(|ui| {
        let ready = matches!(changes, Ok(n) if n > 0);
        if big_button(ui, ready, "Apply").clicked() {
            apply(ui.ctx(), state);
        }
        ui.vertical(|ui| {
            match &changes {
                Ok(0) => ui.label("Everything is applied."),
                Ok(1) => ui.label("1 change ready."),
                Ok(n) => ui.label(format!("{n} changes ready.")),
                Err(raw) => ui
                    .colored_label(RED, human_error(raw))
                    .on_hover_text(raw.as_str()),
            };
            if let Some(pending) = &state.pending_restart {
                ui.colored_label(
                    YELLOW,
                    format!(
                        "Restart Deadlock to load {} saved change(s).",
                        pending.names.len()
                    ),
                );
            } else {
                ui.weak("Takes effect next time you start Deadlock.");
            }
        });
        if state.is_dirty() && ui.button("Discard changes").clicked() {
            state.revert_all();
        }
    });
    status_line(ui, &state.status);
    ui.add_space(4.0);
}

fn safety_menu(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(RichText::new("Tools").size(15.0).strong());
    ui.add_space(8.0);

    if ui.button("Undo last change").clicked() {
        state.menu_open = false;
        state.status = Some(match state.undo_last() {
            Ok(()) => Status::Info(
                "Undone. The game files are back to before your last Apply.".into(),
            ),
            Err(e) => Status::Info(e),
        });
    }

    if ui.button("Restore original game files").clicked() {
        state.menu_open = false;
        state.status = Some(match state.restore_original_files() {
            Ok(()) => {
                Status::Info("The game files are back to how they were before DeadTune.".into())
            }
            Err(e) => Status::Info(e),
        });
    }

    let ranked = state.settings.source == TargetSource::RankedSafe;
    let label = if ranked {
        "Turn off Ranked-safe mode"
    } else {
        "Enable Ranked-safe mode"
    };
    if ui.button(label).clicked() {
        state.menu_open = false;
        state.status = Some(match state.toggle_ranked_safe() {
            Ok(_) if ranked => Status::Info(
                "Your settings are back. Takes effect next time you start Deadlock.".into(),
            ),
            Ok(_) => Status::Info(
                "Ranked-safe mode is on. Takes effect next time you start Deadlock.".into(),
            ),
            Err(e) => Status::Error(e),
        });
    }

    ui.separator();

    if ui.button("Run checks").clicked() {
        state.menu_open = false;
        state.run_checks();
    }

    if state.settings.bind_helper_dismissed && state.settings.bridge == BridgeKind::ExecFile {
        if ui.button("Show keybind help").clicked() {
            state.menu_open = false;
            state.settings.bind_helper_dismissed = false;
        }
    }

    ui.separator();
    ui.weak("Ranked-safe mode keeps your performance tweaks but reverts the engine config to vanilla so matchmaking never complains.");
}

fn preset_selector(ui: &mut egui::Ui, state: &mut AppState) {
    let current = match &state.profile.base {
        dt_core::profile::BaseRef::Preset(id) => preset::info(*id).label.to_string(),
        dt_core::profile::BaseRef::File(_) => "My original settings".to_string(),
    };

    ui.label(RichText::new("Preset").size(13.0).weak());
    ui.add_space(2.0);

    let mut picked = None;
    egui::ComboBox::from_id_salt("simple_base")
        .selected_text(RichText::new(&current).size(18.0).strong())
        .width(700.0)
        .height(200.0)
        .show_ui(ui, |ui| {
            for info in preset::all() {
                let Some(blurb) = friendly::preset_blurb(info.id) else {
                    continue;
                };
                let selected = state.profile.base == dt_core::profile::BaseRef::Preset(info.id);
                let label = format!("{} — {}", info.label, blurb);
                if ui
                    .selectable_label(selected, label)
                    .clicked()
                {
                    picked = Some(info.id);
                }
            }
        });

    if let Some(id) = picked {
        state.set_base(dt_core::profile::BaseRef::Preset(id));
    }
}

fn bind_helper_compact(ui: &mut egui::Ui, state: &mut AppState) {
    if state.settings.bind_helper_dismissed || state.settings.bridge != BridgeKind::ExecFile {
        return;
    }
    let frame = egui::Frame::new()
        .fill(egui::Color32::from_rgb(40, 50, 60))
        .inner_margin(egui::Margin::symmetric(12, 8));
    frame.show(ui, |ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new("Live key bind")
                    .size(14.0)
                    .strong(),
            );
            ui.weak("Some settings change while you play. Paste this in the game console once:");
            let line = ExecFileBridge::bind_hint(&state.settings.bind_key);
            ui.horizontal(|ui| {
                ui.code(RichText::new(&line).monospace().small());
                if ui.small_button("Copy").clicked() {
                    ui.ctx().copy_text(line.clone());
                    state.status = Some(Status::Info(
                        "Copied. Paste in Deadlock console (F7).".into(),
                    ));
                }
            });
            if ui.small_button("Done").clicked() {
                state.settings.bind_helper_dismissed = true;
            }
        });
    });
}

fn settings_search_and_list(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(RichText::new("Settings").size(15.0).weak());
    ui.add_space(4.0);

    let _search_response = ui.add(
        egui::TextEdit::singleline(&mut state.ui.search)
            .desired_width(700.0)
            .hint_text("Search settings...")
    );

    if state.settings.source == TargetSource::RankedSafe {
        ui.add_space(8.0);
        ui.colored_label(
            YELLOW,
            "Ranked-safe mode is on: changes are kept but not applied.",
        );
    }

    ui.add_space(12.0);

    let mut edits = Vec::new();
    let mut last_category = "";
    let search_lower = state.ui.search.to_lowercase();

    for name in friendly::simple_rows(state.catalog) {
        let Some(entry) = state.catalog.get(name) else {
            continue;
        };

        let label = friendly::label(name).unwrap_or(name);
        let label_lower = label.to_lowercase();

        if !search_lower.is_empty()
            && !label_lower.contains(&search_lower)
            && !entry.notes.to_lowercase().contains(&search_lower)
        {
            continue;
        }

        if entry.category != last_category {
            last_category = &entry.category;
            if last_category != "" {
                ui.add_space(8.0);
            }
            ui.label(
                RichText::new(&entry.category)
                    .size(13.0)
                    .color(ui.visuals().weak_text_color())
                    .strong(),
            );
            ui.add_space(4.0);
        }

        let setting = state.setting(name);
        let value = state.current_value(name);
        let is_changed = matches!(setting, Setting::Override(_) | Setting::CommentedOut);

        ui.horizontal(|ui| {
            ui.set_height(48.0);

            ui.vertical(|ui| {
                ui.set_width(380.0);
                let label_text = RichText::new(label).size(13.0).strong();
                ui.label(label_text);
                if !entry.notes.is_empty() {
                    ui.weak(RichText::new(&entry.notes).size(11.0));
                }
            });

            ui.add_space(12.0);

            if let Some(v) = views::control(ui, name, Some(entry), value.as_deref()) {
                edits.push((name, Some(v)));
            }

            if is_changed {
                ui.add_space(8.0);
                ui.colored_label(Color32::from_rgb(100, 200, 255), "✓ changed");
                if ui.small_button("Reset").clicked() {
                    edits.push((name, None));
                }
            }
        });
    }

    for (name, value) in edits {
        match value {
            Some(v) => {
                if let Err(e) = state.set_convar(name, v) {
                    state.status = Some(Status::Error(e.to_string()));
                }
            }
            None => state.revert_convar(name),
        }
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
