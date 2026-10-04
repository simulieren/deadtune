//! The default, plain-language screens: find the game, welcome flow, and the simple view
//! (a left rail of sections, the selected section's settings as cards, an Apply bar).

use dt_core::bridge::execfile::ExecFileBridge;
use dt_core::catalog::{CatalogEntry, Kind};
use dt_core::doctor::CheckStatus;
use dt_core::preset::{self, PresetId};
use dt_core::profile::BaseRef;
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Layout, Margin, Rect, RichText, Sense,
    Stroke, StrokeKind, Ui, vec2,
};

use crate::friendly::{self, Control, human_error};
use crate::live::BridgeKind;
use crate::settings::{TargetSource, View};
use crate::state::{AppState, Section, StartChoice, Status, Timing, Welcome, bool_text, fmt_num};
use crate::theme::{
    self, ACCENT, BAD, BORDER, CARD_HOVER, GOOD, ON_ACCENT, RAIL, TEXT, WARN, WEAK,
};

fn big_button(ui: &mut Ui, enabled: bool, text: &str) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text).size(16.0).strong()).min_size(vec2(160.0, 40.0)),
    )
}

fn accent_button(ui: &mut Ui, enabled: bool, text: &str) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text).size(16.0).strong().color(ON_ACCENT))
            .fill(ACCENT)
            .min_size(vec2(150.0, 40.0)),
    )
}

fn status_line(ui: &mut Ui, status: &Option<Status>) {
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
pub fn find_game(ui: &mut Ui, input: &mut String, error: Option<&str>) -> bool {
    ui.add_space(24.0);
    ui.label(RichText::new("Welcome to DeadTune").text_style(theme::title()));
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

fn card(ui: &mut Ui, selected: bool, title: &str, author: &str, blurb: &str) -> bool {
    let stroke = if selected {
        Stroke::new(2.0, ACCENT)
    } else {
        Stroke::new(1.0, BORDER)
    };
    let fill = if selected {
        ACCENT.gamma_multiply(0.12)
    } else {
        theme::CARD
    };
    let frame = theme::card().stroke(stroke).fill(fill).show(ui, |ui| {
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
    frame.response.interact(Sense::click()).clicked()
}

pub fn welcome(ui: &mut Ui, state: &mut AppState) {
    let Some(step) = state.welcome.clone() else {
        return;
    };
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::BG).inner_margin(Margin::same(28)))
        .show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.label(RichText::new("Welcome to DeadTune").text_style(theme::title()));
                ui.add_space(8.0);
                match step {
                    Welcome::PickStart { choice } => pick_start(ui, state, choice),
                    Welcome::Done { needs_restart } => {
                        ui.colored_label(GOOD, RichText::new("All set").size(20.0));
                        if needs_restart {
                            ui.label(
                                RichText::new("Takes effect next time you start Deadlock.")
                                    .size(16.0),
                            );
                            if state.ctx.game_running {
                                ui.label(
                                    "Deadlock is running right now: restart it to see the difference.",
                                );
                            }
                        } else {
                            ui.label("Nothing was changed. Your current settings are kept as your starting point.");
                        }
                        ui.add_space(12.0);
                        if accent_button(ui, true, "Continue").clicked() {
                            state.finish_welcome();
                        }
                    }
                }
                status_line(ui, &state.status);
            });
        });
}

fn pick_start(ui: &mut Ui, state: &mut AppState, choice: Option<StartChoice>) {
    ui.colored_label(GOOD, RichText::new("Found Deadlock").size(18.0))
        .on_hover_text(state.paths.game_root.display().to_string());
    ui.add_space(12.0);
    ui.label(RichText::new("Pick a starting preset").size(18.0).strong());
    ui.weak("You can fine-tune everything afterwards.");
    let mut picked = None;
    let columns = ((ui.available_width() / (CARD_WIDTH + 44.0)) as usize).max(1);
    egui::Grid::new("start_cards")
        .spacing([12.0, 12.0])
        .show(ui, |ui| {
            let mut col = 0;
            let mut next = |ui: &mut Ui| {
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
        if accent_button(ui, choice.is_some(), "Apply").clicked()
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

/// Apply with a plain-language result; what was applied becomes the saved profile.
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
                Err(e) => Status::Error(format!("saving your profile: {e}")),
            });
        }
        Err(e) => state.status = Some(Status::Error(e)),
    }
}

