//! Drawing code for the tabs. Reads `AppState`, collects clicks, then calls its transitions.

use std::path::{Path, PathBuf};
use std::time::Instant;

use dt_core::backup::FileKind;
use dt_core::bridge::execfile::ExecFileBridge;
use dt_core::bridge::netcon::NetconBridge;
use dt_core::catalog::{CatalogEntry, Kind};
use dt_core::launch;
use dt_core::preset::{self, PresetId};
use dt_core::profile::{BaseRef, ConVarEdits, Profile, builtin_suggestions};
use dt_core::video;
use eframe::egui::{self, RichText};

use crate::advanced::RESTART;
use crate::live::BridgeKind;
use crate::profiles;
use crate::relaunch::Relaunch;
use crate::settings::Settings;
use crate::state::{AppState, Status, bool_text, fmt_num, parse_bool};
use crate::theme::{ACCENT, BAD, GOOD, TEXT, WEAK};
use crate::widgets;

const CONTROL_WIDTH: f32 = 230.0;

pub enum Edit {
    Set(String),
    Comment,
    Revert,
    Favourite,
}

pub fn apply_edit(state: &mut AppState, name: &str, edit: Edit) {
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

/// Opens a folder or URL with the system's default handler.
pub fn open_external(target: impl AsRef<std::ffi::OsStr>) -> std::io::Result<()> {
    #[cfg(windows)]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(any(windows, target_os = "macos")))]
    let program = "xdg-open";
    std::process::Command::new(program)
        .arg(target)
        .spawn()
        .map(|_| ())
}

pub fn entry_hover(ui: &mut egui::Ui, name: &str, entry: Option<&CatalogEntry>) {
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
pub fn control(
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
                    slider = if integer {
                        slider.integer()
                    } else {
                        slider.max_decimals(3)
                    };
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

/// Coloured unified diff; file header lines show only the file name, the full path on hover.
pub fn diff_view(ui: &mut egui::Ui, diff: &str) {
    for line in diff.lines() {
        let header = line.starts_with("+++ ") || line.starts_with("--- ");
        let color = if header {
            WEAK
        } else if line.starts_with('+') {
            GOOD
        } else if line.starts_with('-') {
            BAD
        } else if line.starts_with("@@") {
            RESTART
        } else {
            WEAK.gamma_multiply(0.8)
        };
        let shown = match header {
            true => {
                let (mark, path) = line.split_at(4);
                let file = Path::new(path.trim())
                    .file_name()
                    .map_or(path.into(), |f| f.to_string_lossy());
                format!("{mark}{file}")
            }
            false => line.to_string(),
        };
        let label = ui.add(
            egui::Label::new(RichText::new(shown).monospace().size(11.0).color(color)).extend(),
        );
        if header {
            label.on_hover_text(line);
        }
    }
}

pub fn video(ui: &mut egui::Ui, state: &mut AppState) {
    widgets::page_title(
        ui,
        "Video",
        "video.txt: the game's own menu settings. Kept in ranked-safe mode; applied at next launch.",
    );
    let Some(live) = state.live.video.clone() else {
        ui.colored_label(BAD, format!("{} not found", state.paths.video.display()));
        return;
    };
    let live_settings = match video::read_settings(&live) {
        Ok(s) => s,
        Err(e) => {
            ui.colored_label(BAD, e.to_string());
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
    let changed = live_settings
        .iter()
        .filter(|(k, _)| state.profile.video.contains_key(k))
        .count();
    let mut count = format!("{} settings", live_settings.len());
    if changed > 0 {
        count.push_str(&format!(" · {changed} changed"));
    }
    widgets::hint(ui, &count);
    ui.add_space(4.0);
    let mut edits = Vec::new();
    widgets::card(ui, |ui| {
        egui::Grid::new("video")
            .striped(true)
            .num_columns(4)
            .spacing([16.0, 4.0])
            .min_row_height(24.0)
            .show(ui, |ui| {
                for head in ["Setting", "In file", "Profile", ""] {
                    widgets::caption(ui, head);
                }
                ui.end_row();
                for (key, live_value) in &live_settings {
                    let edited = state.profile.video.get(key);
                    let base = base_settings.iter().find(|(k, _)| k == key).map(|(_, v)| v);
                    let mut value = edited.or(base).unwrap_or(live_value).clone();
                    let color = if edited.is_some() { ACCENT } else { TEXT };
                    let name = key.strip_prefix("setting.").unwrap_or(key);
                    ui.label(RichText::new(name).monospace().size(12.0).color(color))
                        .on_hover_text(key);
                    ui.label(RichText::new(live_value).monospace().size(12.0).color(WEAK));
                    if ui
                        .add_sized([140.0, 20.0], egui::TextEdit::singleline(&mut value))
                        .changed()
                    {
                        edits.push((key.clone(), Some(value)));
                    }
                    if edited.is_some() {
                        if widgets::reset_pill(ui)
                            .on_hover_text("Back to the preset's value")
                            .clicked()
                        {
                            edits.push((key.clone(), None));
                        }
                    } else {
                        ui.label("");
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

pub fn base_label(base: &BaseRef) -> String {
    match base {
        BaseRef::Preset(id) => {
            let info = preset::info(*id);
            format!("{} ({})", info.label, info.author)
        }
        BaseRef::File(path) => format!("file: {}", path.display()),
    }
}

pub fn save_profile(state: &mut AppState) {
    state.status = Some(match state.save_profile() {
        Ok(path) => Status::Info(format!("saved {}", path.display())),
        Err(e) => Status::Error(format!("save failed: {e}")),
    });
}

/// A two-column label / control grid, the form layout of the tab pages.
fn form(ui: &mut egui::Ui, id: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([14.0, 6.0])
        .show(ui, add);
}

fn form_label(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(WEAK));
}

/// One list row: strong title, weak detail, and a trailing small button.
fn list_row(ui: &mut egui::Ui, title: &str, detail: &str, button: &str) -> bool {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).strong());
        ui.label(RichText::new(detail).small().color(WEAK));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.small_button(button).clicked()
        })
        .inner
    })
    .inner
}

pub fn profiles(ui: &mut egui::Ui, state: &mut AppState) {
    widgets::page_title(
        ui,
        "Profiles",
        "A profile is a base preset plus your edits. Save several and switch between them.",
    );
    let saved = profiles::list(&state.profiles_dir());
    let mut picked = None;
    widgets::section(ui, "Current profile", |ui| {
        form(ui, "profile_form", |ui| {
            form_label(ui, "Name");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut state.profile.name).desired_width(220.0));
                if ui
                    .add_enabled(state.is_dirty(), egui::Button::new("Save"))
                    .clicked()
                {
                    save_profile(state);
                }
            });
            ui.end_row();
            form_label(ui, "Contents");
            let edits = &state.profile.convars;
            ui.label(format!(
                "{} set, {} commented out, {} video settings on {}",
                edits.set.len(),
                edits.comment.len(),
                state.profile.video.len(),
                base_label(&state.profile.base)
            ));
            ui.end_row();
            form_label(ui, "New profile");
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut state.ui.new_profile_name)
                        .hint_text("Name")
                        .desired_width(220.0),
                );
                let name = state.ui.new_profile_name.trim().to_string();
                if ui
                    .add_enabled(!name.is_empty(), egui::Button::new("Create empty"))
                    .on_hover_text("Same base preset, no edits")
                    .clicked()
                {
                    let profile = Profile {
                        name,
                        base: state.profile.base.clone(),
                        base_rev: None,
                        convars: ConVarEdits::default(),
                        video: Default::default(),
                        hud: Default::default(),
                        addons: Default::default(),
                    };
                    picked = Some((profile, false));
                    state.ui.new_profile_name.clear();
                }
            });
            ui.end_row();
        });
    });
    widgets::section(ui, "Saved profiles", |ui| {
        if saved.is_empty() {
            widgets::hint(
                ui,
                &format!("None yet in {}", state.profiles_dir().display()),
            );
        }
        for p in &saved {
            let title = if p.name == state.profile.name {
                format!("{} (current)", p.name)
            } else {
                p.name.clone()
            };
            if list_row(ui, &title, &base_label(&p.base), "Load") {
                picked = Some((p.clone(), true));
            }
        }
    });
    widgets::section(ui, "Suggestions", |ui| {
        for p in builtin_suggestions() {
            let detail = format!("{}, {} overrides", base_label(&p.base), p.convars.set.len());
            if list_row(ui, &p.name, &detail, "Use") {
                picked = Some((p.clone(), false));
            }
        }
    });
    if let Some((p, on_disk)) = picked {
        state.switch_profile(p, on_disk);
    }
    widgets::section(ui, "overrides.gi (OptimizationLock updater format)", |ui| {
        form(ui, "overrides_form", |ui| {
            form_label(ui, "Path");
            ui.add(
                egui::TextEdit::singleline(&mut state.ui.overrides_path)
                    .hint_text("/path/to/overrides.gi")
                    .desired_width(420.0),
            );
            ui.end_row();
            ui.label("");
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
                        match dt_core::backup::atomic_write(
                            &path,
                            state.export_overrides().as_bytes(),
                        ) {
                            Ok(()) => Status::Info(format!("exported {}", path.display())),
                            Err(e) => Status::Error(format!("export: {e}")),
                        },
                    );
                }
            });
            ui.end_row();
        });
    });
    widgets::section(ui, "Auto profile by power source", |ui| {
        let names: Vec<String> = saved.iter().map(|p| p.name.clone()).collect();
        let power = &mut state.settings.power;
        ui.checkbox(
            &mut power.enabled,
            "Switch and apply on DeadTune start while the game is closed",
        );
        widgets::hint(
            ui,
            &format!("Power source now: {:?}", dt_core::power::power_source()),
        );
        ui.add_space(4.0);
        form(ui, "power_form", |ui| {
            for (label, slot) in [
                ("Plugged in", &mut power.ac),
                ("Battery", &mut power.battery),
            ] {
                form_label(ui, label);
                egui::ComboBox::from_id_salt(label)
                    .selected_text(slot.clone().unwrap_or_else(|| "(none)".into()))
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(slot, None, "(none)");
                        for n in &names {
                            ui.selectable_value(slot, Some(n.clone()), n);
                        }
                    });
                ui.end_row();
            }
        });
    });
}

