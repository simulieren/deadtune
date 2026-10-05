//! UI images page: folder chips, a search box and a virtualised thumbnail grid on the left,
//! the selected image large on the right with export, replace and reset. Pictures decode on
//! `crate::thumbs`' workers; every change is `crate::images` on the state.

use dt_core::hud::icons::{IconOverride, Target};
use dt_core::texture::encode::Fit;
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Layout, Margin, Rect, RichText, Sense,
    Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2,
};

use crate::icons::{self, Icon};
use crate::images::{ExportKind, ImageEntry, Library, Picture, Tone, Zoom};
use crate::images_edit_view::{self as edit_view, Action};
use crate::state::{AppState, Status};
use crate::theme::{self, ACCENT, BAD, BORDER, CARD, CARD_HOVER, GOOD, TEXT, WARN, WEAK};
use crate::thumbs::{Slot, Thumbs};
use crate::views::open_external;
use crate::widgets;

const TILE: f32 = 92.0;
const LABEL: f32 = 18.0;
const GAP: f32 = 8.0;
const PANEL: f32 = 330.0;
const PREVIEW_SIDE: u32 = 512;

enum Edit {
    Select(String, egui::Modifiers),
    Colour(Action),
    Folder(Option<String>),
    ChangedOnly(bool),
    Fit(Fit),
    Zoom(Zoom),
    Reset(String),
    ResetAll,
    ReplaceFromPath,
    Export(String, ExportKind),
    OpenExports,
}

pub fn page(ui: &mut Ui, state: &mut AppState) {
    state.load_images();
    edit_view::prepare(ui.ctx(), state);
    let library = match state.images.library.take() {
        Some(Ok(library)) => library,
        other => {
            if let Some(Err(e)) = &other {
                ui.colored_label(BAD, e.as_str());
            }
            state.images.library = other;
            return;
        }
    };
    let mut thumbs = state
        .images
        .thumbs
        .take()
        .unwrap_or_else(|| Thumbs::new(ui.ctx(), library.source.clone()));
    thumbs.poll();
    let mut edits = Vec::new();
    colour(&mut edits, |actions| edit_view::shortcuts(ui, actions));
    let drop_target = body(ui, state, &library, &mut thumbs, &mut edits);
    thumbs.end_frame();
    state.images.thumbs = Some(thumbs);
    state.images.library = Some(Ok(library));
    let dropped: Vec<Result<Vec<u8>, String>> =
        ui.input(|i| i.raw.dropped_files.iter().map(|f| f.bytes()).collect());
    if !dropped.is_empty() {
        state.drop_image_files(drop_target, dropped);
    }
    run(state, edits);
}

/// Collects the colour panel's actions as page edits.
fn colour(edits: &mut Vec<Edit>, draw: impl FnOnce(&mut Vec<Action>)) {
    let mut actions = Vec::new();
    draw(&mut actions);
    edits.extend(actions.into_iter().map(Edit::Colour));
}

fn run(state: &mut AppState, edits: Vec<Edit>) {
    for edit in edits {
        match edit {
            Edit::Select(path, keys) => {
                let visible = state.visible_paths();
                state.mark_image(&path, keys.command, keys.shift, &visible);
            }
            Edit::Colour(action) => edit_view::run(state, action),
            Edit::Folder(folder) => state.images.folder = folder,
            Edit::ChangedOnly(on) => state.images.changed_only = on,
            Edit::Fit(fit) => state.set_image_fit(fit),
            Edit::Zoom(zoom) => state.images.zoom = zoom,
            Edit::Reset(path) => state.reset_image(&path),
            Edit::ResetAll => state.reset_all_images(),
            Edit::ReplaceFromPath => {
                let path = state.images.replace_path.clone();
                state.replace_selected_from_path(&path);
            }
            Edit::Export(path, kind) => {
                let _ = state.export_image(&path, kind);
            }
            Edit::OpenExports => {
                let dir = state.exports_dir();
                if let Err(e) = open_external(&dir) {
                    state.status = Some(Status::Error(format!("open {}: {e}", dir.display())));
                }
            }
        }
    }
}

