//! The colour edit panel on the UI images page, its toolbar row (bulk, undo, redo) and the
//! undo shortcuts. Thin: every change is an [`Action`] run through `crate::images_edit`.

use std::ops::RangeInclusive;
use std::time::Instant;

use dt_core::hud::icons::Target;
use dt_core::texture::adjust::{Adjust, AdjustKind, Blend, Rgb};
use eframe::egui::color_picker::color_edit_button_srgb;
use eframe::egui::{
    self, Align, Color32, CornerRadius, Key, Layout, Modifiers, RichText, Sense, Stroke,
    StrokeKind, Ui, vec2,
};

use crate::images::{ImageEntry, Library};
use crate::images_edit::{AMBER, ColorMode, SETTLE};
use crate::state::AppState;
use crate::theme::{ACCENT, BORDER, TEXT, WEAK};
use crate::widgets;

const SWATCHES: [(&str, Rgb); 5] = [
    ("Amber", AMBER),
    ("Sapphire", Rgb([0x4d, 0x75, 0xc3])),
    ("White", Rgb::WHITE),
    ("Black", Rgb::BLACK),
    ("DeadTune gold", Rgb([0xf0, 0xb3, 0x41])),
];
const LABEL_WIDTH: f32 = 74.0;

pub enum Action {
    Edit(String, Adjust),
    Remove(String, AdjustKind),
    Commit,
    Mode(ColorMode),
    Color(Rgb),
    Strength(u8),
    Blend(Blend),
    Swap(String, Rgb, Rgb),
    /// The panel's colour operation on these images.
    ApplyColor(Vec<String>),
    Reset(Vec<String>),
    Undo,
    Redo,
}

pub fn run(state: &mut AppState, action: Action) {
    let now = Instant::now();
    match action {
        Action::Edit(path, adjust) => state.edit_image(&path, adjust, now),
        Action::Remove(path, kind) => state.remove_edit(&path, kind),
        Action::Commit => state.commit_edits(),
        Action::Mode(mode) => state.set_color_mode(mode, now),
        Action::Color(color) => state.set_color(color, now),
        Action::Strength(strength) => state.set_strength(strength, now),
        Action::Blend(blend) => state.set_blend(blend, now),
        Action::Swap(path, from, to) => state.swap_color(&path, from, to, now),
        Action::ApplyColor(paths) => state.adjust_images(&paths, state.current_color_op()),
        Action::Reset(paths) => state.reset_images(&paths),
        Action::Undo => state.undo_images(),
        Action::Redo => state.redo_images(),
    }
}

/// Once a frame before drawing: commits a rested draft and reads the selected icon's colours.
pub fn prepare(ctx: &egui::Context, state: &mut AppState) {
    if state.settle_edits(Instant::now()) {
        ctx.request_repaint_after(SETTLE);
    }
    if let Some(path) = state.images.selected.clone() {
        state.palette_for(&path);
    }
}

/// Ctrl+Z and Ctrl+Shift+Z (Cmd on a Mac), unless a text field has the keyboard.
pub fn shortcuts(ui: &mut Ui, actions: &mut Vec<Action>) {
    if ui.memory(|m| m.focused().is_some()) {
        return;
    }
    ui.input_mut(|i| {
        if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) {
            actions.push(Action::Redo);
        } else if i.consume_key(Modifiers::COMMAND, Key::Z) {
            actions.push(Action::Undo);
        }
    });
}

/// Bulk colour and folder reset, after the folder chips.
pub fn bulk_row(ui: &mut Ui, state: &AppState, library: &Library, actions: &mut Vec<Action>) {
    let small = |text: &str| egui::Button::new(RichText::new(text).size(12.0));
    let visible: Vec<String> = state
        .images
        .visible(library, &state.profile.hud.icons)
        .iter()
        .map(|e| e.path.clone())
        .collect();
    let op = state.current_color_op();
    let can_color = !op.is_identity();
    let marked = state.images.edit.marked.len();
    if marked > 0 {
        let label = format!("Apply to {marked} selected");
        if ui
            .add_enabled(can_color, small(&label))
            .on_hover_text(format!("{} on every marked image", op.label()))
            .on_disabled_hover_text("Select an image, then pick a colour and strength in its panel")
            .clicked()
        {
            actions.push(Action::ApplyColor(state.marked_or_selected()));
        }
    }
    let folder = state.images.folder.is_some();
    if !visible.is_empty() && (folder || !state.images.search.trim().is_empty()) {
        let label = if folder {
            "Apply to folder".to_string()
        } else {
            format!("Apply to {} shown", visible.len())
        };
        if ui
            .add_enabled(can_color, small(&label))
            .on_hover_text(format!(
                "{} on the {} images shown",
                op.label(),
                visible.len()
            ))
            .on_disabled_hover_text("Select an image, then pick a colour and strength in its panel")
            .clicked()
        {
            actions.push(Action::ApplyColor(visible.clone()));
        }
        let changed: Vec<String> = visible
            .into_iter()
            .filter(|p| state.image_override(p).is_some())
            .collect();
        if !changed.is_empty()
            && ui
                .add(small("Reset this folder"))
                .on_hover_text("Puts the images shown back to the game's own")
                .clicked()
        {
            actions.push(Action::Reset(changed));
        }
    }
}

