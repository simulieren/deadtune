//! Game files page: save copies of the game's interface and settings files, compare two
//! snapshots, read the latest report, open the folders. The work is `crate::snapshots`.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use dt_core::snapshot::spec::DEFAULT_SIZE_CAP;
use dt_core::snapshot::{
    Category, ChangeKind, FileChange, SnapshotDiff, SnapshotInfo, human_bytes, store,
};
use eframe::egui::{self, Align, Layout, RichText, Ui, vec2};

use crate::state::{AppState, Status};
use crate::theme::{self, ACCENT, BAD, GOOD, ON_ACCENT, TEXT, WARN, WEAK};
use crate::views::open_external;
use crate::widgets;

enum Edit {
    Take,
    Cancel,
    Compare(PathBuf, PathBuf),
    Open(PathBuf),
    Copy(String),
    Delete(PathBuf),
    ArmDelete(Option<PathBuf>),
    PickOld(usize),
    PickNew(usize),
}

const CAPS: [(Option<u64>, &str); 5] = [
    (Some(1 << 20), "1 MB"),
    (Some(4 << 20), "4 MB"),
    (Some(DEFAULT_SIZE_CAP), "8 MB"),
    (Some(32 << 20), "32 MB"),
    (None, "No limit"),
];

pub fn page(ui: &mut Ui, state: &mut AppState) {
    let mut edits = Vec::new();
    status_card(ui, state, &mut edits);
    what_to_save(ui, state);
    if let Some(diff) = state.latest_diff.clone() {
        report_card(ui, &diff, &mut edits);
    }
    snapshots_card(ui, state, &mut edits);
    for edit in edits {
        match edit {
            Edit::Take => state.start_snapshot(false),
            Edit::Cancel => {
                if let Some(job) = &state.snapshot_job {
                    job.cancel();
                }
            }
            Edit::Compare(old, new) => state.start_compare(old, new),
            Edit::Open(path) => {
                if let Err(e) = open_external(&path) {
                    state.status = Some(Status::Error(format!("open {}: {e}", path.display())));
                }
            }
            Edit::Copy(text) => {
                ui.ctx().copy_text(text);
                state.status = Some(Status::Info("Summary copied.".into()));
            }
            Edit::Delete(folder) => {
                state.ui.snapshot_delete_armed = None;
                state.status = Some(match state.delete_snapshot(&folder) {
                    Ok(()) => Status::Info("Snapshot deleted.".into()),
                    Err(e) => Status::Error(format!("delete snapshot: {e}")),
                });
            }
            Edit::ArmDelete(folder) => state.ui.snapshot_delete_armed = folder,
            Edit::PickOld(i) => state.ui.snapshot_compare.0 = i,
            Edit::PickNew(i) => state.ui.snapshot_compare.1 = i,
        }
    }
}

fn when(t: DateTime<Utc>) -> String {
    t.with_timezone(&chrono::Local)
        .format("%-d %b %Y %H:%M")
        .to_string()
}

fn big_button(ui: &mut Ui, enabled: bool, text: &str, primary: bool) -> egui::Response {
    let label = RichText::new(text).size(13.5).strong();
    let button = if primary {
        egui::Button::new(label.color(ON_ACCENT)).fill(ACCENT)
    } else {
        egui::Button::new(label)
    };
    ui.add_enabled(enabled, button.min_size(vec2(0.0, 32.0)))
}