/// Draws the page; returns the tile under the pointer, where a dropped file goes.
fn body(
    ui: &mut Ui,
    state: &mut AppState,
    library: &Library,
    thumbs: &mut Thumbs,
    edits: &mut Vec<Edit>,
) -> Option<String> {
    toolbar(ui, state, library, edits);
    export_all_strip(ui, state, library);
    let mut replace_path = std::mem::take(&mut state.images.replace_path);
    let target = draw_body(ui, state, library, thumbs, &mut replace_path, edits);
    state.images.replace_path = replace_path;
    target
}

fn draw_body(
    ui: &mut Ui,
    state: &AppState,
    library: &Library,
    thumbs: &mut Thumbs,
    replace_path: &mut String,
    edits: &mut Vec<Edit>,
) -> Option<String> {
    ui.add_space(6.0);
    folder_chips(ui, state, library, edits);
    ui.add_space(10.0);
    let area = ui.available_rect_before_wrap();
    let panel = PANEL.min(area.width() * 0.42);
    let grid_rect = Rect::from_min_max(area.min, pos2(area.right() - panel - 14.0, area.bottom()));
    let panel_rect = Rect::from_min_max(pos2(area.right() - panel, area.top()), area.max);
    ui.allocate_rect(area, Sense::hover());
    let mut drop_target = None;
    let visible = state.images.visible(library, &state.profile.hud.icons);
    let preview = state
        .images
        .selected
        .as_deref()
        .and_then(|p| library.entry(p));
    if let Some(entry) = preview {
        preview_pictures(state, entry, thumbs);
    }
    ui.scope_builder(UiBuilder::new().max_rect(grid_rect), |ui| {
        drop_target = grid(ui, state, &visible, thumbs, edits);
    });
    ui.scope_builder(UiBuilder::new().max_rect(panel_rect), |ui| {
        side_panel(ui, state, preview, thumbs, replace_path, edits);
    });
    drop_target
}

fn toolbar(ui: &mut Ui, state: &mut AppState, library: &Library, edits: &mut Vec<Edit>) {
    let changed = state.images_changed_count();
    ui.horizontal(|ui| {
        search(ui, &mut state.images.search);
        let label = format!("Changed by me  {changed}");
        if widgets::chip(ui, &label, ACCENT, Some(state.images.changed_only))
            .on_hover_text("Show only the images you replaced")
            .clicked()
        {
            edits.push(Edit::ChangedOnly(!state.images.changed_only));
        }
        if changed > 0
            && ui
                .button(RichText::new("Reset all images").size(12.0))
                .on_hover_text("Puts every image back to the game's own after Apply")
                .clicked()
        {
            edits.push(Edit::ResetAll);
        }
        colour(edits, |actions| {
            edit_view::history_buttons(ui, state, actions)
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let vectors = library
                .entries
                .iter()
                .filter(|e| e.kind == Target::Vector)
                .count();
            ui.add(
                egui::Label::new(
                    RichText::new(format!(
                        "{} images, {vectors} vector · {}",
                        library.entries.len(),
                        library.source.describe()
                    ))
                    .size(11.5)
                    .color(WEAK),
                )
                .truncate(),
            );
        });
    });
}

fn export_all_strip(ui: &mut Ui, state: &mut AppState, library: &Library) {
    ui.add_space(4.0);
    let folder = state.images.folder.clone();
    crate::images_export_view::strip(ui, state, Some(&library.source), folder.as_deref());
}

fn search(ui: &mut Ui, query: &mut String) {
    let (rect, _) = ui.allocate_exact_size(vec2(240.0, 26.0), Sense::hover());
    ui.painter().rect(
        rect,
        CornerRadius::same(6),
        CARD,
        Stroke::new(1.0, BORDER),
        StrokeKind::Inside,
    );
    icons::paint(
        ui.painter(),
        Rect::from_center_size(rect.left_center() + vec2(13.0, 0.0), vec2(13.0, 13.0)),
        Icon::Search,
        WEAK,
    );
    let text_rect = Rect::from_min_max(
        rect.left_top() + vec2(26.0, 0.0),
        rect.right_bottom() - vec2(6.0, 0.0),
    );
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(text_rect)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.add(
                egui::TextEdit::singleline(query)
                    .hint_text("Search by name or folder")
                    .desired_width(f32::INFINITY)
                    .frame(egui::Frame::NONE),
            );
        },
    );
}