fn short_sha(sha: &str) -> &str {
    &sha[..12.min(sha.len())]
}

pub fn backups(ui: &mut egui::Ui, state: &mut AppState) {
    widgets::page_title(
        ui,
        "Backups",
        "Every Apply backs the files up first. The original snapshot is taken before DeadTune's first write.",
    );
    let mut restore = None;
    for (kind, title) in [
        (FileKind::GameInfo, "gameinfo.gi"),
        (FileKind::Video, "video.txt"),
    ] {
        let list = state.backups(kind);
        let count = match list.len() {
            1 => "1 backup".to_string(),
            n => format!("{n} backups"),
        };
        widgets::section(ui, &format!("{title} · {count}"), |ui| {
            match state.store.original(kind) {
                Some(original) => {
                    ui.horizontal(|ui| {
                        widgets::badge(ui, "Original", ACCENT);
                        ui.label(
                            RichText::new(original.created.format("%Y-%m-%d %H:%M").to_string())
                                .monospace()
                                .size(12.0),
                        );
                        ui.label(
                            RichText::new(short_sha(&original.sha256))
                                .monospace()
                                .size(11.0)
                                .color(WEAK),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Restore original").clicked() {
                                restore = Some(original.clone());
                            }
                        });
                    });
                }
                None => widgets::hint(
                    ui,
                    "No original snapshot yet; it is taken on the first apply.",
                ),
            }
            if list.is_empty() {
                widgets::hint(ui, "No backups.");
            }
            for entry in list {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(entry.created.format("%Y-%m-%d %H:%M:%S").to_string())
                            .monospace()
                            .size(12.0),
                    );
                    ui.label(
                        RichText::new(short_sha(&entry.sha256))
                            .monospace()
                            .size(11.0)
                            .color(WEAK),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Restore").clicked() {
                            restore = Some(entry.clone());
                        }
                    });
                });
            }
        });
    }
    if ui.button("Open backups folder").clicked() {
        let _ = open_external(&state.store.root);
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
        "avg {:.1} fps · 1% low {:.1} · 0.1% low {:.1} · {} frames, {:.1}s",
        m.avg_fps, m.low_1_fps, m.low_01_fps, m.frames, m.duration_s
    )
}