/// What the simple view asks of the state, collected while drawing and run afterwards.
enum Edit {
    Set(&'static str, String),
    Reset(Vec<&'static str>),
    ResetAll,
    Base(PresetId),
    Go(Section),
    Advanced,
}

pub fn simple(ui: &mut Ui, state: &mut AppState) {
    let mut edits = Vec::new();
    if let Some(banner) = state.banner.clone() {
        egui::Panel::top("simple_banner")
            .frame(
                egui::Frame::new()
                    .fill(WARN.gamma_multiply(0.18))
                    .inner_margin(Margin::symmetric(16, 10)),
            )
            .show(ui, |ui| {
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
            });
    }
    egui::Panel::bottom("simple_apply")
        .frame(
            egui::Frame::new()
                .fill(RAIL)
                .inner_margin(Margin::symmetric(24, 12)),
        )
        .show(ui, |ui| apply_bar(ui, state));
    egui::Panel::left("simple_rail")
        .exact_size(224.0)
        .resizable(false)
        .frame(
            egui::Frame::new()
                .fill(RAIL)
                .inner_margin(Margin::symmetric(12, 18)),
        )
        .show(ui, |ui| rail(ui, state, &mut edits));
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::BG).inner_margin(Margin {
            left: 28,
            right: 28,
            top: 22,
            bottom: 0,
        }))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    let section = state.ui.section;
                    header(ui, state, section, &mut edits);
                    match section {
                        Section::Overview => overview(ui, state, &mut edits),
                        Section::Hud => {
                            theme::card().show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                crate::hud_view::hud(ui, state);
                            });
                        }
                        Section::Safety => safety(ui, state),
                        _ => settings_page(ui, state, section, &mut edits),
                    }
                    ui.add_space(24.0);
                });
        });
    for edit in edits {
        match edit {
            Edit::Set(name, value) => {
                if let Err(e) = state.set_convar(name, value) {
                    state.status = Some(Status::Error(e.to_string()));
                }
            }
            Edit::Reset(names) => state.reset_convars(names),
            Edit::ResetAll => state.reset_to_preset(),
            Edit::Base(id) => state.set_base(BaseRef::Preset(id)),
            Edit::Go(section) => state.ui.section = section,
            Edit::Advanced => state.settings.view = View::Advanced,
        }
    }
}

fn section_changes(state: &AppState, section: Section) -> usize {
    match section {
        Section::Overview => state.changed_count(friendly::ROWS.iter().map(|r| r.name)),
        Section::Hud => state.profile.hud.elements.len(),
        Section::Safety => 0,
        s => state.changed_count(friendly::section_names(s)),
    }
}

fn preset_name(state: &AppState) -> String {
    match &state.profile.base {
        BaseRef::Preset(id) => preset::info(*id).label.to_string(),
        BaseRef::File(_) => "My original settings".into(),
    }
}

fn rail(ui: &mut Ui, state: &AppState, edits: &mut Vec<Edit>) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label(RichText::new("Dead").size(24.0).strong().color(TEXT));
        ui.label(RichText::new("Tune").size(24.0).strong().color(ACCENT));
    });
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(
            RichText::new(format!("Preset: {}", preset_name(state)))
                .small()
                .color(WEAK),
        );
    });
    ui.add_space(14.0);
    for (i, section) in Section::ALL.into_iter().enumerate() {
        let heading = match i {
            1 => Some("SETTINGS"),
            6 => Some("MORE"),
            _ => None,
        };
        if let Some(heading) = heading {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.label(RichText::new(heading).size(11.0).strong().color(WEAK));
            });
        }
        if nav_item(
            ui,
            section,
            state.ui.section == section,
            section_changes(state, section),
        ) {
            edits.push(Edit::Go(section));
        }
    }
    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
        if ui
            .add(egui::Button::new(RichText::new("Advanced view").color(WEAK)).frame(false))
            .on_hover_text("Every setting, profiles, backups, benchmarks")
            .clicked()
        {
            edits.push(Edit::Advanced);
        }
        ui.add_space(4.0);
        if state.settings.source == TargetSource::RankedSafe {
            dot_label(ui, WARN, "Ranked-safe mode on");
        }
        if state.ctx.game_running {
            dot_label(ui, GOOD, "Deadlock is running");
        } else {
            dot_label(ui, WEAK, "Deadlock is closed");
        }
    });
}

