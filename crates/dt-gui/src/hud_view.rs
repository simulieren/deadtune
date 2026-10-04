//! HUD layout editor: a 16:9 preview you can drag elements on, plus per-element controls.
//! Edits live in the profile and go out with the normal Apply.

use dt_core::hud::elements::{ELEMENTS, ElementId};
use dt_core::hud::layout::{self, ElementEdit, OFFSET_LIMIT, SCALE_RANGE, Visibility};
use eframe::egui::{self, Color32, Pos2, Rect, Stroke};

use crate::state::AppState;

const CANVAS_MAX_WIDTH: f32 = 720.0;

fn visibility_label(v: Visibility) -> &'static str {
    match v {
        Visibility::Vanilla => "Normal",
        Visibility::Hidden => "Hidden",
        Visibility::Shown => "Always shown",
    }
}

pub fn hud(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label("Drag elements on the preview or use the controls. Applies with Apply; takes effect next time you start Deadlock.");
    if let Some(e) = state.hud_error() {
        ui.colored_label(
            Color32::from_rgb(230, 180, 60),
            "HUD changes can't be applied right now; other settings still apply.",
        )
        .on_hover_text(e);
    }
    let mut changes: Vec<(ElementId, ElementEdit)> = Vec::new();
    canvas(ui, state, &mut changes);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !state.profile.hud.is_vanilla(),
                egui::Button::new("Reset HUD"),
            )
            .clicked()
        {
            state.reset_hud();
        }
    });
    egui::Grid::new("hud_controls")
        .striped(true)
        .show(ui, |ui| {
            ui.strong("Element");
            ui.strong("Show");
            ui.strong("Move X");
            ui.strong("Move Y");
            ui.strong("Size %");
            ui.strong("Opacity %");
            ui.end_row();
            for spec in ELEMENTS {
                let mut edit = state
                    .profile
                    .hud
                    .elements
                    .get(&spec.id)
                    .cloned()
                    .unwrap_or_default();
                let before = edit.clone();
                let selected = state.ui.hud_selected == Some(spec.id);
                if ui
                    .selectable_label(selected, spec.label)
                    .on_hover_text(spec.notes)
                    .clicked()
                {
                    state.ui.hud_selected = Some(spec.id);
                }
                egui::ComboBox::from_id_salt(("hud_vis", spec.id))
                    .selected_text(visibility_label(edit.visibility))
                    .show_ui(ui, |ui| {
                        for v in [Visibility::Vanilla, Visibility::Hidden, Visibility::Shown] {
                            ui.selectable_value(&mut edit.visibility, v, visibility_label(v));
                        }
                    });
                ui.add(
                    egui::DragValue::new(&mut edit.offset_x).range(-OFFSET_LIMIT..=OFFSET_LIMIT),
                );
                ui.add(
                    egui::DragValue::new(&mut edit.offset_y).range(-OFFSET_LIMIT..=OFFSET_LIMIT),
                );
                ui.add(egui::Slider::new(&mut edit.scale_pct, SCALE_RANGE).integer());
                ui.add(egui::Slider::new(&mut edit.opacity_pct, 0..=100).integer());
                if edit != before {
                    changes.push((spec.id, edit));
                }
                ui.end_row();
            }
        });
    for (id, edit) in changes {
        state.set_hud_element(id, edit);
    }
}

fn canvas(ui: &mut egui::Ui, state: &mut AppState, changes: &mut Vec<(ElementId, ElementEdit)>) {
    let width = ui.available_width().min(CANVAS_MAX_WIDTH);
    let size = egui::vec2(width, width * 9.0 / 16.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, Color32::from_rgb(28, 32, 38));
    let k = size.y / 1080.0;
    for item in layout::preview(&state.profile.hud, [size.x, size.y]) {
        let [x, y, w, h] = item.rect;
        let r = Rect::from_min_size(rect.min + egui::vec2(x, y), egui::vec2(w, h));
        let spec = ELEMENTS
            .iter()
            .find(|s| s.id == item.id)
            .expect("preview yields known ids");
        let selected = state.ui.hud_selected == Some(item.id);
        let base = if selected {
            Color32::from_rgb(90, 190, 110)
        } else {
            Color32::from_rgb(110, 150, 220)
        };
        if item.visible {
            painter.rect_filled(r, 2.0, base.gamma_multiply(0.35 * item.opacity.max(0.15)));
        }
        let stroke = if item.visible {
            Stroke::new(1.5, base)
        } else {
            Stroke::new(1.0, Color32::DARK_GRAY)
        };
        painter.rect_stroke(r, 2.0, stroke, egui::StrokeKind::Inside);
        painter.text(
            r.left_top() + egui::vec2(3.0, 2.0),
            egui::Align2::LEFT_TOP,
            spec.label,
            egui::FontId::proportional(10.0),
            if item.visible {
                Color32::WHITE
            } else {
                Color32::GRAY
            },
        );
        let response = ui.interact(
            r,
            ui.id().with(("hud_el", item.id)),
            egui::Sense::click_and_drag(),
        );
        if response.clicked() || response.drag_started() {
            state.ui.hud_selected = Some(item.id);
        }
        if response.dragged() {
            let delta = response.drag_delta() / k;
            let mut edit = state
                .profile
                .hud
                .elements
                .get(&item.id)
                .cloned()
                .unwrap_or_default();
            edit.offset_x =
                (edit.offset_x + delta.x.round() as i32).clamp(-OFFSET_LIMIT, OFFSET_LIMIT);
            edit.offset_y =
                (edit.offset_y + delta.y.round() as i32).clamp(-OFFSET_LIMIT, OFFSET_LIMIT);
            changes.push((item.id, edit));
        }
    }
    painter.text(
        Pos2::new(rect.right() - 6.0, rect.bottom() - 4.0),
        egui::Align2::RIGHT_BOTTOM,
        "1920x1080 preview",
        egui::FontId::proportional(10.0),
        Color32::GRAY,
    );
}
