//! The "Save all images" strip shared by the UI images and Game files pages: the buttons
//! and zip choice, the progress bar with Cancel while it runs, and the summary with
//! "Open folder" when it ends. The job is `crate::images_export`.

use std::path::PathBuf;
use std::sync::Arc;

use dt_core::snapshot::human_bytes;
use eframe::egui::{self, RichText, Ui};

use crate::images::ImageSource;
use crate::state::{AppState, Status};
use crate::theme::{BAD, GOOD, TEXT, WARN, WEAK};
use crate::views::open_external;

enum Action {
    Start(Option<String>),
    Cancel,
    Open(PathBuf),
}

/// `source` is the page's library when it holds one; `folder` offers "Save this folder".
pub fn strip(
    ui: &mut Ui,
    state: &mut AppState,
    source: Option<&Arc<ImageSource>>,
    folder: Option<&str>,
) {
    if state.images.export_all.job.is_none() {
        ui.horizontal_wrapped(|ui| controls(ui, state, source, folder));
    }
    progress(ui, state, source);
}

/// The start buttons and the zip choice.
pub fn controls(
    ui: &mut Ui,
    state: &mut AppState,
    source: Option<&Arc<ImageSource>>,
    folder: Option<&str>,
) {
    let mut action = None;
    if ui
        .button(RichText::new("Save all images").size(12.5))
        .on_hover_text(
            "Saves every image as PNG (vector icons also as SVG) into a folder in DeadTune's data folder, at the game's own paths, with a manifest.json listing them. The game is not changed.",
        )
        .clicked()
    {
        action = Some(Action::Start(None));
    }
    if let Some(folder) = folder
        && ui
            .button(RichText::new("Save this folder").size(12.5))
            .on_hover_text(format!("Only the images under {folder}"))
            .clicked()
    {
        action = Some(Action::Start(Some(folder.to_string())));
    }
    ui.checkbox(&mut state.images.export_all.zip, "Zip it")
        .on_hover_text("Also writes one .zip of the folder next to it, to send to someone.");
    act(state, action, source);
}

/// The running job's progress bar, or how the last one ended; nothing before the first.
pub fn progress(ui: &mut Ui, state: &mut AppState, source: Option<&Arc<ImageSource>>) {
    let mut action = None;
    if let Some(job) = &state.images.export_all.job {
        let p = &job.progress;
        ui.horizontal(|ui| {
            let fraction = if p.total == 0 {
                0.0
            } else {
                p.done as f32 / p.total as f32
            };
            let what = match &job.folder {
                Some(folder) => format!("Saving {folder}: {} of {} images", p.done, p.total),
                None => format!("Saving {} of {} images", p.done, p.total),
            };
            ui.label(RichText::new(what).size(12.5).color(TEXT));
            ui.add(egui::ProgressBar::new(fraction).desired_width(220.0));
            if ui.button("Cancel").clicked() {
                action = Some(Action::Cancel);
            }
            ui.add(
                egui::Label::new(
                    RichText::new(format!("{} saved · {}", human_bytes(p.bytes), p.path))
                        .size(11.0)
                        .color(WEAK),
                )
                .truncate(),
            );
        });
    }
    match &state.images.export_all.last {
        Some(Ok(done)) => {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(done.summary())
                        .size(12.0)
                        .color(if done.manifest.failed > 0 { WARN } else { GOOD }),
                );
                if ui
                    .small_button("Open folder")
                    .on_hover_text(done.folder.display().to_string())
                    .clicked()
                {
                    action = Some(Action::Open(done.folder.clone()));
                }
                if done.manifest.failed > 0 && ui.small_button("Open failures.txt").clicked() {
                    action = Some(Action::Open(done.failures_file()));
                }
            });
        }
        Some(Err(e)) if e != "cancelled" => {
            ui.label(
                RichText::new(format!("Couldn't save the images: {e}"))
                    .size(12.0)
                    .color(BAD),
            );
        }
        Some(Err(_)) => {
            ui.label(RichText::new("Cancelled.").size(12.0).color(TEXT));
        }
        None => {}
    }
    act(state, action, source);
}

fn act(state: &mut AppState, action: Option<Action>, source: Option<&Arc<ImageSource>>) {
    match action {
        Some(Action::Start(folder)) => match source {
            Some(source) => state.export_images_from(source.clone(), folder),
            None => state.start_images_export(folder),
        },
        Some(Action::Cancel) => state.cancel_images_export(),
        Some(Action::Open(path)) => {
            if let Err(e) = open_external(&path) {
                state.status = Some(Status::Error(format!("open {}: {e}", path.display())));
            }
        }
        None => {}
    }
}