fn folder_chip(ui: &mut Ui, text: &str, count: usize, on: bool) -> egui::Response {
    let font = FontId::proportional(11.5);
    let count_text = count.to_string();
    let w = ui
        .painter()
        .layout_no_wrap(text.into(), font.clone(), TEXT)
        .size()
        .x;
    let cw = ui
        .painter()
        .layout_no_wrap(count_text.clone(), FontId::proportional(10.5), WEAK)
        .size()
        .x;
    let (rect, response) = ui.allocate_exact_size(vec2(w + cw + 26.0, 22.0), Sense::click());
    let fill = if on {
        ACCENT.gamma_multiply(0.16)
    } else if response.hovered() {
        CARD_HOVER
    } else {
        CARD
    };
    let stroke = Stroke::new(
        1.0,
        if on {
            ACCENT.gamma_multiply(0.7)
        } else {
            BORDER
        },
    );
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(255),
        fill,
        stroke,
        StrokeKind::Inside,
    );
    painter.text(
        rect.left_center() + vec2(10.0, 0.0),
        Align2::LEFT_CENTER,
        text,
        font,
        if on { TEXT } else { WEAK.gamma_multiply(1.2) },
    );
    painter.text(
        rect.right_center() - vec2(10.0, 0.0),
        Align2::RIGHT_CENTER,
        count_text,
        FontId::proportional(10.5),
        if on { ACCENT } else { WEAK.gamma_multiply(0.8) },
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn folder_chips(ui: &mut Ui, state: &AppState, library: &Library, edits: &mut Vec<Edit>) {
    let current = state.images.folder.as_deref();
    let top = current.map(|f| f.split('/').next().unwrap_or(f));
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(5.0, 5.0);
        if folder_chip(ui, "All", library.entries.len(), current.is_none()).clicked() {
            edits.push(Edit::Folder(None));
        }
        for folder in library.top_folders() {
            if folder_chip(
                ui,
                folder.name(),
                folder.count,
                top == Some(folder.path.as_str()),
            )
            .clicked()
            {
                edits.push(Edit::Folder(Some(folder.path.clone())));
            }
        }
        ui.add_space(8.0);
        colour(edits, |actions| {
            edit_view::bulk_row(ui, state, library, actions)
        });
    });
    let Some(top) = top else { return };
    let subs: Vec<_> = library.subfolders(top).collect();
    if subs.is_empty() {
        return;
    }
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(5.0, 5.0);
        ui.label(RichText::new(format!("{top} /")).size(11.5).color(WEAK));
        for folder in subs {
            if folder_chip(
                ui,
                folder.name(),
                folder.count,
                current == Some(folder.path.as_str()),
            )
            .clicked()
            {
                let next = if current == Some(folder.path.as_str()) {
                    top.to_string()
                } else {
                    folder.path.clone()
                };
                edits.push(Edit::Folder(Some(next)));
            }
        }
    });
}

fn tile_picture(state: &AppState, entry: &ImageEntry, side: u32) -> Picture {
    state
        .your_picture(&entry.path, side)
        .unwrap_or_else(|| Picture::Game {
            path: entry.path.clone(),
            side,
        })
}

