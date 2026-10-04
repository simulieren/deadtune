//! Drawing code for the tabs. Reads `AppState`, collects clicks, then calls its transitions.

use std::path::{Path, PathBuf};
use std::time::Instant;

use dt_core::backup::FileKind;
use dt_core::bridge::execfile::ExecFileBridge;
use dt_core::bridge::netcon::NetconBridge;
use dt_core::catalog::{ApplyClass, CatalogEntry, Impact, Kind};
use dt_core::launch;
use dt_core::preset::{self, PresetId};
use dt_core::profile::{BaseRef, ConVarEdits, Profile, builtin_suggestions};
use dt_core::video;
use eframe::egui::{self, Color32, RichText};

use crate::live::BridgeKind;
use crate::profiles;
use crate::relaunch::Relaunch;
use crate::settings::{Settings, TargetSource};
use crate::state::{AppState, Scope, Setting, Status, bool_text, fmt_num, parse_bool};

const ROW_HEIGHT: f32 = 24.0;
const NAME_WIDTH: f32 = 290.0;
const CONTROL_WIDTH: f32 = 230.0;

enum Edit {
    Set(String),
    Comment,
    Revert,
    Favourite,
}

fn apply_edit(state: &mut AppState, name: &str, edit: Edit) {
    let result = match edit {
        Edit::Set(v) => state.set_convar(name, v),
        Edit::Comment => state.comment_convar(name),
        Edit::Revert => {
            state.revert_convar(name);
            Ok(())
        }
        Edit::Favourite => {
            state.toggle_favourite(name);
            Ok(())
        }
    };
    if let Err(e) = result {
        state.status = Some(Status::Error(e.to_string()));
    }
}

pub fn run_apply(ctx: &egui::Context, state: &mut AppState) -> bool {
    match state.apply() {
        Ok(applied) => {
            let r = &applied.report;
            let mut msg = format!(
                "applied: gameinfo {}, video {}, {} pushed live{}",
                if r.wrote_gameinfo {
                    "written"
                } else {
                    "unchanged"
                },
                if r.wrote_video {
                    "written"
                } else {
                    "unchanged"
                },
                r.pushed_live,
                if r.needs_restart {
                    ", restart needed"
                } else {
                    ""
                },
            );
            if let Some(text) = applied.copy {
                ctx.copy_text(text);
                msg.push_str("; live commands copied to the clipboard");
            }
            if let Some(w) = applied.warning {
                msg.push_str(&format!("; {w}"));
            }
            state.status = Some(Status::Info(msg));
            true
        }
        Err(e) => {
            state.status = Some(Status::Error(format!("apply failed: {e}")));
            false
        }
    }
}

pub fn run_apply_relaunch(ctx: &egui::Context, state: &mut AppState) {
    let has_changes = state.preview.as_ref().is_ok_and(|p| !p.is_empty());
    if has_changes && !run_apply(ctx, state) {
        return;
    }
    if let Err(e) = launch::kill_game() {
        state.status = Some(Status::Error(e.to_string()));
        return;
    }
    state.relaunch = Relaunch::WaitingExit {
        since: Instant::now(),
    };
}

pub fn open_folder(path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(any(windows, target_os = "macos")))]
    let program = "xdg-open";
    std::process::Command::new(program)
        .arg(path)
        .spawn()
        .map(|_| ())
}

fn class_badge(ui: &mut egui::Ui, class: ApplyClass) {
    let (text, color, hint) = match class {
        ApplyClass::Live => (
            "live",
            Color32::from_rgb(90, 190, 110),
            "Console-settable anywhere",
        ),
        ApplyClass::LiveCheat => (
            "cheat",
            Color32::from_rgb(230, 160, 60),
            "Console-settable in hideout/sandbox",
        ),
        ApplyClass::Restart => ("restart", Color32::GRAY, "Read from gameinfo.gi at launch"),
    };
    ui.label(RichText::new(text).small().color(color))
        .on_hover_text(hint);
}

fn impact_badge(ui: &mut egui::Ui, impact: Impact) {
    let text = match impact {
        Impact::High => "high",
        Impact::Medium => "med",
        Impact::Low => "low",
        Impact::Unknown => return,
    };
    ui.label(RichText::new(text).small().weak())
        .on_hover_text("Performance impact");
}

