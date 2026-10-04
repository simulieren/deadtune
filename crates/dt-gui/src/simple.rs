//! The default, plain-language screens: find the game, welcome flow, and the simple view.

use dt_core::bridge::execfile::ExecFileBridge;
use dt_core::catalog::{CatalogEntry, Kind};
use dt_core::doctor::CheckStatus;
use dt_core::preset;
use dt_core::profile::BaseRef;
use eframe::egui::{self, Color32, RichText};

use crate::friendly::{self, Control, Unit, human_error};
use crate::live::BridgeKind;
use crate::settings::{TargetSource, View};
use crate::state::{AppState, StartChoice, Status, Welcome, bool_text, fmt_num, parse_bool};
use crate::theme::{ACCENT, BAD, BAR, BG, CARD, CARD_HOVER, DIM, GOOD, LINE, RADIUS, TEXT, WARN};
use crate::views;

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
            ui.colored_label(BAD, human_error(raw)).on_hover_text(raw);
        }
        None => {}
    }
}

/// Returns true when the player asks to try again with `input` (empty = search again).
pub fn find_game(ui: &mut egui::Ui, input: &mut String, error: Option<&str>) -> bool {
    ui.add_space(24.0);
    ui.heading(RichText::new("Welcome to DeadTune").size(26.0));
    ui.add_space(12.0);
    ui.colored_label(WARN, RichText::new("Couldn't find Deadlock").size(18.0));
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
            BAD,
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
        egui::Stroke::new(1.0, LINE)
    };
    let fill = if selected {
        ACCENT.gamma_multiply(0.15)
    } else {
        CARD
    };
    let frame = egui::Frame::group(ui.style())
        .stroke(stroke)
        .fill(fill)
        .corner_radius(RADIUS)
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
                    ui.colored_label(GOOD, RichText::new("All set").size(20.0));
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
    ui.colored_label(GOOD, RichText::new("Found Deadlock").size(18.0))
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

/// Apply with a plain-language result. The profile is saved with it, so "Discard" always
/// means "back to what was last applied".
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
            state.status = Some(match state.save_profile() {
                Ok(_) => Status::Info(msg),
                Err(e) => Status::Error(format!("saved the game files, but not your profile: {e}")),
            });
        }
        Err(e) => state.status = Some(Status::Error(e)),
    }
}

const LABEL_WIDTH: f32 = 210.0;
const SLIDER_WIDTH: f32 = 250.0;
/// Room kept for "tweaked" and the Reset button on a changed row.
const RESET_WIDTH: f32 = 150.0;
const GOAL_HEIGHT: f32 = 58.0;

pub fn simple(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Panel::top("simple_top")
        .frame(
            egui::Frame::new()
                .fill(BAR)
                .inner_margin(egui::Margin::symmetric(16, 8)),
        )
        .show_separator_line(false)
        .show(ui, |ui| top_bar(ui, state));
    if state.banner.is_some() {
        egui::Panel::top("simple_banner")
            .frame(
                egui::Frame::new()
                    .fill(WARN.gamma_multiply(0.18))
                    .inner_margin(egui::Margin::symmetric(16, 8)),
            )
            .show(ui, |ui| banner(ui, state));
    }
    egui::Panel::bottom("simple_apply")
        .frame(
            egui::Frame::new()
                .fill(BAR)
                .inner_margin(egui::Margin::symmetric(16, 10)),
        )
        .show(ui, |ui| apply_bar(ui, state));
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin {
            left: 16,
            right: 16,
            top: 12,
            bottom: 0,
        }))
        .show(ui, |ui| {
            hero(ui, state);
            bind_card(ui, state);
            ui.add_space(10.0);
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    groups(ui, state);
                    ui.add_space(6.0);
                    ui.collapsing(RichText::new("Check setup").size(16.0).strong(), |ui| {
                        check_setup(ui, state, true);
                    });
                    ui.add_space(8.0);
                    ui.weak("DeadTune is free software (GPL-3.0). Presets by their authors, credited in Advanced view > Settings.");
                    ui.add_space(12.0);
                });
        });
}

fn top_bar(ui: &mut egui::Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("DeadTune").size(21.0).strong().color(ACCENT));
        ui.add_space(6.0);
        ui.label(RichText::new(&state.profile.name).color(DIM));
        ui.add_space(6.0);
        if state.ctx.game_running {
            ui.label(RichText::new("Deadlock is running").size(13.0).color(GOOD));
        } else {
            ui.label(RichText::new("Deadlock is closed").size(13.0).color(DIM));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Advanced view").clicked() {
                state.settings.view = View::Advanced;
            }
            setup_menu(ui, state);
        });
    });
}