fn dot_label(ui: &mut Ui, color: Color32, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        let (rect, _) = ui.allocate_exact_size(vec2(10.0, 16.0), Sense::hover());
        ui.painter().circle_filled(rect.center(), 4.0, color);
        ui.label(RichText::new(text).small().color(WEAK));
    });
}

fn nav_item(ui: &mut Ui, section: Section, selected: bool, changes: usize) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, CornerRadius::same(6), CARD_HOVER);
        painter.rect_filled(
            Rect::from_min_size(rect.min + vec2(0.0, 8.0), vec2(3.0, rect.height() - 16.0)),
            CornerRadius::same(2),
            ACCENT,
        );
    } else if response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(6), CARD_HOVER.gamma_multiply(0.6));
    }
    let color = if selected {
        TEXT
    } else {
        WEAK.gamma_multiply(1.15)
    };
    painter.text(
        rect.left_center() + vec2(14.0, 0.0),
        Align2::LEFT_CENTER,
        section.label(),
        FontId::proportional(15.0),
        color,
    );
    if changes > 0 {
        let text = changes.to_string();
        let center = rect.right_center() - vec2(20.0, 0.0);
        let badge = Rect::from_center_size(center, vec2(10.0 + 7.0 * text.len() as f32, 18.0));
        painter.rect_filled(badge, CornerRadius::same(255), ACCENT.gamma_multiply(0.22));
        painter.text(
            center,
            Align2::CENTER_CENTER,
            text,
            FontId::proportional(11.5),
            ACCENT,
        );
    }
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

fn header(ui: &mut Ui, state: &AppState, section: Section, edits: &mut Vec<Edit>) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new(section.label())
                    .text_style(theme::title())
                    .strong(),
            );
            ui.label(RichText::new(section.subtitle()).color(WEAK));
        });
        let names = friendly::section_names(section);
        let changed = state.changed_count(names.iter().copied());
        if section != Section::Overview && changed > 0 {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .button(format!("Reset {changed} in this section"))
                    .on_hover_text("Puts these settings back to what your preset uses")
                    .clicked()
                {
                    edits.push(Edit::Reset(names));
                }
            });
        }
    });
    if state.settings.source == TargetSource::RankedSafe && section != Section::Safety {
        ui.add_space(6.0);
        egui::Frame::new()
            .fill(WARN.gamma_multiply(0.14))
            .corner_radius(CornerRadius::same(theme::RADIUS))
            .inner_margin(Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.colored_label(
                    WARN,
                    "Ranked-safe mode is on: the game uses its own settings. Changes here are kept for later.",
                );
            });
    }
    ui.add_space(14.0);
}

fn card_title(ui: &mut Ui, title: &str) {
    ui.label(
        RichText::new(title)
            .text_style(egui::TextStyle::Heading)
            .strong(),
    );
    ui.add_space(4.0);
}

fn overview(ui: &mut Ui, state: &AppState, edits: &mut Vec<Edit>) {
    preset_picker(ui, state, edits);
    ui.add_space(14.0);
    let wide = ui.available_width() >= 760.0;
    let status_width = 320.0;
    let gap = 14.0;
    if wide {
        ui.horizontal_top(|ui| {
            let left = ui.available_width() - status_width - gap;
            ui.vertical(|ui| {
                ui.set_width(left);
                key_settings(ui, state, edits);
            });
            ui.add_space(gap - ui.spacing().item_spacing.x);
            ui.vertical(|ui| {
                ui.set_width(status_width);
                status_card(ui, state, edits);
            });
        });
    } else {
        key_settings(ui, state, edits);
        ui.add_space(gap);
        status_card(ui, state, edits);
    }
}