fn entry_hover(ui: &mut egui::Ui, name: &str, entry: Option<&CatalogEntry>) {
    ui.strong(name);
    let Some(entry) = entry else {
        ui.label("Not in the catalog; treated as restart-only.");
        return;
    };
    if !entry.notes.is_empty() {
        ui.label(&entry.notes);
    }
    ui.label(format!("Category: {}", entry.category));
    if let Some(d) = &entry.default {
        ui.label(format!("Engine default: {d}"));
    }
    if !entry.presets.is_empty() {
        ui.separator();
        for (id, value) in &entry.presets {
            ui.monospace(format!("{:<22} {value}", preset::info(*id).label));
        }
    }
}

/// The value control for one convar; `None` when untouched.
fn control(
    ui: &mut egui::Ui,
    name: &str,
    entry: Option<&CatalogEntry>,
    value: Option<&str>,
) -> Option<String> {
    let kind = entry.map_or(&Kind::String, |e| &e.kind);
    let text = value.unwrap_or("");
    match kind {
        Kind::Bool => {
            let mut on = parse_bool(text);
            ui.checkbox(&mut on, "")
                .changed()
                .then(|| bool_text(on, value))
        }
        Kind::Int | Kind::Float => {
            let integer = matches!(kind, Kind::Int);
            let mut v: f64 = text.trim().parse().unwrap_or(0.0);
            let step = entry
                .and_then(|e| e.step)
                .unwrap_or(if integer { 1.0 } else { 0.0 });
            let response = match entry.and_then(|e| e.range) {
                Some([lo, hi]) if lo < hi => {
                    ui.spacing_mut().slider_width = CONTROL_WIDTH - 70.0;
                    let mut slider = egui::Slider::new(&mut v, lo..=hi);
                    if step > 0.0 {
                        slider = slider.step_by(step);
                    }
                    ui.add(slider)
                }
                _ => {
                    let speed = if step > 0.0 {
                        step
                    } else if integer {
                        1.0
                    } else {
                        0.01
                    };
                    ui.add(egui::DragValue::new(&mut v).speed(speed))
                }
            };
            response.changed().then(|| fmt_num(v, integer))
        }
        Kind::Enum { options } => {
            let mut picked = None;
            egui::ComboBox::from_id_salt(("enum", name))
                .selected_text(text)
                .width(CONTROL_WIDTH - 20.0)
                .show_ui(ui, |ui| {
                    for option in options {
                        if ui.selectable_label(option == text, option).clicked() {
                            picked = Some(option.clone());
                        }
                    }
                });
            picked
        }
        Kind::String => {
            let mut edit = text.to_string();
            ui.add(egui::TextEdit::singleline(&mut edit).desired_width(CONTROL_WIDTH - 20.0))
                .changed()
                .then_some(edit)
        }
    }
}

fn convar_row(ui: &mut egui::Ui, state: &AppState, name: &str) -> Option<Edit> {
    let entry = state.catalog.get(name);
    let denied = state.catalog.is_denied(name);
    let setting = state.setting(name);
    let value = state.current_value(name);
    let favourite = state.settings.favourites.contains(name);
    let mut edit = None;
    ui.horizontal(|ui| {
        ui.set_height(ROW_HEIGHT);
        let star = if favourite { "★" } else { "☆" };
        if ui
            .small_button(star)
            .on_hover_text("Favourite (compact mode, phone remote)")
            .clicked()
        {
            edit = Some(Edit::Favourite);
        }
        let label = match setting {
            Setting::Override(_) | Setting::CommentedOut => {
                RichText::new(name).monospace().strong()
            }
            _ => RichText::new(name).monospace(),
        };
        ui.add_sized([NAME_WIDTH, ROW_HEIGHT], egui::Label::new(label).truncate())
            .on_hover_ui(|ui| entry_hover(ui, name, entry));
        ui.allocate_ui_with_layout(
            egui::vec2(CONTROL_WIDTH, ROW_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_width(CONTROL_WIDTH);
                ui.add_enabled_ui(!denied, |ui| {
                    if let Some(v) = control(ui, name, entry, value.as_deref()) {
                        edit = Some(Edit::Set(v));
                    }
                });
            },
        );
        class_badge(ui, state.catalog.apply_class(name));
        if let Some(e) = entry {
            impact_badge(ui, e.impact);
        }
        if denied {
            ui.label(RichText::new("denylist").small().color(Color32::LIGHT_RED))
                .on_hover_text("Competitive-information convar; DeadTune never changes it.");
            return;
        }
        match &setting {
            Setting::Override(_) => {
                ui.label(RichText::new("edited").small().color(Color32::LIGHT_BLUE));
            }
            Setting::CommentedOut => {
                ui.label(
                    RichText::new("commented")
                        .small()
                        .color(Color32::LIGHT_BLUE),
                );
            }
            Setting::Base(_) => {
                ui.label(RichText::new("preset").small().weak());
            }
            Setting::EngineDefault => {
                ui.label(RichText::new("default").small().weak());
            }
        }
        if matches!(setting, Setting::Override(_) | Setting::CommentedOut) {
            if ui.small_button("revert").clicked() {
                edit = Some(Edit::Revert);
            }
        } else if matches!(setting, Setting::Base(_))
            && ui
                .small_button("//")
                .on_hover_text("Comment out: use the engine default")
                .clicked()
        {
            edit = Some(Edit::Comment);
        }
    });
    edit
}