fn setup_menu(ui: &mut egui::Ui, state: &mut AppState) {
    ui.menu_button("Safety & setup", |ui| {
        ui.set_min_width(260.0);
        if ui.button("Undo last change").clicked() {
            state.status = Some(match state.undo_last() {
                Ok(()) => Status::Info(
                    "Undone. The game files are back to before your last Apply.".into(),
                ),
                Err(e) => Status::Info(e),
            });
            ui.close();
        }
        if ui.button("Restore original game files").clicked() {
            state.status = Some(match state.restore_original_files() {
                Ok(()) => {
                    Status::Info("The game files are back to how they were before DeadTune.".into())
                }
                Err(e) => Status::Info(e),
            });
            ui.close();
        }
        ui.separator();
        let ranked = state.settings.source == TargetSource::RankedSafe;
        let mut want = ranked;
        if ui
            .checkbox(&mut want, "Ranked-safe mode")
            .on_hover_text(
                "Puts the game's own performance settings back so matchmaking never complains. \
                 Your video settings stay. Untick to return to your settings.",
            )
            .changed()
        {
            state.status = Some(match state.toggle_ranked_safe() {
                Ok(_) if ranked => Status::Info(
                    "Your settings are back. Takes effect next time you start Deadlock.".into(),
                ),
                Ok(_) => Status::Info(
                    "Ranked-safe mode is on. Takes effect next time you start Deadlock.".into(),
                ),
                Err(e) => Status::Error(e),
            });
            ui.close();
        }
        ui.separator();
        if ui.button("Key bind for instant changes").clicked() {
            state.ui.bind_help_open = true;
            state.settings.bind_helper_dismissed = false;
            ui.close();
        }
    });
}

fn banner(ui: &mut egui::Ui, state: &mut AppState) {
    let Some(banner) = state.banner.clone() else {
        return;
    };
    ui.horizontal(|ui| {
        let text = if banner.build.is_some() {
            "Deadlock updated and reset your settings."
        } else {
            "Something else changed the game's settings."
        };
        ui.colored_label(WARN, RichText::new(text).strong());
        if ui.button("Put my settings back").clicked() {
            apply(ui.ctx(), state);
        }
        if ui.button("Ignore").clicked() {
            state.banner = None;
        }
    });
}

fn hero(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Frame::new()
        .fill(CARD)
        .corner_radius(10)
        .inner_margin(14)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("What do you want?").size(19.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let changed = state.changed_from_preset();
                    if changed > 0 {
                        if ui
                            .button("Reset all to preset")
                            .on_hover_text("Drops every tweak below and goes back to the preset")
                            .clicked()
                        {
                            state.reset_to_preset();
                        }
                        ui.label(RichText::new(format!("{changed} tweaked")).color(ACCENT));
                    } else {
                        ui.label(RichText::new("No tweaks").color(DIM));
                    }
                });
            });
            ui.add_space(8.0);
            goal_strip(ui, state);
            ui.add_space(8.0);
            preset_line(ui, state);
            if state.settings.source == TargetSource::RankedSafe {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.colored_label(
                        WARN,
                        "Ranked-safe mode is on: the game's own settings are in place and your tweaks are parked.",
                    );
                    if ui.button("Turn off").clicked() {
                        state.status = Some(match state.toggle_ranked_safe() {
                            Ok(_) => Status::Info(
                                "Your settings are back. Takes effect next time you start Deadlock.".into(),
                            ),
                            Err(e) => Status::Error(e),
                        });
                    }
                });
            }
        });
}

fn goal_strip(ui: &mut egui::Ui, state: &mut AppState) {
    let current = match &state.profile.base {
        BaseRef::Preset(id) => Some(*id),
        BaseRef::File(_) => None,
    };
    let gap = 8.0;
    let width = ui.available_width();
    let n = friendly::GOALS.len() as f32;
    let each = (width - gap * (n - 1.0)) / n;
    let (strip, _) = ui.allocate_exact_size(egui::vec2(width, GOAL_HEIGHT), egui::Sense::hover());
    let mut picked = None;
    for (i, (id, title, sub)) in friendly::GOALS.iter().enumerate() {
        let rect = egui::Rect::from_min_size(
            strip.min + egui::vec2(i as f32 * (each + gap), 0.0),
            egui::vec2(each, GOAL_HEIGHT),
        );
        if goal_card(ui, rect, current == Some(*id), title, sub).clicked() {
            picked = Some(*id);
        }
    }
    if let Some(id) = picked {
        state.set_base(BaseRef::Preset(id));
    }
}