fn preset_picker(ui: &mut Ui, state: &AppState, edits: &mut Vec<Edit>) {
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            card_title(ui, "Your preset");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new("More FPS").small().color(WEAK));
                let (rect, _) = ui.allocate_exact_size(vec2(120.0, 10.0), Sense::hover());
                let y = rect.center().y;
                let painter = ui.painter();
                painter.line_segment(
                    [rect.left_center(), rect.right_center()],
                    Stroke::new(1.0, WEAK.gamma_multiply(0.6)),
                );
                painter.line_segment(
                    [rect.right_center(), egui::pos2(rect.right() - 5.0, y - 4.0)],
                    Stroke::new(1.0, WEAK.gamma_multiply(0.6)),
                );
                painter.line_segment(
                    [rect.right_center(), egui::pos2(rect.right() - 5.0, y + 4.0)],
                    Stroke::new(1.0, WEAK.gamma_multiply(0.6)),
                );
                ui.label(RichText::new("Better looking").small().color(WEAK));
            });
        });
        let tiles = friendly::PRESET_SPECTRUM;
        let gap = 8.0;
        let n = tiles.len() as f32;
        let width = ((ui.available_width() - gap * (n - 1.0)) / n - 0.5)
            .floor()
            .max(118.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(gap, gap);
            for (id, tag) in tiles {
                let selected = state.profile.base == BaseRef::Preset(*id);
                if preset_tile(ui, width, selected, tag, preset::info(*id).label) {
                    edits.push(Edit::Base(*id));
                }
            }
        });
        ui.add_space(4.0);
        let blurb = match &state.profile.base {
            BaseRef::Preset(id) => {
                let info = preset::info(*id);
                format!(
                    "{} by {}. {}",
                    info.label,
                    info.author,
                    friendly::preset_blurb(*id).unwrap_or("")
                )
            }
            BaseRef::File(_) => {
                "You're on your original settings. Pick a preset to start from one instead.".into()
            }
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(blurb).color(WEAK));
            let changed = section_changes(state, Section::Overview);
            if changed > 0 {
                ui.label(
                    RichText::new(format!(
                        "Your {changed} changes stay on top when you switch."
                    ))
                    .color(WEAK),
                );
            }
        });
    });
}

fn preset_tile(ui: &mut Ui, width: f32, selected: bool, tag: &str, name: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 64.0), Sense::click());
    let painter = ui.painter();
    let (fill, stroke) = if selected {
        (ACCENT.gamma_multiply(0.14), Stroke::new(2.0, ACCENT))
    } else if response.hovered() {
        (CARD_HOVER, Stroke::new(1.0, WEAK.gamma_multiply(0.5)))
    } else {
        (theme::BG, Stroke::new(1.0, BORDER))
    };
    painter.rect(
        rect,
        CornerRadius::same(theme::RADIUS),
        fill,
        stroke,
        StrokeKind::Inside,
    );
    let tag_color = if selected { ACCENT } else { TEXT };
    painter.text(
        rect.left_top() + vec2(12.0, 12.0),
        Align2::LEFT_TOP,
        tag,
        FontId::proportional(15.5),
        tag_color,
    );
    let name = painter.layout(
        name.to_string(),
        FontId::proportional(11.5),
        WEAK,
        width - 24.0,
    );
    painter.galley(rect.left_top() + vec2(12.0, 35.0), name, WEAK);
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

fn key_settings(ui: &mut Ui, state: &AppState, edits: &mut Vec<Edit>) {
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        card_title(ui, "Biggest FPS wins");
        rows(ui, state, friendly::KEY_SETTINGS, edits);
    });
}

fn status_card(ui: &mut Ui, state: &AppState, edits: &mut Vec<Edit>) {
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        card_title(ui, "Status");
        let changed = section_changes(state, Section::Overview);
        status_item(ui, if changed > 0 { ACCENT } else { GOOD }, |ui| {
            if changed > 0 {
                ui.label(format!("{changed} settings changed from your preset"));
                if ui.small_button("Reset all to preset").clicked() {
                    edits.push(Edit::ResetAll);
                }
            } else {
                ui.label("Using your preset as is");
            }
        });
        if let Some(pending) = &state.pending_restart {
            status_item(ui, WARN, |ui| {
                ui.label(format!(
                    "Restart Deadlock to load {} saved changes",
                    pending.names.len()
                ));
            });
        }
        let ranked = state.settings.source == TargetSource::RankedSafe;
        status_item(ui, if ranked { WARN } else { WEAK }, |ui| {
            ui.label(if ranked {
                "Ranked-safe mode is on"
            } else {
                "Ranked-safe mode is off"
            });
            if ui.small_button("Safety options").clicked() {
                edits.push(Edit::Go(Section::Safety));
            }
        });
        if state.settings.bridge == BridgeKind::ExecFile {
            let ready = state.settings.bind_helper_dismissed;
            status_item(ui, if ready { GOOD } else { WEAK }, |ui| {
                if ready {
                    ui.label(format!(
                        "Instant changes: press {} in game",
                        state.settings.bind_key
                    ));
                } else {
                    ui.label("Instant changes are not set up");
                    if ui.small_button("Set up (1 minute)").clicked() {
                        edits.push(Edit::Go(Section::Safety));
                    }
                }
            });
        }
    });
}