pub fn bench(ui: &mut egui::Ui, state: &mut AppState) {
    widgets::page_title(
        ui,
        "Bench",
        "Compare frame times between configs. Record the same route under each one.",
    );
    let mut save = None;
    widgets::section(ui, "Import capture", |ui| {
        widgets::hint(ui, "PresentMon (Windows) or MangoHud (Linux/Deck) CSV.");
        ui.add_space(2.0);
        form(ui, "bench_form", |ui| {
            form_label(ui, "CSV path");
            ui.add(egui::TextEdit::singleline(&mut state.bench.csv_path).desired_width(420.0));
            ui.end_row();
            form_label(ui, "Label");
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut state.bench.label)
                        .hint_text("e.g. farz 6000")
                        .desired_width(200.0),
                );
                if ui.button("Import").clicked() {
                    state.status = Some(match state.bench.import() {
                        Ok(run) => {
                            Status::Info(format!("{}: {}", run.label, metrics_text(&run.metrics)))
                        }
                        Err(e) => Status::Error(format!("import: {e}")),
                    });
                }
            });
            ui.end_row();
        });
        for (i, run) in state.bench.runs.iter().enumerate() {
            if list_row(
                ui,
                &run.label,
                &metrics_text(&run.metrics),
                "Save to history",
            ) {
                save = Some(i);
            }
        }
        if !state.bench.runs.is_empty() {
            crate::chart::frametimes(ui, &state.bench.runs);
        }
    });
    if let Some(i) = save {
        let dir = crate::bench::history_dir(&state.data_dir);
        let profile = state.profile.name.clone();
        if let Err(e) = state.bench.save_run(i, &profile, &dir) {
            state.status = Some(Status::Error(format!("save: {e}")));
        }
    }

    widgets::section(ui, &format!("History for {}", state.profile.name), |ui| {
        if state.bench.history.is_empty() {
            widgets::hint(ui, "No saved runs for this profile.");
            return;
        }
        egui::Grid::new("history")
            .striped(true)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                for head in ["A", "B", "When", "Label", "Metrics", "Screenshots"] {
                    widgets::caption(ui, head);
                }
                ui.end_row();
                let b = &mut state.bench;
                for (i, record) in b.history.iter().enumerate() {
                    ui.radio_value(&mut b.compare[0], Some(i), "");
                    ui.radio_value(&mut b.compare[1], Some(i), "");
                    ui.label(
                        RichText::new(record.recorded.format("%Y-%m-%d %H:%M").to_string())
                            .monospace()
                            .size(12.0),
                    );
                    ui.label(&record.label);
                    ui.label(RichText::new(metrics_text(&record.metrics)).small());
                    let names: Vec<String> = record
                        .screenshots
                        .iter()
                        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                        .collect();
                    ui.label(RichText::new(names.join(", ")).small().color(WEAK));
                    ui.end_row();
                }
            });
        if let Some(d) = state.bench.delta() {
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!(
                    "B vs A: avg {:+.1}% · 1% low {:+.1}% · 0.1% low {:+.1}%",
                    d.avg_fps_pct, d.low_1_pct, d.low_01_pct
                ))
                .strong()
                .color(ACCENT),
            );
        }
    });

    let dirs = state
        .paths
        .steam_root
        .as_deref()
        .map(dt_core::locate::screenshot_dirs)
        .unwrap_or_default();
    widgets::section(ui, "Screenshots (F12 in game)", |ui| {
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
                    let _ = open_external(dir);
                }
            }
        });
        if dirs.is_empty() {
            widgets::hint(
                ui,
                "Steam screenshot folder unknown (game is not in the main Steam library).",
            );
        }
        widgets::hint(ui, "Tick screenshots to attach them to the next saved run.");
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
    });
}