fn status_card(ui: &mut Ui, state: &AppState, edits: &mut Vec<Edit>) {
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        let build = state.game_build.as_deref().unwrap_or("unknown");
        ui.label(
            RichText::new(format!("Deadlock build {build}"))
                .size(16.0)
                .family(theme::semibold())
                .color(TEXT),
        );
        let last = state.snapshots.iter().find(|s| s.complete);
        let line = match last {
            Some(s) => format!(
                "Last snapshot: build {}, {} ({} files, {}).",
                s.buildid.as_deref().unwrap_or("unknown"),
                s.taken.map(when).unwrap_or_default(),
                s.files,
                human_bytes(s.bytes)
            ),
            None => "No snapshots yet. Take one now; after the next game update the report shows what changed.".into(),
        };
        ui.label(RichText::new(line).color(WEAK));
        ui.add_space(8.0);
        match &state.snapshot_job {
            Some(job) => {
                let p = &job.progress;
                ui.horizontal(|ui| {
                    let fraction = if p.total == 0 {
                        0.0
                    } else {
                        p.done as f32 / p.total as f32
                    };
                    ui.add(
                        egui::ProgressBar::new(fraction)
                            .desired_width(320.0)
                            .text(format!("{} of {} files", p.done, p.total)),
                    );
                    if ui.button("Cancel").clicked() {
                        edits.push(Edit::Cancel);
                    }
                });
                ui.label(
                    RichText::new(format!("{} saved. {}", human_bytes(p.bytes), p.path))
                        .small()
                        .color(WEAK),
                );
            }
            None => {
                let complete: Vec<&SnapshotInfo> = state.snapshots.iter().filter(|s| s.complete).collect();
                ui.horizontal_wrapped(|ui| {
                    if big_button(ui, true, "Take snapshot", true)
                        .on_hover_text("Reads the selected files out of the game and writes them, decoded, into DeadTune's data folder. The game is not changed.")
                        .clicked()
                    {
                        edits.push(Edit::Take);
                    }
                    let (old, new) = state.ui.snapshot_compare;
                    let pair = (state.snapshots.get(old), state.snapshots.get(new));
                    let can_compare = matches!(pair, (Some(a), Some(b)) if a.complete && b.complete && a.folder != b.folder);
                    if big_button(ui, can_compare, "Compare with previous", false)
                        .on_hover_text("Writes a report into the newer snapshot: what changed, and which DeadTune feature reads it.")
                        .clicked()
                        && let (Some(a), Some(b)) = pair
                    {
                        edits.push(Edit::Compare(a.folder.clone(), b.folder.clone()));
                    }
                    if big_button(ui, true, "Open snapshot folder", false).clicked() {
                        let dir = state.snapshot_dir();
                        let _ = std::fs::create_dir_all(&dir);
                        edits.push(Edit::Open(dir));
                    }
                    if big_button(ui, state.latest_diff.is_some(), "Copy summary", false)
                        .on_hover_text("The latest report without the per-file diffs, as text.")
                        .clicked()
                        && let Some(d) = &state.latest_diff
                    {
                        edits.push(Edit::Copy(d.summary()));
                    }
                });
                if complete.len() >= 2 {
                    compare_pickers(ui, state, edits);
                }
            }
        }
        match &state.last_snapshot {
            Some(Ok(done)) => {
                ui.label(RichText::new(done.summary()).color(GOOD));
            }
            Some(Err(e)) if e != "cancelled" => {
                ui.colored_label(BAD, format!("The last snapshot failed: {e}"));
            }
            _ => {}
        }
    });
}

fn compare_pickers(ui: &mut Ui, state: &AppState, edits: &mut Vec<Edit>) {
    let name = |i: usize| {
        state
            .snapshots
            .get(i)
            .map(|s| {
                format!(
                    "build {} ({})",
                    s.buildid.as_deref().unwrap_or("unknown"),
                    s.taken.map(when).unwrap_or_else(|| "incomplete".into())
                )
            })
            .unwrap_or_default()
    };
    let (old, new) = state.ui.snapshot_compare;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Compare").color(WEAK));
        for (which, picked) in [("older", old), ("newer", new)] {
            egui::ComboBox::from_id_salt(("snapshot_pick", which))
                .selected_text(name(picked))
                .width(230.0)
                .show_ui(ui, |ui| {
                    for (i, s) in state.snapshots.iter().enumerate() {
                        if s.complete && ui.selectable_label(i == picked, name(i)).clicked() {
                            edits.push(if which == "older" {
                                Edit::PickOld(i)
                            } else {
                                Edit::PickNew(i)
                            });
                        }
                    }
                });
            if which == "older" {
                ui.label(RichText::new("with").color(WEAK));
            }
        }
    });
}