fn status_item(ui: &mut Ui, color: Color32, add: impl FnOnce(&mut Ui)) {
    ui.horizontal_top(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(10.0, 20.0), Sense::hover());
        ui.painter().circle_filled(rect.center(), 4.0, color);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            add(ui)
        });
    });
    ui.add_space(4.0);
}

fn settings_page(ui: &mut Ui, state: &AppState, section: Section, edits: &mut Vec<Edit>) {
    let groups = friendly::groups(section);
    let columns = if ui.available_width() >= 1100.0 && groups.len() > 1 {
        2
    } else {
        1
    };
    if columns == 1 {
        for group in groups {
            group_card(ui, state, group, edits);
            ui.add_space(14.0);
        }
        return;
    }
    // Greedy split by row count keeps the two columns close in height.
    let mut split: [Vec<&friendly::Group>; 2] = [Vec::new(), Vec::new()];
    let mut heights = [0usize; 2];
    for group in groups {
        let col = if heights[0] <= heights[1] { 0 } else { 1 };
        heights[col] += group.names.len() + 1;
        split[col].push(group);
    }
    ui.columns(2, |cols| {
        for (ui, groups) in cols.iter_mut().zip(&split) {
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                for group in groups {
                    group_card(ui, state, group, edits);
                    ui.add_space(14.0);
                }
            });
        }
    });
}

fn group_card(ui: &mut Ui, state: &AppState, group: &friendly::Group, edits: &mut Vec<Edit>) {
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        card_title(ui, group.title);
        rows(ui, state, group.names, edits);
    });
}

fn rows(ui: &mut Ui, state: &AppState, names: &[&'static str], edits: &mut Vec<Edit>) {
    for (i, name) in names.iter().enumerate() {
        if i > 0 {
            let y = ui.cursor().top() + 1.0;
            let x = ui.max_rect().x_range();
            ui.painter()
                .hline(x, y, Stroke::new(1.0, BORDER.gamma_multiply(0.8)));
            ui.add_space(10.0);
        }
        setting_row(ui, state, name, edits);
        ui.add_space(6.0);
    }
}

fn setting_row(ui: &mut Ui, state: &AppState, name: &'static str, edits: &mut Vec<Edit>) {
    let (Some(row), Some(entry)) = (friendly::row(name), state.catalog.get(name)) else {
        return;
    };
    let value = state.current_value(name).unwrap_or_default();
    let preset = state.preset_value(name).unwrap_or_default();
    let changed = state.is_changed(name);
    let total = ui.available_width();
    let control_width = (total * 0.5).clamp(220.0, 400.0);
    let left_width = total - control_width - 16.0;
    let top = ui.cursor().top();
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(vec2(left_width, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(left_width);
            ui.spacing_mut().item_spacing.y = 3.0;
            ui.horizontal(|ui| {
                let label = RichText::new(row.label).size(15.0).strong();
                ui.label(if changed {
                    label.color(ACCENT)
                } else {
                    label.color(TEXT)
                });
                if changed {
                    let was = friendly::display(row.control, &preset);
                    if ui
                        .add(
                            egui::Button::new(RichText::new("Reset").small().color(ACCENT))
                                .fill(ACCENT.gamma_multiply(0.14))
                                .corner_radius(CornerRadius::same(255))
                                .min_size(vec2(0.0, 20.0)),
                        )
                        .on_hover_text(format!("Back to your preset: {was}"))
                        .clicked()
                    {
                        edits.push(Edit::Reset(vec![name]));
                    }
                }
            });
            ui.label(RichText::new(row.help).small().color(WEAK));
            if changed {
                let was = friendly::display(row.control, &preset);
                ui.label(RichText::new(format!("Preset: {was}")).small().color(WEAK));
            }
        });
        ui.add_space(16.0 - ui.spacing().item_spacing.x);
        ui.allocate_ui_with_layout(
            vec2(control_width, 30.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.set_width(control_width);
                if let Some(v) = control(ui, row.control, entry, &value, &preset) {
                    edits.push(Edit::Set(name, v));
                }
            },
        );
    });
    if changed {
        let bottom = ui.cursor().top();
        let x = ui.max_rect().left() - 10.0;
        ui.painter().rect_filled(
            Rect::from_x_y_ranges(x..=x + 3.0, top..=bottom - 2.0),
            CornerRadius::same(2),
            ACCENT,
        );
    }
}