/// The colour controls for the selected image.
pub fn section(ui: &mut Ui, state: &AppState, entry: &ImageEntry, actions: &mut Vec<Action>) {
    let path = &entry.path;
    let shown = state.shown_adjustments(path);
    if entry.kind == Target::Vector {
        palette_row(ui, state, path, shown, actions);
    }
    color_rows(ui, state, actions);
    ui.add_space(4.0);
    let value = |kind: AdjustKind, default: i32| {
        shown
            .iter()
            .find(|a| a.kind() == kind)
            .map_or(default, |a| match *a {
                Adjust::Hue { degrees } => degrees.into(),
                Adjust::Saturation { percent }
                | Adjust::Brightness { percent }
                | Adjust::Contrast { percent } => percent.into(),
                Adjust::Opacity { percent } => percent.into(),
                _ => default,
            })
    };
    let rows: [(&str, AdjustKind, RangeInclusive<i32>, i32, &str); 5] = [
        ("Hue", AdjustKind::Hue, -180..=180, 0, ""),
        ("Saturation", AdjustKind::Saturation, -100..=100, 0, "%"),
        ("Brightness", AdjustKind::Brightness, -100..=100, 0, "%"),
        ("Contrast", AdjustKind::Contrast, -100..=100, 0, "%"),
        ("Opacity", AdjustKind::Opacity, 0..=100, 100, "%"),
    ];
    for (label, kind, range, default, suffix) in rows {
        let (changed, stopped) = slider(ui, label, value(kind, default), range, suffix);
        if let Some(v) = changed {
            let adjust = match kind {
                AdjustKind::Hue => Adjust::Hue { degrees: v as i16 },
                AdjustKind::Saturation => Adjust::Saturation { percent: v as i16 },
                AdjustKind::Brightness => Adjust::Brightness { percent: v as i16 },
                AdjustKind::Contrast => Adjust::Contrast { percent: v as i16 },
                _ => Adjust::Opacity { percent: v as u8 },
            };
            actions.push(Action::Edit(path.clone(), adjust));
        }
        if stopped {
            actions.push(Action::Commit);
        }
    }
    let inverted = shown.contains(&Adjust::Invert);
    ui.horizontal(|ui| {
        fixed_label(ui, "Invert");
        if widgets::switch(ui, inverted).clicked() {
            actions.push(if inverted {
                Action::Remove(path.clone(), AdjustKind::Invert)
            } else {
                Action::Edit(path.clone(), Adjust::Invert)
            });
            actions.push(Action::Commit);
        }
    });
    applied_list(ui, path, shown, actions);
}

fn fixed_label(ui: &mut Ui, text: &str) {
    ui.allocate_ui_with_layout(
        vec2(LABEL_WIDTH, 20.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.set_min_width(LABEL_WIDTH);
            ui.label(RichText::new(text).size(12.0).color(WEAK));
        },
    );
}

/// A labelled slider filling the row; returns the new value and whether a drag ended.
fn slider(
    ui: &mut Ui,
    label: &str,
    mut value: i32,
    range: RangeInclusive<i32>,
    suffix: &str,
) -> (Option<i32>, bool) {
    ui.horizontal(|ui| {
        fixed_label(ui, label);
        ui.spacing_mut().slider_width = (ui.available_width() - 62.0).max(60.0);
        let before = value;
        let signed = *range.start() < 0;
        let response = ui.add(
            egui::Slider::new(&mut value, range)
                .suffix(suffix)
                .trailing_fill(!signed),
        );
        let changed = (value != before).then_some(value);
        (changed, response.drag_stopped())
    })
    .inner
}