fn goal_card(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    selected: bool,
    title: &str,
    sub: &str,
) -> egui::Response {
    let response = ui.interact(rect, ui.id().with(("goal", title)), egui::Sense::click());
    let (fill, stroke) = if selected {
        (ACCENT.gamma_multiply(0.2), egui::Stroke::new(2.0, ACCENT))
    } else if response.hovered() {
        (
            CARD_HOVER,
            egui::Stroke::new(1.0, Color32::from_rgb(90, 96, 112)),
        )
    } else {
        (BAR, egui::Stroke::new(1.0, LINE))
    };
    let painter = ui.painter();
    painter.rect(rect, RADIUS, fill, stroke, egui::StrokeKind::Inside);
    let title_color = if selected { ACCENT } else { TEXT };
    painter.text(
        rect.center() - egui::vec2(0.0, 10.0),
        egui::Align2::CENTER_CENTER,
        title,
        egui::FontId::proportional(17.0),
        title_color,
    );
    painter.text(
        rect.center() + egui::vec2(0.0, 11.0),
        egui::Align2::CENTER_CENTER,
        sub,
        egui::FontId::proportional(12.5),
        DIM,
    );
    response
}

fn preset_line(ui: &mut egui::Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        let (name, blurb) = match &state.profile.base {
            BaseRef::Preset(id) => {
                let info = preset::info(*id);
                (
                    format!("{} by {}", info.label, info.author),
                    friendly::preset_blurb(*id).unwrap_or(""),
                )
            }
            BaseRef::File(_) => (
                "My original settings".to_string(),
                "Your files as they were before DeadTune.",
            ),
        };
        ui.label(RichText::new("Preset:").color(DIM));
        ui.label(RichText::new(name).strong());
        ui.label(RichText::new(blurb).color(DIM));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut picked = None;
            egui::ComboBox::from_id_salt("simple_base")
                .selected_text("All presets")
                .width(150.0)
                .show_ui(ui, |ui| {
                    for info in preset::all() {
                        let Some(blurb) = friendly::preset_blurb(info.id) else {
                            continue;
                        };
                        let selected = state.profile.base == BaseRef::Preset(info.id);
                        if ui
                            .selectable_label(selected, format!("{} ({})", info.label, info.author))
                            .on_hover_text(blurb)
                            .clicked()
                        {
                            picked = Some(info.id);
                        }
                    }
                });
            if let Some(id) = picked {
                state.set_base(BaseRef::Preset(id));
            }
        });
    });
}

fn bind_card(ui: &mut egui::Ui, state: &mut AppState) {
    let open = state.ui.bind_help_open;
    let strip =
        !state.settings.bind_helper_dismissed && state.settings.bridge == BridgeKind::ExecFile;
    if !open && !strip {
        return;
    }
    ui.add_space(8.0);
    egui::Frame::new()
        .fill(CARD)
        .corner_radius(RADIUS)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Instant changes").strong());
                ui.label(
                    RichText::new(
                        "Bind a key once and settings like the FPS limit change while you play.",
                    )
                    .color(DIM),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Done, hide").clicked() {
                        state.settings.bind_helper_dismissed = true;
                        state.ui.bind_help_open = false;
                    }
                    if !open && ui.button("Show me").clicked() {
                        state.ui.bind_help_open = true;
                    }
                });
            });
            if open {
                ui.add_space(4.0);
                ui.label("1. In Deadlock, open the console (F7) and paste this line:");
                let line = ExecFileBridge::bind_hint(&state.settings.bind_key);
                ui.horizontal(|ui| {
                    ui.code(RichText::new(&line).size(14.0));
                    if ui.button("Copy").clicked() {
                        ui.ctx().copy_text(line.clone());
                        state.status = Some(Status::Info(
                            "Copied. Paste it into the Deadlock console.".into(),
                        ));
                    }
                });
                ui.label(format!(
                    "2. After pressing Apply here, press {} in game to load the change.",
                    state.settings.bind_key
                ));
            }
        });
}