fn control(
    ui: &mut Ui,
    control: Control,
    entry: &CatalogEntry,
    value: &str,
    preset: &str,
) -> Option<String> {
    let integer = matches!(entry.kind, Kind::Int);
    match control {
        Control::Toggle { invert } => {
            let on = friendly::toggle_on(control, value);
            let clicked = switch(ui, on).clicked();
            ui.label(RichText::new(if on { "On" } else { "Off" }).color(if on {
                TEXT
            } else {
                WEAK
            }));
            clicked.then(|| bool_text(on == invert, Some(value)))
        }
        Control::Levels(levels) => {
            let current = friendly::level_index(levels, value);
            let marked = friendly::level_index(levels, preset);
            segmented(ui, levels, current, marked).map(|i| fmt_num(levels[i].0, integer))
        }
        Control::Slider { .. } => {
            let [lo, hi] = entry.range.unwrap_or([0.0, 1.0]);
            let mut v: f64 = value.trim().parse().unwrap_or(lo);
            let readout = 118.0;
            ui.spacing_mut().slider_width = ui.available_width() - readout - 8.0;
            let before = v;
            let mut slider = egui::Slider::new(&mut v, lo..=hi).show_value(false);
            if integer {
                slider = slider.integer();
            }
            let response = ui.add(slider);
            let edited = (response.changed() && v != before).then(|| {
                fmt_num(
                    friendly::snap(v, [lo, hi], entry.step.unwrap_or(0.0)),
                    integer,
                )
            });
            if let Ok(p) = preset.trim().parse::<f64>()
                && (lo..=hi).contains(&p)
            {
                let rect = response.rect;
                let r = rect.height() / 2.5;
                let x = egui::lerp(
                    rect.left() + r..=rect.right() - r,
                    ((p - lo) / (hi - lo)) as f32,
                );
                ui.painter().line_segment(
                    [
                        egui::pos2(x, rect.top() + 2.0),
                        egui::pos2(x, rect.top() + 7.0),
                    ],
                    Stroke::new(2.0, WEAK),
                );
            }
            let shown = edited.clone().unwrap_or_else(|| value.to_string());
            ui.allocate_ui_with_layout(
                vec2(readout, 24.0),
                Layout::right_to_left(Align::Center),
                |ui| {
                    ui.label(RichText::new(friendly::display(control, &shown)).strong());
                },
            );
            edited
        }
    }
}