pub fn compact_row(ui: &mut egui::Ui, state: &mut AppState, name: &str) {
    let entry = state.catalog.get(name);
    let value = state.current_value(name);
    let denied = state.catalog.is_denied(name);
    let mut edit = None;
    ui.label(RichText::new(name).monospace().small())
        .on_hover_ui(|ui| entry_hover(ui, name, entry));
    ui.horizontal(|ui| {
        ui.add_enabled_ui(!denied, |ui| {
            if let Some(v) = control(ui, name, entry, value.as_deref()) {
                edit = Some(Edit::Set(v));
            }
        });
        class_badge(ui, state.catalog.apply_class(name));
    });
    if let Some(edit) = edit {
        apply_edit(state, name, edit);
    }
}

pub fn categories(ui: &mut egui::Ui, state: &mut AppState) {
    ui.add(
        egui::TextEdit::singleline(&mut state.ui.search)
            .hint_text("Search name, category, notes")
            .desired_width(f32::INFINITY),
    );
    ui.separator();
    egui::ScrollArea::vertical().show(ui, |ui| {
        let scope = &mut state.ui.scope;
        ui.selectable_value(scope, Scope::All, "All");
        ui.selectable_value(scope, Scope::Changed, "Changed in profile");
        ui.selectable_value(scope, Scope::Favourites, "Favourites");
        ui.separator();
        for cat in state.catalog.categories() {
            ui.selectable_value(scope, Scope::Category(cat.to_string()), cat);
        }
    });
}

pub fn convar_list(ui: &mut egui::Ui, state: &mut AppState) {
    let rows = state.visible_rows();
    ui.horizontal(|ui| {
        ui.weak(format!("{} convars", rows.len()));
        if state.settings.source == TargetSource::RankedSafe {
            ui.colored_label(
                Color32::GOLD,
                "Ranked-safe mode: edits are saved to the profile but not applied.",
            );
        }
    });
    let mut edits = Vec::new();
    egui::ScrollArea::vertical().auto_shrink(false).show_rows(
        ui,
        ROW_HEIGHT + 4.0,
        rows.len(),
        |ui, range| {
            for name in &rows[range] {
                if let Some(edit) = convar_row(ui, state, name) {
                    edits.push((name.clone(), edit));
                }
            }
        },
    );
    for (name, edit) in edits {
        apply_edit(state, &name, edit);
    }
}

pub fn diff_view(ui: &mut egui::Ui, diff: &str) {
    for line in diff.lines() {
        let color = if line.starts_with("+++") || line.starts_with("---") {
            Color32::GRAY
        } else if line.starts_with('+') {
            Color32::from_rgb(110, 200, 120)
        } else if line.starts_with('-') {
            Color32::from_rgb(230, 110, 110)
        } else if line.starts_with("@@") {
            Color32::LIGHT_BLUE
        } else {
            ui.visuals().weak_text_color()
        };
        ui.label(RichText::new(line).monospace().size(11.0).color(color));
    }
}

fn name_list(ui: &mut egui::Ui, title: &str, color: Color32, names: &[String]) {
    if names.is_empty() {
        return;
    }
    egui::CollapsingHeader::new(RichText::new(format!("{title} ({})", names.len())).color(color))
        .id_salt(title)
        .default_open(names.len() <= 12)
        .show(ui, |ui| {
            for n in names {
                ui.monospace(n);
            }
        });
}

enum PendingAction {
    Apply,
    ApplyRelaunch,
    Push,
    RevertAll,
}