fn what_to_save(ui: &mut Ui, state: &mut AppState) {
    let selection = state.settings.snapshots.selection.clone();
    let inventory = state.snapshot_inventory().map(|inv| {
        let per: Vec<(usize, u64)> = Category::ALL.iter().map(|c| inv.totals(*c)).collect();
        (per, inv.selected_totals(&selection))
    });
    let settings = &mut state.settings.snapshots;
    widgets::card(ui, |ui| {
        widgets::caption(ui, "What to save");
        if let Err(e) = &inventory {
            ui.colored_label(BAD, format!("Couldn't read the game's files: {e}"));
        }
        for (i, cat) in Category::ALL.into_iter().enumerate() {
            ui.horizontal(|ui| {
                let mut on = settings.selection.categories.contains(&cat);
                if ui
                    .checkbox(&mut on, cat.label())
                    .on_hover_text(cat.blurb())
                    .changed()
                {
                    if on {
                        settings.selection.categories.insert(cat);
                    } else {
                        settings.selection.categories.remove(&cat);
                    }
                }
                if let Ok((per, _)) = &inventory {
                    let (files, bytes) = per[i];
                    ui.label(
                        RichText::new(format!("{}, {}", plural(files, "file"), human_bytes(bytes)))
                            .small()
                            .color(WEAK),
                    );
                }
            });
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut settings.selection.decode, "Decode to readable text")
                .on_hover_text("Stylesheets, layouts and scripts are stored as the game compiled them and as plain CSS, XML and JavaScript next to them.");
            ui.add_space(12.0);
            ui.label(RichText::new("Skip files bigger than").color(WEAK));
            let current = CAPS
                .iter()
                .find(|(cap, _)| *cap == settings.selection.size_cap)
                .map_or("Custom", |(_, label)| *label);
            egui::ComboBox::from_id_salt("snapshot_cap")
                .selected_text(current)
                .width(100.0)
                .show_ui(ui, |ui| {
                    for (cap, label) in CAPS {
                        ui.selectable_value(&mut settings.selection.size_cap, cap, label);
                    }
                });
        });
        if let Ok((_, (files, bytes))) = &inventory {
            ui.label(
                RichText::new(format!(
                    "Selected: {}, {} to store. Bigger files are still listed, so the report sees them change.",
                    plural(*files, "file"),
                    human_bytes(*bytes)
                ))
                .small()
                .color(WEAK),
            );
        }
        ui.add_space(6.0);
        ui.checkbox(
            &mut settings.auto,
            "Snapshot automatically after game updates",
        )
        .on_hover_text("When Steam installs a new build, DeadTune takes a snapshot in the background and compares it with the previous one.");
    });
}

fn impact_row(ui: &mut Ui, f: &FileChange, diff: &SnapshotDiff, edits: &mut Vec<Edit>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(f.features.join(", ")).strong().color(WARN));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            file_buttons(ui, f, diff, edits);
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(format!("{} {}", f.path, f.kind.label())).color(TEXT),
                    )
                    .truncate(),
                )
                .on_hover_text(&f.path);
            });
        });
    });
}

fn plural(n: usize, what: &str) -> String {
    format!("{n} {what}{}", if n == 1 { "" } else { "s" })
}

fn file_buttons(ui: &mut Ui, f: &FileChange, diff: &SnapshotDiff, edits: &mut Vec<Edit>) {
    let (folder, text) = match f.kind {
        ChangeKind::Removed => (&diff.old_folder, &f.old_text),
        _ => (&diff.new_folder, &f.new_text),
    };
    let raw = folder.join(store::RAW).join(&f.path);
    if ui
        .add_enabled(raw.is_file(), egui::Button::new("Open").small())
        .on_hover_text(raw.display().to_string())
        .clicked()
    {
        edits.push(Edit::Open(raw));
    }
    let text_path = text.as_ref().map(|t| folder.join(store::TEXT).join(t));
    if ui
        .add_enabled(
            text_path.as_ref().is_some_and(|p| p.is_file()),
            egui::Button::new("Text").small(),
        )
        .clicked()
        && let Some(p) = text_path
    {
        edits.push(Edit::Open(p));
    }
}

fn file_list(ui: &mut Ui, diff: &SnapshotDiff, kind: ChangeKind, edits: &mut Vec<Edit>) {
    let files: Vec<&FileChange> = diff.files_of(kind).collect();
    if files.is_empty() {
        return;
    }
    let title = match kind {
        ChangeKind::Changed => "Changed files",
        ChangeKind::Added => "Added files",
        ChangeKind::Removed => "Removed files",
    };
    egui::CollapsingHeader::new(format!("{title} ({})", files.len()))
        .id_salt(("snapshot_files", kind.label()))
        .show(ui, |ui| {
            for f in files {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&f.path).monospace().size(11.5));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        file_buttons(ui, f, diff, edits);
                    });
                });
            }
        });
}