fn switch(ui: &mut Ui, on: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(44.0, 24.0), Sense::click());
    let t = ui.ctx().animate_bool_responsive(response.id, on);
    let fill = if on {
        ACCENT
    } else {
        ui.visuals().widgets.inactive.bg_fill
    };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(255), fill);
    let x = egui::lerp(rect.left() + 12.0..=rect.right() - 12.0, t);
    painter.circle_filled(
        egui::pos2(x, rect.center().y),
        8.5,
        if on { ON_ACCENT } else { TEXT },
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Returns the clicked level; `marked` gets a dot, showing what the preset uses.
fn segmented(
    ui: &mut Ui,
    levels: &[(f64, &str)],
    current: Option<usize>,
    marked: Option<usize>,
) -> Option<usize> {
    let gap = 2.0;
    let n = levels.len() as f32;
    let width = (ui.available_width() - gap * (n - 1.0)) / n;
    let mut picked = None;
    ui.spacing_mut().item_spacing.x = gap;
    for (i, (_, label)) in levels.iter().enumerate() {
        let (rect, response) = ui.allocate_exact_size(vec2(width, 30.0), Sense::click());
        let selected = current == Some(i);
        let r = 6;
        let corner = CornerRadius {
            nw: if i == 0 { r } else { 2 },
            sw: if i == 0 { r } else { 2 },
            ne: if i + 1 == levels.len() { r } else { 2 },
            se: if i + 1 == levels.len() { r } else { 2 },
        };
        let w = &ui.visuals().widgets;
        let fill = if selected {
            ACCENT
        } else if response.hovered() {
            w.hovered.weak_bg_fill
        } else {
            w.inactive.weak_bg_fill
        };
        let painter = ui.painter();
        painter.rect_filled(rect, corner, fill);
        let color = if selected {
            ON_ACCENT
        } else {
            TEXT.gamma_multiply(0.85)
        };
        let font = FontId::proportional(13.5);
        painter.text(rect.center(), Align2::CENTER_CENTER, *label, font, color);
        if marked == Some(i) && !selected {
            painter.circle_filled(rect.center_top() + vec2(0.0, 5.0), 2.0, WEAK);
        }
        let mut response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        if marked == Some(i) {
            response = response.on_hover_text("What your preset uses");
        }
        if response.clicked() && !selected {
            picked = Some(i);
        }
    }
    picked
}

fn apply_bar(ui: &mut Ui, state: &mut AppState) {
    let changes = state
        .preview
        .as_ref()
        .map(|p| p.live.len() + p.queued_cheat.len() + p.restart.len() + p.video_changes.len())
        .map_err(Clone::clone);
    let hud_only = state.preview.as_ref().is_ok_and(|p| !p.is_empty());
    let ready = matches!(changes, Ok(n) if n > 0) || hud_only;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            match &changes {
                Ok(0) if !hud_only => ui.label(RichText::new("Everything is applied").size(16.0).strong()),
                Ok(0) => ui.label(RichText::new("HUD changes ready").size(16.0).strong()),
                Ok(1) => ui.label(RichText::new("1 change ready").size(16.0).strong().color(ACCENT)),
                Ok(n) => ui.label(
                    RichText::new(format!("{n} changes ready"))
                        .size(16.0)
                        .strong()
                        .color(ACCENT),
                ),
                Err(raw) => ui
                    .colored_label(BAD, human_error(raw))
                    .on_hover_text(raw.as_str()),
            };
            let key = &state.settings.bind_key;
            let when = match state.timing() {
                Timing::Nothing => match &state.pending_restart {
                    Some(p) => format!("Restart Deadlock to load {} saved changes.", p.names.len()),
                    None => "Pick a preset or change a setting, then Apply.".into(),
                },
                Timing::NextLaunch => "Takes effect next time you start Deadlock.".into(),
                Timing::Instant => format!("Takes effect right away: press {key} in game."),
                Timing::Mixed { now, later } => format!(
                    "{now} take effect right away (press {key} in game), {later} next time you start Deadlock."
                ),
            };
            ui.label(RichText::new(when).color(WEAK));
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if accent_button(ui, ready, "Apply").clicked() {
                apply(ui.ctx(), state);
            }
            if state.is_dirty()
                && ui
                    .add(egui::Button::new("Discard").min_size(vec2(96.0, 40.0)))
                    .on_hover_text("Throw away changes you haven't applied")
                    .clicked()
            {
                state.revert_all();
            }
            ui.add_space(12.0);
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add_space(24.0);
                status_line(ui, &state.status);
            });
        });
    });
}