pub fn pending(ui: &mut egui::Ui, state: &mut AppState) {
    let mut action = None;
    ui.heading("Pending changes");
    if state.settings.source == TargetSource::RankedSafe {
        ui.colored_label(Color32::GOLD, "Target: stock ConVars block (ranked-safe).");
    }
    match &state.preview {
        Err(e) => {
            ui.colored_label(Color32::LIGHT_RED, format!("Cannot build plan: {e}"));
        }
        Ok(plan) => {
            if plan.is_empty() {
                ui.weak("Files match the profile.");
            }
            let live: Vec<String> = plan
                .live
                .iter()
                .map(|c| format!("{} = {}", c.name, c.value))
                .collect();
            name_list(ui, "Live now", Color32::from_rgb(90, 190, 110), &live);
            name_list(
                ui,
                "Queued cheat (next launch or tick hideout)",
                Color32::from_rgb(230, 160, 60),
                &plan.queued_cheat,
            );
            name_list(ui, "Restart required", Color32::GRAY, &plan.restart);
            name_list(ui, "Denied (denylist)", Color32::LIGHT_RED, &plan.denied);
            let video: Vec<String> = plan
                .video_changes
                .iter()
                .map(|(k, v)| format!("{k} = {v}"))
                .collect();
            name_list(ui, "video.txt", Color32::LIGHT_BLUE, &video);
            if state.ctx.game_running && !plan.is_empty() {
                ui.colored_label(
                    Color32::GOLD,
                    "Game is running: file changes take effect next launch.",
                );
            }
            ui.horizontal_wrapped(|ui| {
                let can_apply = !plan.is_empty();
                if ui
                    .add_enabled(
                        can_apply,
                        egui::Button::new(RichText::new("Apply").strong()),
                    )
                    .clicked()
                {
                    action = Some(PendingAction::Apply);
                }
                if ui
                    .add_enabled(
                        !state.relaunch.is_active(),
                        egui::Button::new("Apply + relaunch"),
                    )
                    .on_hover_text("Write files, close the game, start it again through Steam")
                    .clicked()
                {
                    action = Some(PendingAction::ApplyRelaunch);
                }
                if ui
                    .button("Push live")
                    .on_hover_text("Send live-class changes through the active bridge")
                    .clicked()
                {
                    action = Some(PendingAction::Push);
                }
                if ui
                    .add_enabled(state.is_dirty(), egui::Button::new("Revert all"))
                    .clicked()
                {
                    action = Some(PendingAction::RevertAll);
                }
            });
            if state.settings.bridge == BridgeKind::ExecFile {
                ui.weak(format!(
                    "Exec-file bridge: bind once in the console: {}",
                    ExecFileBridge::bind_hint(&state.settings.bind_key)
                ));
            }
            if !state.preview_diff.is_empty() {
                ui.separator();
                ui.strong("Diff");
                egui::ScrollArea::both()
                    .auto_shrink(false)
                    .show(ui, |ui| diff_view(ui, &state.preview_diff));
            }
        }
    }
    let ctx = ui.ctx().clone();
    match action {
        Some(PendingAction::Apply) => {
            run_apply(&ctx, state);
        }
        Some(PendingAction::ApplyRelaunch) => run_apply_relaunch(&ctx, state),
        Some(PendingAction::Push) => {
            let result = state.push_now();
            crate::app::report_push(&ctx, state, result);
        }
        Some(PendingAction::RevertAll) => state.revert_all(),
        None => {}
    }
}

pub fn video(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("video.txt");
    ui.weak("Normal menu settings; kept in ranked-safe mode. Applied at next launch.");
    let Some(live) = state.live.video.clone() else {
        ui.colored_label(
            Color32::LIGHT_RED,
            format!("{} not found", state.paths.video.display()),
        );
        return;
    };
    let live_settings = match video::read_settings(&live) {
        Ok(s) => s,
        Err(e) => {
            ui.colored_label(Color32::LIGHT_RED, e.to_string());
            return;
        }
    };
    let base_settings = state
        .base
        .as_ref()
        .ok()
        .and_then(|b| b.texts.video.as_deref())
        .and_then(|v| video::read_settings(v).ok())
        .unwrap_or_default();
    let mut edits = Vec::new();
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            egui::Grid::new("video")
                .striped(true)
                .num_columns(4)
                .show(ui, |ui| {
                    ui.strong("Setting");
                    ui.strong("In file");
                    ui.strong("Profile");
                    ui.end_row();
                    for (key, live_value) in &live_settings {
                        let edited = state.profile.video.get(key);
                        let base = base_settings.iter().find(|(k, _)| k == key).map(|(_, v)| v);
                        let mut value = edited.or(base).unwrap_or(live_value).clone();
                        ui.monospace(key);
                        ui.monospace(live_value);
                        if ui
                            .add(egui::TextEdit::singleline(&mut value).desired_width(140.0))
                            .changed()
                        {
                            edits.push((key.clone(), Some(value)));
                        }
                        if edited.is_some() && ui.small_button("revert").clicked() {
                            edits.push((key.clone(), None));
                        }
                        ui.end_row();
                    }
                });
        });
    for (key, value) in edits {
        match value {
            Some(v) => state.set_video(&key, v),
            None => state.revert_video(&key),
        }
    }
}