fn grid(
    ui: &mut Ui,
    state: &AppState,
    visible: &[&ImageEntry],
    thumbs: &mut Thumbs,
    edits: &mut Vec<Edit>,
) -> Option<String> {
    if visible.is_empty() {
        ui.add_space(30.0);
        ui.vertical_centered(|ui| {
            let text = if state.images.changed_only {
                "You haven't replaced any images here yet."
            } else {
                "No images match."
            };
            ui.label(RichText::new(text).color(WEAK));
        });
        return None;
    }
    let width = ui.available_width();
    let columns = (((width + GAP) / (TILE + GAP)).floor() as usize).max(1);
    let size = ((width - GAP * (columns - 1) as f32) / columns as f32).floor() - 1.0;
    let rows = visible.len().div_ceil(columns);
    let side = ((TILE - 12.0) * ui.ctx().pixels_per_point()).round() as u32;
    let pointer = ui.ctx().pointer_latest_pos();
    let hovering_files = ui.input(|i| !i.raw.hovered_files.is_empty());
    let mut target = None;
    ui.spacing_mut().item_spacing = vec2(GAP, GAP);
    egui::ScrollArea::vertical()
        .id_salt("images_grid")
        .auto_shrink(false)
        .show_rows(ui, size + LABEL, rows, |ui, range| {
            let first = range.start.saturating_sub(1);
            let last = (range.end + 1).min(rows);
            for row in range {
                ui.horizontal(|ui| {
                    for entry in visible.iter().skip(row * columns).take(columns) {
                        let response = tile(ui, state, entry, size, side, thumbs, hovering_files);
                        if response.clicked() {
                            let keys = ui.input(|i| i.modifiers);
                            edits.push(Edit::Select(entry.path.clone(), keys));
                        }
                        if pointer.is_some_and(|p| response.rect.contains(p)) {
                            target = Some(entry.path.clone());
                        }
                    }
                });
            }
            for row in [first, last.saturating_sub(1)] {
                for entry in visible.iter().skip(row * columns).take(columns) {
                    thumbs.get(&tile_picture(state, entry, side));
                }
            }
        });
    target
}