fn color_rows(ui: &mut Ui, state: &AppState, actions: &mut Vec<Action>) {
    let e = &state.images.edit;
    let [r, g, b] = e.color.0;
    let current = Color32::from_rgb(r, g, b);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        for mode in ColorMode::ALL {
            let on = e.mode == mode && e.strength > 0;
            if widgets::chip(ui, mode.label(), current, Some(on))
                .on_hover_text(mode_hint(mode))
                .clicked()
                && e.mode != mode
            {
                actions.push(Action::Mode(mode));
            }
        }
    });
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        fixed_label(ui, "Colour");
        let mut rgb = e.color.0;
        ui.spacing_mut().interact_size = vec2(34.0, 20.0);
        color_edit_button_srgb(ui, &mut rgb).on_hover_text(e.color.hex());
        if rgb != e.color.0 {
            actions.push(Action::Color(Rgb(rgb)));
        }
        ui.add_space(4.0);
        for (name, color) in SWATCHES {
            if swatch(ui, color, color == e.color)
                .on_hover_text(format!("{name} {}", color.hex()))
                .clicked()
            {
                actions.push(Action::Color(color));
            }
        }
    });
    let (changed, stopped) = slider(ui, "Strength", e.strength.into(), 0..=100, "%");
    if let Some(v) = changed {
        actions.push(Action::Strength(v as u8));
    }
    if stopped {
        actions.push(Action::Commit);
    }
    if e.mode == ColorMode::Overlay {
        ui.horizontal(|ui| {
            fixed_label(ui, "Blend");
            egui::ComboBox::from_id_salt("images_blend")
                .width(120.0)
                .selected_text(blend_label(e.blend))
                .show_ui(ui, |ui| {
                    for blend in Blend::ALL {
                        if ui
                            .selectable_label(blend == e.blend, blend_label(blend))
                            .clicked()
                            && blend != e.blend
                        {
                            actions.push(Action::Blend(blend));
                            actions.push(Action::Commit);
                        }
                    }
                });
        });
    }
}

fn blend_label(blend: Blend) -> &'static str {
    match blend {
        Blend::Normal => "Normal",
        Blend::Multiply => "Multiply",
        Blend::Screen => "Screen",
        Blend::Overlay => "Overlay",
    }
}

fn mode_hint(mode: ColorMode) -> &'static str {
    match mode {
        ColorMode::Tint => "Multiplies the picture by the colour; dark parts stay dark",
        ColorMode::Colorize => "Paints white and grey parts in the colour, keeping light and shade",
        ColorMode::Overlay => "Lays the colour on top, blended with the picture",
    }
}

fn swatch(ui: &mut Ui, color: Rgb, on: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
    let [r, g, b] = color.0;
    let painter = ui.painter();
    painter.rect(
        rect.shrink(1.0),
        CornerRadius::same(4),
        Color32::from_rgb(r, g, b),
        Stroke::new(
            if on { 2.0 } else { 1.0 },
            if on {
                ACCENT
            } else if response.hovered() {
                TEXT
            } else {
                BORDER
            },
        ),
        StrokeKind::Outside,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn palette_row(
    ui: &mut Ui,
    state: &AppState,
    path: &str,
    shown: &[Adjust],
    actions: &mut Vec<Action>,
) {
    let Some(palette) = state.cached_palette(path) else {
        return;
    };
    ui.label(RichText::new("Colours in this icon").size(12.0).color(TEXT));
    if !palette.colors.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().interact_size = vec2(30.0, 20.0);
            for swatch in &palette.colors {
                let from = swatch.rgb;
                let to = shown.iter().find_map(|a| match *a {
                    Adjust::Swap { from: f, to } if f == from => Some(to),
                    _ => None,
                });
                let before = to.unwrap_or(from).0;
                let mut rgb = before;
                let uses = match swatch.uses {
                    1 => "used once".to_string(),
                    n => format!("used {n} times"),
                };
                let hover = match to {
                    Some(to) => format!("{from}, {uses}; now {to}"),
                    None => format!("{from}, {uses}"),
                };
                color_edit_button_srgb(ui, &mut rgb).on_hover_text(hover);
                if rgb != before {
                    actions.push(Action::Swap(path.to_string(), from, Rgb(rgb)));
                }
            }
        });
    }
    let note = match (palette.colors.is_empty(), palette.current_color) {
        (true, false) => Some("This icon names no colours; it draws in black. Use the rows below."),
        (true, true) => Some("This icon takes the game's text colour. Use the rows below."),
        (false, true) => Some("Some parts take the game's text colour, which these can't change."),
        (false, false) => None,
    };
    if let Some(note) = note {
        ui.label(RichText::new(note).size(11.0).color(WEAK));
    }
    ui.add_space(4.0);
}

fn applied_list(ui: &mut Ui, path: &str, shown: &[Adjust], actions: &mut Vec<Action>) {
    if shown.is_empty() {
        return;
    }
    ui.add_space(6.0);
    ui.label(RichText::new("Applied edits").size(12.0).color(TEXT));
    for adjust in shown {
        ui.horizontal(|ui| {
            ui.label(RichText::new(adjust.label()).size(11.5).color(WEAK));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(egui::Button::new(RichText::new("Remove").size(10.5)).small())
                    .clicked()
                {
                    actions.push(Action::Remove(path.to_string(), adjust.kind()));
                }
            });
        });
    }
}