fn base_label(base: &BaseRef) -> String {
    match base {
        BaseRef::Preset(id) => {
            let info = preset::info(*id);
            format!("{} ({})", info.label, info.author)
        }
        BaseRef::File(path) => format!("file: {}", path.display()),
    }
}

pub fn base_picker(ui: &mut egui::Ui, state: &mut AppState) {
    let mut picked = None;
    ui.label("Base:");
    egui::ComboBox::from_id_salt("base")
        .selected_text(base_label(&state.profile.base))
        .width(260.0)
        .show_ui(ui, |ui| {
            for info in preset::all() {
                let base = BaseRef::Preset(info.id);
                let text = format!("{} ({})", info.label, info.author);
                if ui
                    .selectable_label(state.profile.base == base, text)
                    .clicked()
                {
                    picked = Some(base);
                }
            }
        });
    if let Err(e) = &state.base {
        ui.colored_label(Color32::LIGHT_RED, "!").on_hover_text(e);
    }
    if let Some(base) = picked {
        state.set_base(base);
    }
}

pub fn profile_picker(ui: &mut egui::Ui, state: &mut AppState) {
    let saved = profiles::list(&state.profiles_dir());
    let mut picked = None;
    ui.label("Profile:");
    let name = if state.is_dirty() {
        format!("{} *", state.profile.name)
    } else {
        state.profile.name.clone()
    };
    egui::ComboBox::from_id_salt("profile")
        .selected_text(name)
        .width(180.0)
        .show_ui(ui, |ui| {
            for p in &saved {
                if ui
                    .selectable_label(p.name == state.profile.name, &p.name)
                    .clicked()
                {
                    picked = Some((p.clone(), true));
                }
            }
            ui.separator();
            for p in builtin_suggestions() {
                if ui
                    .selectable_label(false, format!("New from \"{}\"", p.name))
                    .clicked()
                {
                    picked = Some((p, false));
                }
            }
        });
    if ui
        .add_enabled(state.is_dirty(), egui::Button::new("Save"))
        .clicked()
    {
        save_profile(state);
    }
    if let Some((profile, on_disk)) = picked {
        state.switch_profile(profile, on_disk);
    }
}

fn save_profile(state: &mut AppState) {
    state.status = Some(match state.save_profile() {
        Ok(path) => Status::Info(format!("saved {}", path.display())),
        Err(e) => Status::Error(format!("save failed: {e}")),
    });
}