type Edit = (&'static str, Option<String>);

fn groups(ui: &mut egui::Ui, state: &mut AppState) {
    let rows = friendly::simple_rows(state.catalog);
    let mut edits: Vec<Edit> = Vec::new();
    for (i, (title, names)) in friendly::GROUPS.iter().enumerate() {
        let names: Vec<&'static str> = names.iter().copied().filter(|n| rows.contains(n)).collect();
        let changed = names.iter().filter(|n| state.is_changed(n)).count();
        let id = ui.make_persistent_id(("simple_group", title));
        egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            id,
            i < friendly::OPEN_GROUPS,
        )
        .show_header(ui, |ui| {
            ui.label(RichText::new(*title).size(16.0).strong());
            ui.label(
                RichText::new(format!("{} settings", names.len()))
                    .small()
                    .color(DIM),
            );
            if changed > 0 {
                ui.label(
                    RichText::new(format!("{changed} tweaked"))
                        .small()
                        .color(ACCENT),
                );
            }
        })
        .body(|ui| {
            for name in names {
                row(ui, state, name, &mut edits);
            }
        });
        ui.add_space(2.0);
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

fn row(ui: &mut egui::Ui, state: &AppState, name: &'static str, edits: &mut Vec<Edit>) {
    let Some(entry) = state.catalog.get(name) else {
        return;
    };
    let Some(control) = friendly::control(name, &entry.kind) else {
        return;
    };
    let changed = state.is_changed(name);
    let value = state.current_value(name).unwrap_or_default();
    let fill = if changed {
        ACCENT.gamma_multiply(0.08)
    } else {
        Color32::TRANSPARENT
    };
    egui::Frame::new()
        .fill(fill)
        .corner_radius(RADIUS)
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let mut text = RichText::new(friendly::label(name).unwrap_or(name)).strong();
                if changed {
                    text = text.color(ACCENT);
                }
                views::left_label(ui, LABEL_WIDTH, egui::Label::new(text).truncate())
                    .on_hover_text(friendly::help(name, state.catalog));
                let new_value = match control {
                    Control::Toggle => {
                        let on = parse_bool(&value);
                        segments(ui, name, &["Off", "On"], Some(on as usize))
                            .map(|i| bool_text(i == 1, Some(&value)))
                    }
                    Control::Levels(levels) => {
                        let mut labels: Vec<String> = Vec::new();
                        for (_, l) in levels {
                            if !labels.iter().any(|x| x == l) {
                                labels.push((*l).to_string());
                            }
                        }
                        let current = friendly::level_label(levels, &value);
                        if current.is_none() {
                            labels.push(format!("Custom ({value})"));
                        }
                        let selected = match current {
                            Some(c) => labels.iter().position(|l| l == c),
                            None => Some(labels.len() - 1),
                        };
                        segments(ui, name, &labels, selected).and_then(|i| {
                            levels
                                .iter()
                                .find(|(_, l)| *l == labels[i])
                                .map(|(raw, _)| (*raw).to_string())
                        })
                    }
                    Control::Slider { unit, special } => slider(ui, entry, unit, special, &value),
                };
                if let Some(v) = new_value {
                    edits.push((name, Some(v)));
                }
                let reserve = if changed { RESET_WIDTH } else { 0.0 };
                let help_width = (ui.available_width() - reserve).max(0.0);
                let help = friendly::help(name, state.catalog);
                ui.allocate_ui_with_layout(
                    egui::vec2(help_width, ui.spacing().interact_size.y),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.add_space(10.0);
                        ui.add(
                            egui::Label::new(RichText::new(help).size(13.0).color(DIM)).truncate(),
                        )
                        .on_hover_text(help);
                    },
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if changed {
                        if ui
                            .small_button("Reset")
                            .on_hover_text("Back to the preset's value")
                            .clicked()
                        {
                            edits.push((name, None));
                        }
                        ui.label(RichText::new("tweaked").small().color(ACCENT));
                    }
                });
            });
        });
}

/// A row of exclusive choices; returns the index of a newly chosen one.
fn segments(
    ui: &mut egui::Ui,
    salt: &str,
    labels: &[impl AsRef<str>],
    selected: Option<usize>,
) -> Option<usize> {
    let mut picked = None;
    ui.push_id(salt, |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.spacing_mut().button_padding = egui::vec2(10.0, 3.0);
        for (i, label) in labels.iter().enumerate() {
            let on = selected == Some(i);
            let (fill, color) = if on {
                (ACCENT, Color32::BLACK)
            } else {
                (ui.visuals().widgets.inactive.weak_bg_fill, TEXT)
            };
            let button = egui::Button::new(RichText::new(label.as_ref()).color(color))
                .fill(fill)
                .corner_radius(RADIUS);
            if ui.add(button).clicked() && !on {
                picked = Some(i);
            }
        }
    });
    picked
}