pub fn launch(ui: &mut egui::Ui, state: &mut AppState) {
    widgets::page_title(
        ui,
        "Launch",
        "Start Deadlock through Steam, and choose how live changes reach the running game.",
    );
    widgets::section(ui, "Launch", |ui| {
        form(ui, "launch_form", |ui| {
            form_label(ui, "Launch options");
            let mut args = state.settings.launch.args.join(" ");
            if ui
                .add(
                    egui::TextEdit::singleline(&mut args)
                        .hint_text("-novid -high")
                        .desired_width(420.0),
                )
                .changed()
            {
                state.settings.launch.args = args.split_whitespace().map(str::to_string).collect();
            }
            ui.end_row();
            form_label(ui, "Console window");
            ui.checkbox(&mut state.settings.console_window, "add -console");
            ui.end_row();
            form_label(ui, "Steam URL");
            ui.label(
                RichText::new(launch::steam_url(&state.launch_args()))
                    .monospace()
                    .size(11.5)
                    .color(WEAK),
            )
            .on_hover_text(
                "DeadTune adds +exec deadtune_boot (its boot cfg: key bind, live convars, a marker \
                 for the console log) and -condebug (writes the console log it reads back).",
            );
            ui.end_row();
            form_label(ui, "From Steam");
            ui.horizontal(|ui| {
                let text = dt_core::bridge::boot::steam_launch_options(state.settings.console_window);
                ui.label(RichText::new(&text).monospace().size(11.5));
                if ui
                    .small_button("Copy")
                    .on_hover_text(
                        "Paste into Steam: right-click Deadlock > Properties > General > Launch Options",
                    )
                    .clicked()
                {
                    ui.ctx().copy_text(text);
                }
            });
            ui.end_row();
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button("Launch game").clicked()
                && let Err(e) = state.launch_game()
            {
                state.status = Some(Status::Error(e));
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
            widgets::hint(
                ui,
                &format!(
                    "Waiting for a restart to load: {}",
                    pending.names.join(", ")
                ),
            );
        }
    });

    widgets::section(ui, "Live bridge", |ui| {
        ui.horizontal(|ui| {
            for kind in BridgeKind::ALL {
                ui.selectable_value(&mut state.settings.bridge, kind, kind.label());
            }
        });
        ui.add_space(2.0);
        match state.settings.bridge {
            BridgeKind::ExecFile => {
                ui.horizontal(|ui| {
                    form_label(ui, "Key");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.settings.bind_key)
                            .desired_width(50.0),
                    );
                    let hint = ExecFileBridge::bind_hint(&state.settings.bind_key);
                    ui.label(RichText::new(&hint).monospace().size(12.0));
                    if ui.small_button("Copy").clicked() {
                        ui.ctx().copy_text(hint);
                    }
                });
                widgets::hint(
                    ui,
                    "The boot cfg binds this key when Deadlock starts from DeadTune (or with +exec deadtune_boot). DeadTune writes cfg/deadtune_live.cfg; press the key in game to load it and the console log confirms each convar.",
                );
            }
            BridgeKind::Netcon => {
                ui.horizontal(|ui| {
                    form_label(ui, "Port");
                    ui.add(egui::DragValue::new(&mut state.settings.netcon_port).range(1..=65535));
                    if ui.button("Probe").clicked() {
                        let port = state.settings.netcon_port;
                        state.status = Some(if NetconBridge::probe(port) {
                            Status::Info(format!("netcon answers on port {port}"))
                        } else {
                            Status::Error(format!("nothing listening on 127.0.0.1:{port}"))
                        });
                    }
                });
                widgets::hint(
                    ui,
                    &format!(
                        "Needs -netconport {} in the launch options. Works on Linux; on Windows it may need -tools, which blocks matchmaking.",
                        state.settings.netcon_port
                    ),
                );
            }
            BridgeKind::Clipboard => {
                widgets::hint(
                    ui,
                    "Push copies the commands; paste them into the console (F7).",
                );
            }
        }
    });
    #[cfg(feature = "remote")]
    widgets::card(ui, |ui| crate::remote::ui(ui, state));
}