pub fn profiles(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Profile");
    ui.horizontal(|ui| {
        ui.label("Name");
        ui.text_edit_singleline(&mut state.profile.name);
        if ui
            .add_enabled(state.is_dirty(), egui::Button::new("Save"))
            .clicked()
        {
            save_profile(state);
        }
    });
    let edits = &state.profile.convars;
    ui.weak(format!(
        "{} set, {} commented out, {} video settings. Base: {}",
        edits.set.len(),
        edits.comment.len(),
        state.profile.video.len(),
        base_label(&state.profile.base)
    ));
    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut state.ui.new_profile_name);
        let name = state.ui.new_profile_name.trim().to_string();
        if ui
            .add_enabled(!name.is_empty(), egui::Button::new("New empty profile"))
            .clicked()
        {
            let profile = Profile {
                name,
                base: state.profile.base.clone(),
                base_rev: None,
                convars: ConVarEdits::default(),
                video: Default::default(),
            };
            state.switch_profile(profile, false);
            state.ui.new_profile_name.clear();
        }
    });

    ui.separator();
    ui.heading("Saved profiles");
    let saved = profiles::list(&state.profiles_dir());
    if saved.is_empty() {
        ui.weak(format!("None yet in {}", state.profiles_dir().display()));
    }
    let mut picked = None;
    for p in &saved {
        ui.horizontal(|ui| {
            ui.label(&p.name);
            ui.weak(base_label(&p.base));
            if ui.small_button("Load").clicked() {
                picked = Some((p.clone(), true));
            }
        });
    }
    ui.heading("Suggestions");
    for p in builtin_suggestions() {
        ui.horizontal(|ui| {
            ui.label(&p.name);
            ui.weak(format!(
                "{}, {} overrides",
                base_label(&p.base),
                p.convars.set.len()
            ));
            if ui.small_button("Use").clicked() {
                picked = Some((p.clone(), false));
            }
        });
    }
    if let Some((p, on_disk)) = picked {
        state.switch_profile(p, on_disk);
    }

    ui.separator();
    ui.heading("overrides.gi (OptimizationLock updater format)");
    ui.horizontal(|ui| {
        ui.label("Path");
        ui.add(egui::TextEdit::singleline(&mut state.ui.overrides_path).desired_width(420.0));
    });
    ui.horizontal(|ui| {
        let path = PathBuf::from(state.ui.overrides_path.trim());
        if ui.button("Import into profile").clicked() {
            let result = std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|t| state.import_overrides(&t));
            state.status = Some(match result {
                Ok((n, denied)) if denied.is_empty() => {
                    Status::Info(format!("imported {n} overrides"))
                }
                Ok((n, denied)) => Status::Info(format!(
                    "imported {n}; skipped denylisted {}",
                    denied.join(", ")
                )),
                Err(e) => Status::Error(format!("import: {e}")),
            });
        }
        if ui.button("Export profile").clicked() {
            state.status = Some(
                match dt_core::backup::atomic_write(&path, state.export_overrides().as_bytes()) {
                    Ok(()) => Status::Info(format!("exported {}", path.display())),
                    Err(e) => Status::Error(format!("export: {e}")),
                },
            );
        }
    });

    ui.separator();
    ui.heading("Auto profile by power source");
    ui.label(format!("Now: {:?}", dt_core::power::power_source()));
    let names: Vec<String> = saved.iter().map(|p| p.name.clone()).collect();
    let power = &mut state.settings.power;
    ui.checkbox(
        &mut power.enabled,
        "Switch and apply on DeadTune start while the game is closed",
    );
    for (label, slot) in [
        ("Plugged in", &mut power.ac),
        ("Battery", &mut power.battery),
    ] {
        ui.horizontal(|ui| {
            ui.label(label);
            egui::ComboBox::from_id_salt(label)
                .selected_text(slot.clone().unwrap_or_else(|| "(none)".into()))
                .show_ui(ui, |ui| {
                    ui.selectable_value(slot, None, "(none)");
                    for n in &names {
                        ui.selectable_value(slot, Some(n.clone()), n);
                    }
                });
        });
    }
}

pub fn backups(ui: &mut egui::Ui, state: &mut AppState) {
    let mut restore = None;
    for (kind, title) in [
        (FileKind::GameInfo, "gameinfo.gi"),
        (FileKind::Video, "video.txt"),
    ] {
        ui.heading(title);
        match state.store.original(kind) {
            Some(original) => {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "Original snapshot {} (sha {})",
                        original.created.format("%Y-%m-%d %H:%M"),
                        &original.sha256[..12.min(original.sha256.len())]
                    ));
                    if ui.button("Restore original").clicked() {
                        restore = Some(original.clone());
                    }
                });
            }
            None => {
                ui.weak("No original snapshot yet; it is taken on the first apply.");
            }
        }
        let list = state.backups(kind);
        if list.is_empty() {
            ui.weak("No backups.");
        }
        for entry in list {
            ui.horizontal(|ui| {
                ui.monospace(entry.created.format("%Y-%m-%d %H:%M:%S").to_string());
                ui.weak(&entry.sha256[..12.min(entry.sha256.len())]);
                if ui.small_button("Restore").clicked() {
                    restore = Some(entry.clone());
                }
            });
        }
        ui.separator();
    }
    if ui.button("Open backups folder").clicked() {
        let _ = open_folder(&state.store.root);
    }
    if let Some(entry) = restore {
        state.status = Some(match state.restore(&entry) {
            Ok(()) => Status::Info(format!("restored {}", entry.path.display())),
            Err(e) => Status::Error(format!("restore failed: {e}")),
        });
    }
}