fn tile(
    ui: &mut Ui,
    state: &AppState,
    entry: &ImageEntry,
    size: f32,
    side: u32,
    thumbs: &mut Thumbs,
    hovering_files: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(size, size + LABEL), Sense::click());
    let selected = state.images.selected.as_deref() == Some(entry.path.as_str());
    let marked = state.images.edit.marked.contains(&entry.path);
    let changed = state.image_override(&entry.path);
    let problem = state.image_problem(&entry.path);
    let image_rect = Rect::from_min_size(rect.min, vec2(size, size));
    let painter = ui.painter();
    let fill = if response.hovered() { CARD_HOVER } else { CARD };
    painter.rect_filled(image_rect, CornerRadius::same(6), fill);
    let slot = thumbs.get(&tile_picture(state, entry, side));
    let inner = Rect::from_center_size(image_rect.center(), vec2(TILE - 16.0, TILE - 16.0));
    let mut reason = None;
    match slot {
        Some(Slot::Ready { texture, .. }) => {
            let size = texture.size_vec2();
            let scale = (inner.width() / size.x).min(inner.height() / size.y);
            let shown = Rect::from_center_size(inner.center(), size * scale);
            painter.image(
                texture.id(),
                shown,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        Some(Slot::Failed(why)) => {
            painter.rect_filled(
                inner.shrink(10.0),
                CornerRadius::same(4),
                BAD.gamma_multiply(0.14),
            );
            painter.text(
                inner.center() - vec2(0.0, 6.0),
                Align2::CENTER_CENTER,
                "!",
                FontId::new(16.0, theme::semibold()),
                BAD,
            );
            painter.text(
                inner.center() + vec2(0.0, 12.0),
                Align2::CENTER_CENTER,
                "Can't show",
                FontId::proportional(10.0),
                BAD.gamma_multiply(0.9),
            );
            reason = Some(why);
        }
        None => {
            let t = ui.input(|i| i.time) as f32;
            let pulse = 0.35 + 0.15 * (t * 3.0 + rect.left() * 0.02).sin();
            painter.rect_filled(
                inner.shrink(14.0),
                CornerRadius::same(4),
                BORDER.gamma_multiply(pulse),
            );
        }
    }
    if entry.kind == Target::Vector {
        painter.text(
            image_rect.left_bottom() + vec2(6.0, -5.0),
            Align2::LEFT_BOTTOM,
            "SVG",
            FontId::proportional(9.0),
            WEAK.gamma_multiply(0.8),
        );
    }
    if let Some(over) = changed {
        let color = match (problem.is_some(), over.is_experimental()) {
            (true, _) => BAD,
            (false, true) => WARN,
            (false, false) => ACCENT,
        };
        painter.circle_filled(image_rect.right_top() + vec2(-9.0, 9.0), 4.0, color);
    }
    let stroke = if selected {
        Stroke::new(2.0, ACCENT)
    } else if marked || hovering_files && response.contains_pointer() {
        Stroke::new(2.0, ACCENT.gamma_multiply(0.6))
    } else {
        Stroke::new(1.0, BORDER)
    };
    painter.rect_stroke(
        image_rect,
        CornerRadius::same(6),
        stroke,
        StrokeKind::Inside,
    );
    let label_rect = Rect::from_min_max(pos2(rect.left(), image_rect.bottom()), rect.max);
    let galley = painter.layout(
        entry.name().to_string(),
        FontId::proportional(10.5),
        if selected { TEXT } else { WEAK },
        f32::INFINITY,
    );
    let galley = if galley.size().x > size {
        let mut job = egui::text::LayoutJob::simple_singleline(
            entry.name().to_string(),
            FontId::proportional(10.5),
            if selected { TEXT } else { WEAK },
        );
        job.wrap = egui::text::TextWrapping::truncate_at_width(size);
        painter.layout_job(job)
    } else {
        galley
    };
    painter.galley(
        pos2(
            label_rect.center().x - galley.size().x / 2.0,
            label_rect.top() + 3.0,
        ),
        galley,
        TEXT,
    );
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.on_hover_ui(|ui| {
        ui.label(RichText::new(entry.rel()).monospace().size(11.0));
        if changed.is_some() {
            ui.label(RichText::new("Changed by you").color(ACCENT));
        }
        if let Some(why) = problem {
            ui.colored_label(BAD, format!("Not used in the last build: {why}"));
        }
        if let Some(why) = &reason {
            ui.colored_label(BAD, format!("DeadTune can't show this image: {why}"));
        }
    })
}

/// Asks for the selected image's large pictures first, ahead of the tiles.
fn preview_pictures(state: &AppState, entry: &ImageEntry, thumbs: &mut Thumbs) {
    if let Some(yours) = state.your_picture(&entry.path, PREVIEW_SIDE) {
        thumbs.get(&yours);
    }
    thumbs.get(&game_preview(entry));
}

fn game_preview(entry: &ImageEntry) -> Picture {
    Picture::Game {
        path: entry.path.clone(),
        side: PREVIEW_SIDE,
    }
}

fn checker(painter: &egui::Painter, rect: Rect, thumbs: &Thumbs) {
    let cells = rect.size() / 8.0;
    painter.image(
        thumbs.checker.id(),
        rect,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(cells.x / 2.0, cells.y / 2.0)),
        Color32::WHITE,
    );
}

/// A square on a checkerboard with `picture` fitted (or at its own size for `Actual`).
fn picture_box(
    ui: &mut Ui,
    thumbs: &mut Thumbs,
    picture: &Picture,
    side: f32,
    zoom: Zoom,
) -> Option<crate::images::Facts> {
    let (rect, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
    let painter = ui.painter_at(rect);
    checker(&painter, rect, thumbs);
    painter.rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(1.0, BORDER),
        StrokeKind::Inside,
    );
    match thumbs.get(picture) {
        Some(Slot::Ready { texture, facts }) => {
            let size = texture.size_vec2();
            let natural =
                vec2(facts.width as f32, facts.height as f32) / ui.ctx().pixels_per_point();
            let shown = match zoom {
                Zoom::Fit => size * ((side - 16.0) / size.x).min((side - 16.0) / size.y),
                Zoom::Actual => natural,
            };
            painter.image(
                texture.id(),
                Rect::from_center_size(rect.center(), shown),
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            Some(facts)
        }
        Some(Slot::Failed(why)) => {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Can't show",
                FontId::proportional(11.5),
                BAD,
            );
            ui.label(RichText::new(why).size(11.0).color(WEAK));
            None
        }
        None => {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Loading",
                FontId::proportional(11.5),
                WEAK,
            );
            None
        }
    }
}