pub fn settings(ui: &mut egui::Ui, state: &mut AppState, reopen: &mut Option<Settings>) {
    widgets::page_title(
        ui,
        "Settings",
        "Where DeadTune finds the game and keeps its data.",
    );
    widgets::section(ui, "Game", |ui| {
        let mut dir = state
            .settings
            .game_dir
            .as_ref()
            .map(|d| d.display().to_string())
            .unwrap_or_default();
        form(ui, "settings_form", |ui| {
            let path = |ui: &mut egui::Ui, p: &Path| {
                let full = p.display().to_string();
                let chars: Vec<char> = full.chars().collect();
                let shown = match chars.len() {
                    n if n > 64 => format!("...{}", chars[n - 64..].iter().collect::<String>()),
                    _ => full.clone(),
                };
                ui.label(RichText::new(shown).monospace().size(11.5))
                    .on_hover_text(full);
            };
            form_label(ui, "Deadlock folder");
            path(ui, &state.paths.game_root);
            ui.end_row();
            form_label(ui, "gameinfo.gi");
            path(ui, &state.paths.gameinfo);
            ui.end_row();
            form_label(ui, "Override folder");
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut dir)
                        .hint_text("auto-detected")
                        .desired_width(380.0),
                );
                if ui.button("Use").clicked() {
                    let mut settings = state.settings.clone();
                    settings.game_dir = (!dir.trim().is_empty()).then(|| PathBuf::from(dir.trim()));
                    *reopen = Some(settings);
                }
            });
            ui.end_row();
            form_label(ui, "Data folder");
            ui.horizontal(|ui| {
                path(ui, &state.data_dir);
                if ui.small_button("Open").clicked() {
                    let _ = open_external(&state.data_dir);
                }
            });
            ui.end_row();
        });
    });
    widgets::card(ui, |ui| crate::simple::check_setup(ui, state, false));
    widgets::section(ui, "Updates", |ui| {
        crate::update_view::settings(ui, state, false)
    });
    widgets::section(ui, "Credits", |ui| {
        ui.label(
            "DeadTune is free software under the GNU GPL-3.0. Presets belong to their authors:",
        );
        ui.add_space(2.0);
        egui::Grid::new("credits")
            .num_columns(3)
            .spacing([14.0, 4.0])
            .show(ui, |ui| {
                for info in preset::all() {
                    ui.label(RichText::new(info.label).strong());
                    ui.label(RichText::new(format!("by {}", info.author)).color(WEAK));
                    if info.id != PresetId::Vanilla {
                        ui.label(RichText::new(info.source_url()).small().color(WEAK));
                    } else {
                        ui.label("");
                    }
                    ui.end_row();
                }
            });
        ui.add_space(2.0);
        widgets::hint(
            ui,
            "Override model ported from OptimizationLock's gameinfo_updater.py (GPL-3.0).",
        );
        widgets::hint(
            ui,
            "Inter typeface by The Inter Project Authors (rsms.me/inter), SIL Open Font License 1.1.",
        );
    });
}