fn metrics_text(m: &dt_core::bench::Metrics) -> String {
    format!(
        "avg {:.1} fps | 1% low {:.1} | 0.1% low {:.1} | {} frames, {:.1}s",
        m.avg_fps, m.low_1_fps, m.low_01_fps, m.frames, m.duration_s
    )
}

pub fn bench(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Import capture");
    ui.weak("PresentMon (Windows) or MangoHud (Linux/Deck) CSV. Record the same route under each config.");
    ui.horizontal(|ui| {
        ui.label("CSV path");
        ui.add(egui::TextEdit::singleline(&mut state.bench.csv_path).desired_width(420.0));
    });
    ui.horizontal(|ui| {
        ui.label("Label");
        ui.add(
            egui::TextEdit::singleline(&mut state.bench.label)
                .hint_text("e.g. farz 6000")
                .desired_width(200.0),
        );
        if ui.button("Import").clicked() {
            state.status = Some(match state.bench.import() {
                Ok(run) => Status::Info(format!("{}: {}", run.label, metrics_text(&run.metrics))),
                Err(e) => Status::Error(format!("import: {e}")),
            });
        }
    });
    let mut save = None;
    for (i, run) in state.bench.runs.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.strong(&run.label);
            ui.label(metrics_text(&run.metrics));
            if ui.small_button("Save to history").clicked() {
                save = Some(i);
            }
        });
    }
    if !state.bench.runs.is_empty() {
        crate::chart::frametimes(ui, &state.bench.runs);
    }
    if let Some(i) = save {
        let dir = crate::bench::history_dir(&state.data_dir);
        let profile = state.profile.name.clone();
        if let Err(e) = state.bench.save_run(i, &profile, &dir) {
            state.status = Some(Status::Error(format!("save: {e}")));
        }
    }

    ui.separator();
    ui.heading(format!("History for {}", state.profile.name));
    if state.bench.history.is_empty() {
        ui.weak("No saved runs for this profile.");
    }
    egui::Grid::new("history").striped(true).show(ui, |ui| {
        ui.strong("A");
        ui.strong("B");
        ui.strong("When");
        ui.strong("Label");
        ui.strong("Metrics");
        ui.strong("Screenshots");
        ui.end_row();
        let b = &mut state.bench;
        for (i, record) in b.history.iter().enumerate() {
            ui.radio_value(&mut b.compare[0], Some(i), "");
            ui.radio_value(&mut b.compare[1], Some(i), "");
            ui.monospace(record.recorded.format("%Y-%m-%d %H:%M").to_string());
            ui.label(&record.label);
            ui.label(metrics_text(&record.metrics));
            let names: Vec<String> = record
                .screenshots
                .iter()
                .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                .collect();
            ui.label(names.join(", "));
            ui.end_row();
        }
    });
    if let Some(d) = state.bench.delta() {
        ui.label(
            RichText::new(format!(
                "B vs A: avg {:+.1}% | 1% low {:+.1}% | 0.1% low {:+.1}%",
                d.avg_fps_pct, d.low_1_pct, d.low_01_pct
            ))
            .strong(),
        );
    }

    ui.separator();
    ui.heading("Screenshots (F12 in game)");
    let dirs = state
        .paths
        .steam_root
        .as_deref()
        .map(dt_core::locate::screenshot_dirs)
        .unwrap_or_default();
    ui.horizontal(|ui| {
        if ui.button("Refresh").clicked() {
            state.reload_bench();
        }
        for dir in &dirs {
            if ui
                .button("Open folder")
                .on_hover_text(dir.display().to_string())
                .clicked()
            {
                let _ = open_folder(dir);
            }
        }
    });
    if dirs.is_empty() {
        ui.weak("Steam screenshot folder unknown (game is not in the main Steam library).");
    }
    ui.weak("Tick screenshots to attach them to the next saved run.");
    let b = &mut state.bench;
    for shot in &b.screenshots {
        let mut on = b.picked.contains(shot);
        let name = shot
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if ui.checkbox(&mut on, name).changed() {
            if on {
                b.picked.insert(shot.clone());
            } else {
                b.picked.remove(shot);
            }
        }
    }
}