fn report_card(ui: &mut Ui, diff: &SnapshotDiff, edits: &mut Vec<Edit>) {
    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            widgets::caption(ui, "Latest comparison");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Open report").clicked() {
                    edits.push(Edit::Open(diff.report_path()));
                }
                if ui.button("Copy summary").clicked() {
                    edits.push(Edit::Copy(diff.summary()));
                }
            });
        });
        ui.label(
            RichText::new(diff.title())
                .size(15.0)
                .family(theme::semibold())
                .color(TEXT),
        );
        ui.label(
            RichText::new(format!(
                "{} in the files DeadTune watches. Compared {}.",
                diff.total().phrase(),
                when(diff.compared)
            ))
            .color(WEAK),
        );
        ui.add_space(6.0);
        let impact: Vec<&FileChange> = diff.impact().collect();
        if impact.is_empty() {
            ui.label(RichText::new("Nothing a DeadTune feature depends on changed.").color(GOOD));
        } else {
            ui.label(
                RichText::new(format!(
                    "{} file{} a DeadTune feature reads changed. Check these first:",
                    impact.len(),
                    if impact.len() == 1 { "" } else { "s" }
                ))
                .color(WARN),
            );
            for f in impact {
                impact_row(ui, f, diff, edits);
            }
        }
        ui.add_space(6.0);
        let by_category: Vec<String> = diff
            .categories
            .iter()
            .filter(|(_, c)| c.total() > 0)
            .map(|(cat, c)| format!("{}: {}.", cat.label(), c.phrase()))
            .collect();
        if !by_category.is_empty() {
            ui.label(RichText::new(by_category.join(" ")).color(TEXT));
        }
        let elsewhere = diff.elsewhere_total();
        if elsewhere.total() > 0 {
            let parts: Vec<String> = diff
                .elsewhere
                .iter()
                .map(|(dir, c)| format!("{dir} {}", c.phrase()))
                .collect();
            ui.label(
                RichText::new(format!(
                    "Elsewhere in the game: {} ({}).",
                    elsewhere.phrase(),
                    parts.join("; ")
                ))
                .small()
                .color(WEAK),
            );
        }
        ui.add_space(4.0);
        for kind in [ChangeKind::Changed, ChangeKind::Added, ChangeKind::Removed] {
            file_list(ui, diff, kind, edits);
        }
    });
}

fn snapshots_card(ui: &mut Ui, state: &AppState, edits: &mut Vec<Edit>) {
    widgets::card(ui, |ui| {
        widgets::caption(ui, "Snapshots");
        if state.snapshots.is_empty() {
            widgets::hint(
                ui,
                "Each snapshot is a folder in DeadTune's data folder: the files as the game has them under raw, the readable copies under text, and the report from the comparison.",
            );
            return;
        }
        for s in &state.snapshots {
            snapshot_row(ui, s, state.ui.snapshot_delete_armed.as_deref(), edits);
        }
    });
}

fn snapshot_row(ui: &mut Ui, s: &SnapshotInfo, armed: Option<&Path>, edits: &mut Vec<Edit>) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!(
                "Build {}",
                s.buildid.as_deref().unwrap_or("unknown")
            ))
            .strong()
            .color(TEXT),
        );
        let detail = if s.complete {
            format!(
                "{}, {} files, {}",
                s.taken.map(when).unwrap_or_default(),
                s.files,
                human_bytes(s.bytes)
            )
        } else {
            "incomplete (cancelled or interrupted); the next snapshot of this build fills it in"
                .into()
        };
        ui.label(RichText::new(detail).color(WEAK));
        if let Some(report) = s.reports.first() {
            ui.label(RichText::new(report).small().color(WEAK));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let is_armed = armed == Some(s.folder.as_path());
            let label = if is_armed { "Really delete?" } else { "Delete" };
            let button =
                egui::Button::new(RichText::new(label).color(if is_armed { BAD } else { TEXT }));
            if ui.add(button).clicked() {
                edits.push(if is_armed {
                    Edit::Delete(s.folder.clone())
                } else {
                    Edit::ArmDelete(Some(s.folder.clone()))
                });
            }
            if ui.button("Open").clicked() {
                edits.push(Edit::Open(s.folder.clone()));
            }
        });
    });
}