fn slider(
    ui: &mut egui::Ui,
    entry: &CatalogEntry,
    unit: Unit,
    special: friendly::Specials,
    value: &str,
) -> Option<String> {
    let integer = matches!(entry.kind, Kind::Int);
    let mut v: f64 = value.trim().parse().unwrap_or(0.0);
    let [lo, hi] = entry.range?;
    let step = entry.step.unwrap_or(if integer { 1.0 } else { 0.0 });
    let mut slider = egui::Slider::new(&mut v, lo..=hi)
        .custom_formatter(move |x, _| friendly::slider_text(unit, special, x))
        .custom_parser(move |text| friendly::slider_parse(unit, special, text));
    slider = if integer {
        slider.integer()
    } else {
        slider.max_decimals(3)
    };
    if step > 0.0 {
        slider = slider.step_by(step);
    }
    ui.spacing_mut().slider_width = SLIDER_WIDTH;
    ui.add(slider).changed().then(|| fmt_num(v, integer))
}

fn apply_bar(ui: &mut egui::Ui, state: &mut AppState) {
    let plan: Result<(usize, usize), String> = state
        .preview
        .as_ref()
        .map(|p| {
            (
                p.live.len(),
                p.restart.len() + p.queued_cheat.len() + p.video_changes.len(),
            )
        })
        .map_err(Clone::clone);
    let (live, restart) = plan.clone().unwrap_or((0, 0));
    let total = live + restart;
    ui.horizontal(|ui| {
        let button = egui::Button::new(
            RichText::new("Apply")
                .size(18.0)
                .strong()
                .color(Color32::BLACK),
        )
        .fill(ACCENT)
        .corner_radius(RADIUS)
        .min_size(egui::vec2(170.0, 42.0));
        if ui.add_enabled(total > 0, button).clicked() {
            apply(ui.ctx(), state);
        }
        ui.add_space(6.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            match &plan {
                Err(raw) => {
                    ui.colored_label(BAD, human_error(raw))
                        .on_hover_text(raw.as_str());
                }
                Ok(_) => {
                    let headline = if total == 0 {
                        "Everything is applied.".to_string()
                    } else {
                        let preset = match &state.profile.base {
                            BaseRef::Preset(id) => format!("{} preset", preset::info(*id).label),
                            BaseRef::File(_) => "your original settings".to_string(),
                        };
                        match state.changed_from_preset() {
                            0 => format!("Ready to apply: {preset}."),
                            1 => format!("Ready to apply: {preset} + 1 tweak."),
                            n => format!("Ready to apply: {preset} + {n} tweaks."),
                        }
                    };
                    ui.label(RichText::new(headline).size(16.0).strong());
                    let (effect, color) = effect_text(state, live, restart);
                    let detail = match total {
                        0 => effect,
                        1 => format!("1 setting changes. {effect}"),
                        n => format!("{n} settings change. {effect}"),
                    };
                    ui.label(RichText::new(detail).color(color));
                }
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if state.is_dirty()
                && ui
                    .button("Discard")
                    .on_hover_text("Back to what was last applied")
                    .clicked()
            {
                state.revert_all();
            }
            status_line(ui, &state.status);
        });
    });
}

fn effect_text(state: &AppState, live: usize, restart: usize) -> (String, Color32) {
    if live + restart == 0 {
        return match &state.pending_restart {
            Some(p) => (
                format!(
                    "Restart Deadlock to load {} saved change(s).",
                    p.names.len()
                ),
                WARN,
            ),
            None => ("The game files match what you see here.".into(), DIM),
        };
    }
    let key = &state.settings.bind_key;
    let text = match (state.ctx.game_running && live > 0, restart) {
        (true, 0) => format!("Instant: press {key} in game after Apply."),
        (true, _) => {
            format!("{live} instant (press {key} in game), {restart} after you restart Deadlock.")
        }
        (false, _) => "Takes effect next time you start Deadlock.".into(),
    };
    (text, DIM)
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
            CheckStatus::Pass => (GOOD, "OK"),
            CheckStatus::Warn => (WARN, "Warning"),
            CheckStatus::Fail => (BAD, "Problem"),
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
