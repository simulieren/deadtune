//! In-game settings page (simple-view section, advanced HUD tab): the Wide FOV row and
//! the DeadTune group DeadTune adds to Deadlock's own settings menu (`hud::ingame`).
//! Edits live in `profile.hud.ingame` and go out with the normal Apply.

use dt_core::hud::ingame::{IngameSettings, PERFORMANCE_ROWS, PerfRow};
use eframe::egui::{self, Align, Layout, RichText, Ui};

use crate::minimap_view::marked;
use crate::state::AppState;
use crate::theme::{ACCENT, WARN, WEAK};
use crate::widgets;

pub fn page(ui: &mut Ui, state: &mut AppState) {
    crate::hud_view::hud_error(ui, state);
    let current = state.profile.hud.ingame.clone();
    let mut next = current.clone();
    widgets::card(ui, |ui| {
        header(ui, &current, &mut next);
        wide_fov(ui, &current, &mut next);
    });
    widgets::card(ui, |ui| {
        group(ui, state, &current, &mut next);
    });
    credits(ui);
    if next != current {
        state.set_ingame(next);
    }
}

fn header(ui: &mut Ui, current: &IngameSettings, next: &mut IngameSettings) {
    ui.horizontal(|ui| {
        widgets::caption(ui, "In-game settings");
        widgets::badge(ui, "Experimental, untested in game", WARN);
        let n = current.changed_count();
        ui.add_space(8.0);
        ui.label(
            RichText::new(format!("Changed: {n}"))
                .small()
                .color(if n > 0 { ACCENT } else { WEAK }),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add_enabled(n > 0, egui::Button::new("Reset all"))
                .on_hover_text("Back to the game's own settings menu")
                .clicked()
            {
                *next = IngameSettings::default();
            }
        });
    });
    widgets::hint(
        ui,
        "Rows DeadTune adds to Deadlock's Settings menu, rebuilt from your game's own menu \
         after every update. The game's stock rows are left exactly as they are.",
    );
}

fn wide_fov(ui: &mut Ui, current: &IngameSettings, next: &mut IngameSettings) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if widgets::switch(ui, current.wide_fov).clicked() {
            next.wide_fov = !current.wide_fov;
        }
        ui.label(marked("Wide FOV slider", current.wide_fov))
            .on_hover_text(
                "Settings > Game > Camera Settings, right under the game's FOV slider. \
             Drag it in a match to widen the view past the game's 90 degree limit.",
            );
    });
    widgets::hint(
        ui,
        "What you set in game is carried back to the Overview's Wide view the next time \
         DeadTune starts, so it survives restarts without touching the menu again.",
    );
}

fn group(ui: &mut Ui, state: &AppState, current: &IngameSettings, next: &mut IngameSettings) {
    ui.horizontal(|ui| {
        widgets::caption(ui, "DeadTune group");
        let n = current.performance.len();
        ui.add_space(8.0);
        ui.label(
            RichText::new(format!("{n} of {} rows", PERFORMANCE_ROWS.len()))
                .small()
                .color(if n > 0 { ACCENT } else { WEAK }),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add_enabled(n < PERFORMANCE_ROWS.len(), egui::Button::new("All"))
                .clicked()
            {
                next.performance = PERFORMANCE_ROWS
                    .iter()
                    .map(|r| r.convar.to_string())
                    .collect();
            }
            if ui.add_enabled(n > 0, egui::Button::new("None")).clicked() {
                next.performance.clear();
            }
        });
    });
    widgets::hint(
        ui,
        "A DeadTune section under Settings > Advanced with live sliders for settings that \
         normally need a restart. Changes made there last until the game closes; set them \
         here in DeadTune to keep them.",
    );
    ui.add_space(4.0);
    for row in PERFORMANCE_ROWS {
        row_switch(ui, state, row, current, next);
    }
}

fn row_switch(
    ui: &mut Ui,
    state: &AppState,
    row: &PerfRow,
    current: &IngameSettings,
    next: &mut IngameSettings,
) {
    let on = current.performance.contains(row.convar);
    ui.horizontal(|ui| {
        if widgets::switch(ui, on).clicked() {
            if on {
                next.performance.remove(row.convar);
            } else {
                next.performance.insert(row.convar.to_string());
            }
        }
        let note = state
            .catalog
            .get(row.convar)
            .map(|e| e.notes.clone())
            .unwrap_or_default();
        ui.label(marked(row.label, on)).on_hover_text(note);
    });
}

fn credits(ui: &mut Ui) {
    ui.add_space(4.0);
    ui.label(
        RichText::new(
            "Idea by Mixboat (Wide FOV Slider); rebuilt by DeadTune from your game files. \
             The game's own settings slider writes whichever setting it is bound to; DeadTune \
             only adds rows, never replaces the menu.",
        )
        .size(11.5)
        .color(WEAK),
    );
}