pub fn launch(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Launch");
    let mut args = state.settings.launch.args.join(" ");
    ui.horizontal(|ui| {
        ui.label("Launch options");
        if ui
            .add(egui::TextEdit::singleline(&mut args).desired_width(420.0))
            .changed()
        {
            state.settings.launch.args = args.split_whitespace().map(str::to_string).collect();
        }
    });
    ui.weak(launch::steam_url(&state.settings.launch));
    ui.horizontal(|ui| {
        if ui.button("Launch game").clicked() {
            if let Err(e) = launch::launch(&state.settings.launch) {
                state.status = Some(Status::Error(e.to_string()));
            }
        }
        if ui
            .add_enabled(
                !state.relaunch.is_active(),
                egui::Button::new("Apply + relaunch"),
            )
            .clicked()
        {
            run_apply_relaunch(ui.ctx(), state);
        }
    });
    if let Some(pending) = &state.pending_restart {
        ui.label(format!(
            "Waiting for a restart to load: {}",
            pending.names.join(", ")
        ));
    }

    ui.separator();
    ui.heading("Live bridge");
    ui.horizontal(|ui| {
        for kind in BridgeKind::ALL {
            ui.selectable_value(&mut state.settings.bridge, kind, kind.label());
        }
    });
    match state.settings.bridge {
        BridgeKind::ExecFile => {
            ui.horizontal(|ui| {
                ui.label("Key");
                ui.add(
                    egui::TextEdit::singleline(&mut state.settings.bind_key).desired_width(60.0),
                );
                let hint = ExecFileBridge::bind_hint(&state.settings.bind_key);
                ui.monospace(&hint);
                if ui.small_button("Copy").clicked() {
                    ui.ctx().copy_text(hint);
                }
            });
            ui.weak("Bind once in the console (F7). DeadTune writes cfg/deadtune_live.cfg; press the key in game to load it.");
        }
        BridgeKind::Netcon => {
            ui.horizontal(|ui| {
                ui.label("Port");
                ui.add(egui::DragValue::new(&mut state.settings.netcon_port).range(1..=65535));
                if ui.button("Probe").clicked() {
                    let ok = NetconBridge::probe(state.settings.netcon_port);
                    state.status = Some(if ok {
                        Status::Info(format!(
                            "netcon answers on port {}",
                            state.settings.netcon_port
                        ))
                    } else {
                        Status::Error(format!(
                            "nothing listening on 127.0.0.1:{}",
                            state.settings.netcon_port
                        ))
                    });
                }
            });
            ui.weak(format!(
                "Needs -netconport {} in the launch options. Works on Linux; on Windows it may need -tools, which blocks matchmaking.",
                state.settings.netcon_port
            ));
        }
        BridgeKind::Clipboard => {
            ui.weak("Push copies the commands; paste them into the console (F7).");
        }
    }
    #[cfg(feature = "remote")]
    {
        ui.separator();
        crate::remote::ui(ui, state);
    }
}

pub fn settings(ui: &mut egui::Ui, state: &mut AppState, reopen: &mut Option<Settings>) {
    ui.heading("Game");
    ui.label(format!(
        "Deadlock folder: {}",
        state.paths.game_root.display()
    ));
    ui.label(format!("gameinfo.gi: {}", state.paths.gameinfo.display()));
    let mut dir = state
        .settings
        .game_dir
        .as_ref()
        .map(|d| d.display().to_string())
        .unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label("Override folder");
        ui.add(
            egui::TextEdit::singleline(&mut dir)
                .hint_text("auto-detected")
                .desired_width(420.0),
        );
        if ui.button("Use").clicked() {
            let mut settings = state.settings.clone();
            settings.game_dir = (!dir.trim().is_empty()).then(|| PathBuf::from(dir.trim()));
            *reopen = Some(settings);
        }
    });
    ui.horizontal(|ui| {
        ui.label(format!("Data folder: {}", state.data_dir.display()));
        if ui.small_button("Open").clicked() {
            let _ = open_folder(&state.data_dir);
        }
    });

    ui.separator();
    ui.heading("Credits");
    ui.label("DeadTune is free software under the GNU GPL-3.0. Presets belong to their authors and are credited per preset:");
    for info in preset::all() {
        ui.horizontal(|ui| {
            ui.label(RichText::new(info.label).strong());
            ui.label(format!("by {}", info.author));
            if info.id != PresetId::Vanilla {
                ui.weak(info.upstream_gameinfo);
            }
        });
    }
    ui.weak("Override model ported from OptimizationLock's gameinfo_updater.py (GPL-3.0).");
}