fn safety(ui: &mut Ui, state: &mut AppState) {
    let wide = ui.available_width() >= 900.0;
    let undo = |ui: &mut Ui, state: &mut AppState| {
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            card_title(ui, "Undo and restore");
            ui.label(
                RichText::new(
                    "Your original files were backed up before DeadTune changed anything.",
                )
                .color(WEAK),
            );
            ui.add_space(6.0);
            if ui
                .add(egui::Button::new("Undo last change").min_size(vec2(200.0, 34.0)))
                .clicked()
            {
                state.status = Some(match state.undo_last() {
                    Ok(()) => Status::Info(
                        "Undone. The game files are back to before your last Apply.".into(),
                    ),
                    Err(e) => Status::Info(e),
                });
            }
            ui.label(
                RichText::new("Click again to step further back.")
                    .small()
                    .color(WEAK),
            );
            ui.add_space(6.0);
            if ui
                .add(egui::Button::new("Restore original game files").min_size(vec2(200.0, 34.0)))
                .clicked()
            {
                state.status = Some(match state.restore_original_files() {
                    Ok(()) => Status::Info(
                        "The game files are back to how they were before DeadTune.".into(),
                    ),
                    Err(e) => Status::Info(e),
                });
            }
            ui.label(
                RichText::new("Puts the game exactly back to how it was before DeadTune.")
                    .small()
                    .color(WEAK),
            );
        });
    };
    let ranked = |ui: &mut Ui, state: &mut AppState| {
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            card_title(ui, "Ranked-safe mode");
            let on = state.settings.source == TargetSource::RankedSafe;
            let clicked = ui
                .horizontal(|ui| {
                    let clicked = switch(ui, on).clicked();
                    ui.label(RichText::new(if on { "On" } else { "Off" }).strong());
                    clicked
                })
                .inner;
            ui.label(
                RichText::new(
                    "Puts the game's own performance settings back so matchmaking never complains. \
                     Your video settings stay. Turn it off to go back to your settings.",
                )
                .color(WEAK),
            );
            if clicked {
                state.status = Some(match state.toggle_ranked_safe() {
                    Ok(_) if on => Status::Info(
                        "Your settings are back. Takes effect next time you start Deadlock.".into(),
                    ),
                    Ok(_) => Status::Info(
                        "Ranked-safe mode is on. Takes effect next time you start Deadlock.".into(),
                    ),
                    Err(e) => Status::Error(e),
                });
            }
        });
    };
    if wide {
        ui.columns(2, |cols| {
            let top_left = Layout::top_down(Align::Min);
            cols[0].with_layout(top_left, |ui| undo(ui, state));
            cols[1].with_layout(top_left, |ui| ranked(ui, state));
        });
    } else {
        undo(ui, state);
        ui.add_space(14.0);
        ranked(ui, state);
    }
    ui.add_space(14.0);
    if state.settings.bridge == BridgeKind::ExecFile {
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            bind_helper(ui, state);
        });
        ui.add_space(14.0);
    }
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        check_setup(ui, state, true);
    });
    ui.add_space(10.0);
    ui.label(
        RichText::new("DeadTune is free software (GPL-3.0). Presets by their authors, credited in Advanced view > Settings.")
            .small()
            .color(WEAK),
    );
    ui.horizontal(|ui| {
        if ui.button("Open advanced view").clicked() {
            state.settings.view = View::Advanced;
        }
    });
}

fn bind_helper(ui: &mut Ui, state: &mut AppState) {
    card_title(ui, "Instant changes (one-time setup)");
    let key = state.settings.bind_key.clone();
    if state.settings.bind_helper_dismissed {
        ui.label(format!(
            "Done. After you Apply, press {key} in game to load changes like the FPS limit."
        ));
        if ui.small_button("Show the steps again").clicked() {
            state.settings.bind_helper_dismissed = false;
        }
        return;
    }
    ui.label(
        RichText::new(
            "Some settings, like the FPS limit, can change while you play. Do this once:",
        )
        .color(WEAK),
    );
    ui.label("1. In Deadlock, open the console (F7) and paste this line:");
    let line = ExecFileBridge::bind_hint(&key);
    ui.horizontal(|ui| {
        egui::Frame::new()
            .fill(RAIL)
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::symmetric(10, 6))
            .show(ui, |ui| ui.monospace(&line));
        if ui.button("Copy").clicked() {
            ui.ctx().copy_text(line.clone());
            state.status = Some(Status::Info(
                "Copied. Paste it into the Deadlock console.".into(),
            ));
        }
    });
    ui.label(format!(
        "2. After changing a setting here, press {key} in game to load it."
    ));
    if ui.button("Done, hide this").clicked() {
        state.settings.bind_helper_dismissed = true;
    }
}

/// `plain` hides the technical detail behind a tooltip.
pub fn check_setup(ui: &mut Ui, state: &mut AppState, plain: bool) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Check setup")
                .text_style(egui::TextStyle::Heading)
                .strong(),
        );
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