fn side_panel(
    ui: &mut Ui,
    state: &AppState,
    entry: Option<&ImageEntry>,
    thumbs: &mut Thumbs,
    replace_path: &mut String,
    edits: &mut Vec<Edit>,
) {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(theme::RADIUS))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            egui::ScrollArea::vertical()
                .id_salt("images_panel")
                .auto_shrink(false)
                .show(ui, |ui| match entry {
                    Some(entry) => selected(ui, state, entry, thumbs, replace_path, edits),
                    None => nothing_selected(ui, state),
                });
        });
}

fn nothing_selected(ui: &mut Ui, state: &AppState) {
    ui.add_space(8.0);
    ui.label(
        RichText::new("Pick an image")
            .size(15.0)
            .family(theme::semibold()),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new("Click any tile to see it large, save a copy, or replace it with your own.")
            .color(WEAK),
    );
    ui.add_space(10.0);
    widgets::caption(ui, "How replacing works");
    for line in [
        "Drag a PNG or SVG file from your computer onto a tile.",
        "Pictures take a PNG. Vector icons take an SVG.",
        "Press Apply, then start Deadlock to see it.",
        "Reset puts the game's image back.",
    ] {
        ui.label(RichText::new(format!("· {line}")).size(12.0).color(WEAK));
    }
    notice(ui, state, "");
}

fn selected(
    ui: &mut Ui,
    state: &AppState,
    entry: &ImageEntry,
    thumbs: &mut Thumbs,
    replace_path: &mut String,
    edits: &mut Vec<Edit>,
) {
    let width = ui.available_width();
    ui.label(
        RichText::new(entry.name())
            .size(15.0)
            .family(theme::semibold())
            .color(TEXT),
    );
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add(egui::Button::new(RichText::new("Copy").size(11.0)).small())
                .on_hover_text("Copy the full game path")
                .clicked()
            {
                ui.ctx().copy_text(entry.path.clone());
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(entry.rel())
                            .monospace()
                            .size(10.5)
                            .color(WEAK),
                    )
                    .truncate(),
                )
                .on_hover_text(&entry.path);
            });
        });
    });
    ui.add_space(6.0);
    let box_side = width.min((ui.ctx().content_rect().height() * 0.27).max(150.0));
    let over = state.image_override(&entry.path);
    let yours = state.your_picture(&entry.path, PREVIEW_SIDE);
    let facts = match &yours {
        Some(yours) => {
            let half = ((width - 10.0) / 2.0).min(box_side);
            let mut facts = None;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                ui.vertical(|ui| {
                    ui.set_width(half);
                    widgets::caption(ui, "Game's");
                    facts = picture_box(ui, thumbs, &game_preview(entry), half, Zoom::Fit);
                });
                ui.vertical(|ui| {
                    ui.set_width(half);
                    widgets::caption(ui, "Yours");
                    picture_box(ui, thumbs, yours, half, Zoom::Fit);
                });
            });
            facts
        }
        None if matches!(thumbs.get(&game_preview(entry)), Some(Slot::Failed(_))) => picture_box(
            ui,
            thumbs,
            &game_preview(entry),
            box_side.min(140.0),
            Zoom::Fit,
        ),
        None => {
            ui.vertical_centered(|ui| {
                picture_box(
                    ui,
                    thumbs,
                    &game_preview(entry),
                    box_side,
                    state.images.zoom,
                )
            })
            .inner
        }
    };
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        let kind = match entry.kind {
            Target::Vector => "Vector icon",
            Target::Raster => "Picture",
        };
        widgets::badge(
            ui,
            kind,
            if entry.kind == Target::Vector {
                GOOD
            } else {
                ACCENT
            },
        );
        if let Some(f) = &facts {
            ui.label(
                RichText::new(format!("{} x {} · {}", f.width, f.height, f.format))
                    .size(11.5)
                    .color(WEAK),
            );
        }
        if yours.is_none() && facts.is_some() {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let zoom = match state.images.zoom {
                    Zoom::Fit => 1,
                    Zoom::Actual => 0,
                };
                if let Some(i) = widgets::segmented(ui, &["1:1", "Fit"], zoom) {
                    edits.push(Edit::Zoom([Zoom::Actual, Zoom::Fit][i]));
                }
            });
        }
    });
    if let Some(over) = over {
        changed_block(ui, state, entry, over, edits);
    }
    colour(edits, |actions| {
        edit_view::section(ui, state, entry, actions)
    });
    ui.add_space(10.0);
    replace_block(ui, state, entry, replace_path, edits);
    ui.add_space(10.0);
    widgets::caption(ui, "Save a copy");
    ui.horizontal_wrapped(|ui| {
        if ui.button("Export PNG").clicked() {
            edits.push(Edit::Export(entry.path.clone(), ExportKind::Png));
        }
        if entry.kind == Target::Vector && ui.button("Export SVG").clicked() {
            edits.push(Edit::Export(entry.path.clone(), ExportKind::Svg));
        }
        if state.images.exported.is_some()
            && ui
                .button("Open folder")
                .on_hover_text(state.exports_dir().display().to_string())
                .clicked()
        {
            edits.push(Edit::OpenExports);
        }
    });
    notice(ui, state, &entry.path);
}

fn changed_block(
    ui: &mut Ui,
    state: &AppState,
    entry: &ImageEntry,
    over: &IconOverride,
    edits: &mut Vec<Edit>,
) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Changed by you").color(ACCENT).size(12.0));
        if widgets::reset_pill(ui)
            .on_hover_text("Back to the game's own image after Apply")
            .clicked()
        {
            edits.push(Edit::Reset(entry.path.clone()));
        }
    });
    if over.is_experimental() {
        ui.label(
            RichText::new(
                "Test: a PNG inside a vector icon may not show in game. Use an SVG to be sure.",
            )
            .size(11.5)
            .color(WARN),
        );
    }
    if let Some(why) = state.image_problem(&entry.path) {
        ui.label(
            RichText::new(format!(
                "This replacement can't be used right now: {why}. Reset it or drop your file again."
            ))
            .size(11.5)
            .color(BAD),
        );
    }
}

fn replace_block(
    ui: &mut Ui,
    state: &AppState,
    entry: &ImageEntry,
    path: &mut String,
    edits: &mut Vec<Edit>,
) {
    widgets::caption(ui, "Replace");
    let hint = match entry.kind {
        Target::Raster => "Drop a PNG on this window, or paste the file's path below.",
        Target::Vector => {
            "Drop an SVG on this window, or paste the file's path below. A PNG works too, as a test."
        }
    };
    ui.label(RichText::new(hint).size(12.0).color(WEAK));
    if entry.kind == Target::Raster {
        let current = match state.images.fit {
            Fit::Original => 0,
            Fit::Own => 1,
        };
        ui.horizontal(|ui| {
            if let Some(i) = widgets::segmented(
                ui,
                &["Fit to original size", "Keep my image's size"],
                current,
            ) {
                edits.push(Edit::Fit([Fit::Original, Fit::Own][i]));
            }
        });
    }
    ui.horizontal(|ui| {
        let response = ui.add(
            egui::TextEdit::singleline(path)
                .hint_text("Path to your PNG or SVG file")
                .desired_width(ui.available_width() - 74.0),
        );
        let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (ui
            .add_enabled(!path.trim().is_empty(), egui::Button::new("Replace"))
            .clicked()
            || enter)
            && !path.trim().is_empty()
        {
            edits.push(Edit::ReplaceFromPath);
        }
    });
}

fn notice(ui: &mut Ui, state: &AppState, path: &str) {
    let Some(n) = state.images.notice.as_ref().filter(|n| n.path == path) else {
        return;
    };
    let color = match n.tone {
        Tone::Good => GOOD,
        Tone::Warn => WARN,
        Tone::Bad => BAD,
    };
    ui.add_space(8.0);
    egui::Frame::new()
        .fill(color.gamma_multiply(0.12))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(&n.text).size(12.0).color(color));
        });
}
