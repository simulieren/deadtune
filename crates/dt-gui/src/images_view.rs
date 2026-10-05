//! UI images page: a toolbar (search, filters, tile size, add and export), a folder column,
//! a virtualised thumbnail grid with picking, and the selected image's panel (preview,
//! replace, fit in the slot, colours, save a copy). Files dropped anywhere on the page or
//! chosen in the file dialog go through `crate::images_collect`; pictures decode on
//! `crate::thumbs`' workers; every change is a method on the state.

use std::path::PathBuf;
use std::time::Instant;

use dt_core::hud::icons::{IconOverride, Target};
use dt_core::texture::encode::Fit;
use dt_core::texture::frame::{self, Crop};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Key, Layout, Margin, Modifiers, Rect,
    RichText, Sense, Shape, Stroke, StrokeKind, Ui, UiBuilder, Vec2, pos2, vec2,
};

use crate::file_pick::Picking;
use crate::icons::{self, Icon};
use crate::images::{ExportKind, ImageEntry, Library, Picture, TileSize, Tone, Zoom};
use crate::images_collect::{ImportReview, Incoming};
use crate::images_edit_view::{self as edit_view, Action};
use crate::state::{AppState, Status};
use crate::theme::{
    self, ACCENT, BAD, BG, BORDER, CARD, CARD_HOVER, GOOD, ON_ACCENT, RAIL, TEXT, WARN, WEAK,
};
use crate::thumbs::{Slot, Thumbs};
use crate::views::open_external;
use crate::widgets;

const LABEL: f32 = 18.0;
const GAP: f32 = 8.0;
const PANEL: f32 = 340.0;
const FOLDERS: f32 = 170.0;
const PREVIEW_SIDE: u32 = 512;
/// The position editor's size range, relative to the picture fitting inside the slot.
const SIZE_RANGE: std::ops::RangeInclusive<i32> = 25..=800;

enum Edit {
    Select(String, Modifiers),
    Pick(String),
    PickAll,
    ClearPicks,
    Colour(Action),
    Folder(Option<String>),
    ChangedOnly(bool),
    Tile(TileSize),
    Fit(Fit),
    Crop(String, Crop),
    CommitCrop,
    Zoom(Zoom),
    Reset(String),
    ResetAll,
    Choose,
    Export(String, ExportKind),
    ExportPicked(Vec<String>),
    ExportChanges,
    ConfirmImport,
    CancelImport,
    Dismiss,
    Open(PathBuf),
}

pub fn page(ui: &mut Ui, state: &mut AppState) {
    state.load_images();
    state.poll_picker();
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
    keys(ui, state, &mut edits);
    let area = ui.available_rect_before_wrap();
    toolbar(ui, state, &library, &mut edits);
    draw_body(ui, state, &library, &mut thumbs, &mut edits);
    drop_overlay(ui, state, &library, area);
    thumbs.end_frame();
    state.images.thumbs = Some(thumbs);
    state.images.library = Some(Ok(library));
    let dropped: Vec<PathBuf> = ui.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .map(|f| f.path().to_path_buf())
            .collect()
    });
    if !dropped.is_empty() {
        state.receive_files(dropped.iter().map(|p| Incoming::from_path(p)).collect());
    }
    run(ui.ctx(), state, edits);
}

/// Collects the colour panel's actions as page edits.
fn colour(edits: &mut Vec<Edit>, draw: impl FnOnce(&mut Vec<Action>)) {
    let mut actions = Vec::new();
    draw(&mut actions);
    edits.extend(actions.into_iter().map(Edit::Colour));
}

/// Ctrl+A picks every image shown and Escape drops the picks, unless a text field types.
fn keys(ui: &mut Ui, state: &AppState, edits: &mut Vec<Edit>) {
    if ui.memory(|m| m.focused().is_some()) {
        return;
    }
    ui.input_mut(|i| {
        if i.consume_key(Modifiers::COMMAND, Key::A) {
            edits.push(Edit::PickAll);
        }
        if !state.images.edit.marked.is_empty() && i.consume_key(Modifiers::NONE, Key::Escape) {
            edits.push(Edit::ClearPicks);
        }
    });
}

fn run(ctx: &egui::Context, state: &mut AppState, edits: Vec<Edit>) {
    for edit in edits {
        match edit {
            Edit::Select(path, keys) => {
                let visible = state.visible_paths();
                state.mark_image(&path, keys.command, keys.shift, &visible);
            }
            Edit::Pick(path) => {
                let visible = state.visible_paths();
                state.mark_image(&path, true, false, &visible);
            }
            Edit::PickAll => state.mark_all_visible(),
            Edit::ClearPicks => state.clear_marks(),
            Edit::Colour(action) => edit_view::run(state, action),
            Edit::Folder(folder) => state.images.folder = folder,
            Edit::ChangedOnly(on) => state.images.changed_only = on,
            Edit::Tile(size) => state.images.tile = size,
            Edit::Fit(fit) => state.set_image_fit(fit),
            Edit::Crop(path, crop) => {
                state.drag_crop(&path, crop, Instant::now());
                ctx.request_repaint_after(crate::images_edit::SETTLE);
            }
            Edit::CommitCrop => state.commit_edits(),
            Edit::Zoom(zoom) => state.images.zoom = zoom,
            Edit::Reset(path) => state.reset_image(&path),
            Edit::ResetAll => state.reset_all_images(),
            Edit::Choose => {
                if state.images.picker.is_none() {
                    state.images.picker = Some(Picking::open(ctx));
                }
            }
            Edit::Export(path, kind) => {
                let _ = state.export_image(&path, kind);
            }
            Edit::ExportPicked(paths) => state.export_images(&paths),
            Edit::ExportChanges => {
                let _ = state.export_changes();
            }
            Edit::ConfirmImport => state.confirm_import(),
            Edit::CancelImport => state.cancel_import(),
            Edit::Dismiss => state.images.notice = None,
            Edit::Open(path) => {
                if let Err(e) = open_external(&path) {
                    state.status = Some(Status::Error(format!("open {}: {e}", path.display())));
                }
            }
        }
    }
}

fn small(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).size(12.0))
}

/// A button with a painted icon before its label.
fn icon_button(ui: &mut Ui, icon: Icon, text: &str, enabled: bool) -> egui::Response {
    let font = FontId::proportional(12.0);
    let width = ui
        .painter()
        .layout_no_wrap(text.to_string(), font.clone(), TEXT)
        .size()
        .x;
    let (rect, response) = ui.allocate_exact_size(
        vec2(width + 38.0, 26.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let hovered = enabled && response.hovered();
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(6),
        if hovered { CARD_HOVER } else { CARD },
        Stroke::new(1.0, BORDER),
        StrokeKind::Inside,
    );
    let color = if !enabled {
        WEAK.gamma_multiply(0.6)
    } else if hovered {
        TEXT
    } else {
        TEXT.gamma_multiply(0.9)
    };
    icons::paint(
        painter,
        Rect::from_center_size(rect.left_center() + vec2(16.0, 0.0), vec2(14.0, 14.0)),
        icon,
        color,
    );
    painter.text(
        rect.left_center() + vec2(28.0, 0.0),
        Align2::LEFT_CENTER,
        text,
        font,
        color,
    );
    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

fn toolbar(ui: &mut Ui, state: &mut AppState, library: &Library, edits: &mut Vec<Edit>) {
    let changed = state.images_changed_count();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        search(ui, &mut state.images.search);
        let label = format!("Changed by me  {changed}");
        if widgets::chip(ui, &label, ACCENT, Some(state.images.changed_only))
            .on_hover_text("Show only the images you changed")
            .clicked()
        {
            edits.push(Edit::ChangedOnly(!state.images.changed_only));
        }
        if let Some(size) = tile_size_switch(ui, state.images.tile) {
            edits.push(Edit::Tile(size));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            export_menu(ui, state, library, edits);
            let picking = state.images.picker.is_some();
            if icon_button(ui, Icon::Upload, "Add files", !picking)
                .on_hover_text(
                    "Choose PNG, SVG or zip files. One file replaces the selected image; several, or a zip, import by name. You can also drag files and folders onto this page.",
                )
                .clicked()
            {
                edits.push(Edit::Choose);
            }
            ui.add_space(6.0);
            if ui
                .add_enabled(state.can_redo(), small("Redo"))
                .on_hover_text("Ctrl+Shift+Z")
                .clicked()
            {
                edits.push(Edit::Colour(Action::Redo));
            }
            if ui
                .add_enabled(state.can_undo(), small("Undo"))
                .on_hover_text("Ctrl+Z")
                .clicked()
            {
                edits.push(Edit::Colour(Action::Undo));
            }
        });
    });
    ui.add_space(4.0);
    if state.images.export_all.job.is_some() || state.images.export_all.last.is_some() {
        crate::images_export_view::progress(ui, state, Some(&library.source));
    }
}

fn export_menu(ui: &mut Ui, state: &mut AppState, library: &Library, edits: &mut Vec<Edit>) {
    let response = icon_button(ui, Icon::Download, "Export", true);
    egui::Popup::menu(&response).show(|ui| {
        ui.set_min_width(250.0);
        let changed = state.images_changed_count();
        if ui
            .add_enabled(changed > 0, egui::Button::new("My changes as a collection (.zip)"))
            .on_hover_text(
                "Every image you changed, finished as the game will draw it, in one zip under the game's paths. Drop it on this page, here or on another PC, to import it.",
            )
            .on_disabled_hover_text("Change an image first")
            .clicked()
        {
            ui.close();
            edits.push(Edit::ExportChanges);
        }
        let picked = state.marked_or_selected();
        if ui
            .add_enabled(
                !picked.is_empty(),
                egui::Button::new(match picked.len() {
                    0 | 1 => "Selected image as the game has it".to_string(),
                    n => format!("{n} picked images as the game has them"),
                }),
            )
            .on_disabled_hover_text("Select an image first")
            .clicked()
        {
            ui.close();
            edits.push(Edit::ExportPicked(picked));
        }
        ui.separator();
        if state.images.export_all.job.is_none() {
            let folder = state.images.folder.clone();
            crate::images_export_view::controls(
                ui,
                state,
                Some(&library.source),
                folder.as_deref(),
            );
        }
        ui.separator();
        if ui.button("Open the exports folder").clicked() {
            ui.close();
            edits.push(Edit::Open(state.exports_dir()));
        }
    });
}

/// Three squares of growing size; returns the one clicked when it is not already on.
fn tile_size_switch(ui: &mut Ui, current: TileSize) -> Option<TileSize> {
    let mut picked = None;
    let (outer, _) = ui.allocate_exact_size(vec2(3.0 * 24.0 + 4.0, 24.0), Sense::hover());
    ui.painter().rect_filled(outer, CornerRadius::same(5), RAIL);
    for (i, size) in TileSize::ALL.into_iter().enumerate() {
        let rect = Rect::from_min_size(
            pos2(outer.left() + 2.0 + i as f32 * 24.0, outer.top() + 2.0),
            vec2(24.0, 20.0),
        );
        let response = ui
            .interact(rect, ui.id().with(("tile_size", i)), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(match size {
                TileSize::Small => "Small tiles",
                TileSize::Medium => "Medium tiles",
                TileSize::Large => "Large tiles",
            });
        let on = size == current;
        if on {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(4), CARD_HOVER);
        }
        let side = 7.0 + i as f32 * 3.0;
        ui.painter().rect_stroke(
            Rect::from_center_size(rect.center(), vec2(side, side)),
            CornerRadius::same(2),
            Stroke::new(1.3, if on || response.hovered() { TEXT } else { WEAK }),
            StrokeKind::Middle,
        );
        if response.clicked() && !on {
            picked = Some(size);
        }
    }
    picked
}

fn search(ui: &mut Ui, query: &mut String) {
    let (rect, _) = ui.allocate_exact_size(vec2(260.0, 26.0), Sense::hover());
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

fn draw_body(
    ui: &mut Ui,
    state: &AppState,
    library: &Library,
    thumbs: &mut Thumbs,
    edits: &mut Vec<Edit>,
) {
    if let Some(review) = &state.images.import {
        import_card(ui, review, edits);
    } else {
        page_notice(ui, state, edits);
    }
    ui.add_space(8.0);
    let area = ui.available_rect_before_wrap();
    let panel = PANEL.min(area.width() * 0.38);
    let folders = if area.width() > 900.0 { FOLDERS } else { 0.0 };
    let folder_rect = Rect::from_min_max(area.min, pos2(area.left() + folders, area.bottom()));
    let grid_left = area.left() + folders + if folders > 0.0 { 14.0 } else { 0.0 };
    let grid_rect = Rect::from_min_max(
        pos2(grid_left, area.top()),
        pos2(area.right() - panel - 14.0, area.bottom()),
    );
    let panel_rect = Rect::from_min_max(pos2(area.right() - panel, area.top()), area.max);
    ui.allocate_rect(area, Sense::hover());
    let visible = state.images.visible(library, &state.profile.hud.icons);
    let preview = state
        .images
        .selected
        .as_deref()
        .and_then(|p| library.entry(p));
    if let Some(entry) = preview {
        preview_pictures(state, entry, thumbs);
    }
    if folders > 0.0 {
        ui.scope_builder(UiBuilder::new().max_rect(folder_rect), |ui| {
            folder_column(ui, state, library, edits);
        });
    }
    ui.scope_builder(UiBuilder::new().max_rect(grid_rect), |ui| {
        if folders == 0.0 {
            folder_combo(ui, state, library, edits);
        }
        grid_header(ui, state, library, &visible, edits);
        grid(ui, state, &visible, thumbs, edits);
    });
    ui.scope_builder(UiBuilder::new().max_rect(panel_rect), |ui| {
        side_panel(ui, state, preview, thumbs, edits);
    });
}

fn import_card(ui: &mut Ui, review: &ImportReview, edits: &mut Vec<Edit>) {
    let matched = review.plan.matched.len();
    let unmatched: Vec<_> = review.plan.unmatched().collect();
    egui::Frame::new()
        .fill(ACCENT.gamma_multiply(0.08))
        .stroke(Stroke::new(1.0, ACCENT.gamma_multiply(0.5)))
        .corner_radius(CornerRadius::same(theme::RADIUS))
        .inner_margin(Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                icons::paint(
                    ui.painter(),
                    Rect::from_min_size(ui.cursor().min + vec2(0.0, 2.0), vec2(16.0, 16.0)),
                    Icon::Upload,
                    ACCENT,
                );
                ui.add_space(22.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(format!("Import “{}”", review.title))
                            .size(14.0)
                            .family(theme::semibold())
                            .color(TEXT),
                    );
                    let mut line = format!(
                        "{matched} {} the game's images by name",
                        if matched == 1 {
                            "file matches"
                        } else {
                            "files match"
                        }
                    );
                    if review.replaces_changes > 0 {
                        line.push_str(&format!(
                            ", {} of them over images you already changed",
                            review.replaces_changes
                        ));
                    }
                    line.push('.');
                    if !unmatched.is_empty() {
                        line.push_str(&format!(
                            " {} {} left out.",
                            unmatched.len(),
                            if unmatched.len() == 1 {
                                "file is"
                            } else {
                                "files are"
                            }
                        ));
                    }
                    line.push_str(
                        " Nothing changes until you press Import, and Undo takes it back.",
                    );
                    ui.add(egui::Label::new(RichText::new(line).size(12.0).color(WEAK)).wrap());
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let label = format!(
                            "Import {matched} {}",
                            if matched == 1 { "image" } else { "images" }
                        );
                        let import = egui::Button::new(
                            RichText::new(label).size(12.5).strong().color(ON_ACCENT),
                        )
                        .fill(ACCENT)
                        .min_size(vec2(0.0, 26.0));
                        if ui.add(import).clicked() {
                            edits.push(Edit::ConfirmImport);
                        }
                        if ui
                            .add(egui::Button::new("Cancel").min_size(vec2(0.0, 26.0)))
                            .clicked()
                        {
                            edits.push(Edit::CancelImport);
                        }
                    });
                    {
                        egui::CollapsingHeader::new(
                            RichText::new("Show the files").size(12.0).color(WEAK),
                        )
                        .id_salt("images_import_files")
                        .show(ui, |ui| {
                            for m in review.plan.matched.iter().take(8) {
                                ui.label(
                                    RichText::new(format!(
                                        "{}  →  {}",
                                        m.file.name,
                                        &m.game_path[dt_core::hud::icons::IMAGES_ROOT.len()..]
                                    ))
                                    .size(11.0)
                                    .color(TEXT),
                                );
                            }
                            if matched > 8 {
                                ui.label(
                                    RichText::new(format!("and {} more", matched - 8))
                                        .size(11.0)
                                        .color(WEAK),
                                );
                            }
                            for (name, why) in unmatched.iter().take(8) {
                                ui.label(
                                    RichText::new(format!("{name}: {}", why.reason()))
                                        .size(11.0)
                                        .color(WARN),
                                );
                            }
                        });
                    }
                });
            });
        });
}

fn page_notice(ui: &mut Ui, state: &AppState, edits: &mut Vec<Edit>) {
    let Some(n) = state.images.notice.as_ref().filter(|n| n.path.is_empty()) else {
        return;
    };
    let color = tone_color(n.tone);
    egui::Frame::new()
        .fill(color.gamma_multiply(0.10))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add(egui::Button::new(RichText::new("Dismiss").size(11.5)).small())
                        .clicked()
                    {
                        edits.push(Edit::Dismiss);
                    }
                    if let Some(path) = &state.images.exported
                        && n.tone != Tone::Bad
                        && ui
                            .add(egui::Button::new(RichText::new("Open folder").size(11.5)).small())
                            .clicked()
                    {
                        let dir = if path.is_dir() {
                            path.clone()
                        } else {
                            path.parent()
                                .map_or_else(|| path.clone(), |p| p.to_path_buf())
                        };
                        edits.push(Edit::Open(dir));
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.add(
                            egui::Label::new(RichText::new(&n.text).size(12.0).color(color)).wrap(),
                        );
                    });
                });
            });
        });
}

fn tone_color(tone: Tone) -> Color32 {
    match tone {
        Tone::Good => GOOD,
        Tone::Warn => WARN,
        Tone::Bad => BAD,
    }
}

fn folder_row(ui: &mut Ui, text: &str, count: usize, on: bool, indent: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
    let painter = ui.painter();
    if on {
        painter.rect_filled(rect, CornerRadius::same(5), CARD_HOVER);
        painter.rect_filled(
            Rect::from_min_size(rect.min + vec2(0.0, 5.0), vec2(2.5, 14.0)),
            CornerRadius::same(1),
            ACCENT,
        );
    } else if response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(5), CARD);
    }
    let count_text = count.to_string();
    let count_width = painter
        .layout_no_wrap(count_text.clone(), FontId::proportional(10.5), WEAK)
        .size()
        .x;
    let mut job = egui::text::LayoutJob::simple_singleline(
        text.to_string(),
        FontId::proportional(12.5),
        if on { TEXT } else { TEXT.gamma_multiply(0.82) },
    );
    job.wrap =
        egui::text::TextWrapping::truncate_at_width(rect.width() - 30.0 - indent - count_width);
    let galley = painter.layout_job(job);
    painter.galley(
        pos2(
            rect.left() + 10.0 + indent,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        TEXT,
    );
    painter.text(
        rect.right_center() - vec2(8.0, 0.0),
        Align2::RIGHT_CENTER,
        count_text,
        FontId::proportional(10.5),
        if on {
            ACCENT
        } else {
            WEAK.gamma_multiply(0.85)
        },
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn folder_column(ui: &mut Ui, state: &AppState, library: &Library, edits: &mut Vec<Edit>) {
    let current = state.images.folder.as_deref();
    let top = current.map(|f| f.split('/').next().unwrap_or(f));
    let footer = 34.0;
    let list = Rect::from_min_max(
        ui.max_rect().min,
        pos2(ui.max_rect().right(), ui.max_rect().bottom() - footer),
    );
    ui.scope_builder(UiBuilder::new().max_rect(list), |ui| {
        widgets::caption(ui, "Folders");
        egui::ScrollArea::vertical()
            .id_salt("images_folders")
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                if folder_row(
                    ui,
                    "All images",
                    library.entries.len(),
                    current.is_none(),
                    0.0,
                )
                .clicked()
                {
                    edits.push(Edit::Folder(None));
                }
                for folder in library.top_folders() {
                    let open = top == Some(folder.path.as_str());
                    if folder_row(
                        ui,
                        folder.name(),
                        folder.count,
                        current == Some(folder.path.as_str()),
                        0.0,
                    )
                    .clicked()
                    {
                        edits.push(Edit::Folder(Some(folder.path.clone())));
                    }
                    if !open {
                        continue;
                    }
                    for sub in library.subfolders(&folder.path) {
                        let on = current == Some(sub.path.as_str());
                        if folder_row(ui, sub.name(), sub.count, on, 12.0).clicked() {
                            let next = if on {
                                folder.path.clone()
                            } else {
                                sub.path.clone()
                            };
                            edits.push(Edit::Folder(Some(next)));
                        }
                    }
                }
            });
    });
    let vectors = library
        .entries
        .iter()
        .filter(|e| e.kind == Target::Vector)
        .count();
    let foot = Rect::from_min_max(
        pos2(ui.max_rect().left(), ui.max_rect().bottom() - footer + 6.0),
        ui.max_rect().max,
    );
    ui.scope_builder(UiBuilder::new().max_rect(foot), |ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        let line = |text: String| egui::Label::new(RichText::new(text).size(10.5).color(WEAK));
        ui.add(line(format!(
            "{} images, {vectors} vector",
            library.entries.len()
        )));
        ui.add(line(library.source.describe()).truncate())
            .on_hover_text(library.source.describe());
    });
}

/// The folder list as one drop-down, for windows too narrow for the column.
fn folder_combo(ui: &mut Ui, state: &AppState, library: &Library, edits: &mut Vec<Edit>) {
    let current = state.images.folder.clone();
    egui::ComboBox::from_id_salt("images_folder_combo")
        .width(220.0)
        .selected_text(current.clone().unwrap_or_else(|| "All images".into()))
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(current.is_none(), "All images")
                .clicked()
            {
                edits.push(Edit::Folder(None));
            }
            for folder in &library.folders {
                let on = current.as_deref() == Some(folder.path.as_str());
                if ui
                    .selectable_label(on, format!("{}  {}", folder.path, folder.count))
                    .clicked()
                {
                    edits.push(Edit::Folder(Some(folder.path.clone())));
                }
            }
        });
    ui.add_space(6.0);
}

/// What the grid shows, or what is picked and what can be done to the picks.
fn grid_header(
    ui: &mut Ui,
    state: &AppState,
    library: &Library,
    visible: &[&ImageEntry],
    edits: &mut Vec<Edit>,
) {
    let picked = state.images.edit.marked.len();
    let height = 30.0;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    let ui = &mut child;
    ui.spacing_mut().item_spacing.x = 8.0;
    if picked == 0 {
        let place = state.images.folder.as_deref().unwrap_or("All images");
        ui.label(
            RichText::new(place)
                .size(13.0)
                .family(theme::semibold())
                .color(TEXT),
        );
        ui.label(
            RichText::new(format!(
                "{} {}",
                visible.len(),
                if visible.len() == 1 {
                    "image"
                } else {
                    "images"
                }
            ))
            .size(12.0)
            .color(WEAK),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if !visible.is_empty()
                && ui
                    .add(small("Pick all"))
                    .on_hover_text("Pick every image shown (Ctrl+A)")
                    .clicked()
            {
                edits.push(Edit::PickAll);
            }
            if state.images.changed_only
                && state.images.folder.is_none()
                && state.images_changed_count() > 0
                && ui
                    .add(small("Reset all"))
                    .on_hover_text("Puts every image back to the game's own after Apply")
                    .clicked()
            {
                edits.push(Edit::ResetAll);
            }
            colour(edits, |actions| {
                edit_view::bulk_row(ui, state, library, actions)
            });
            ui.add(
                egui::Label::new(
                    RichText::new("Click to open · tick the circle or Ctrl-click to pick several")
                        .size(11.0)
                        .color(WEAK),
                )
                .truncate(),
            );
        });
        return;
    }
    let bar = rect;
    ui.painter()
        .rect_filled(bar, CornerRadius::same(6), ACCENT.gamma_multiply(0.12));
    ui.add_space(8.0);
    check_circle(
        ui.painter(),
        pos2(bar.left() + 16.0, bar.center().y),
        8.0,
        true,
    );
    ui.add_space(18.0);
    ui.label(
        RichText::new(format!("{picked} picked"))
            .size(12.5)
            .family(theme::semibold())
            .color(TEXT),
    );
    if picked < visible.len()
        && ui
            .add(small(&format!("Pick all {}", visible.len())))
            .clicked()
    {
        edits.push(Edit::PickAll);
    }
    if ui.add(small("Clear")).on_hover_text("Escape").clicked() {
        edits.push(Edit::ClearPicks);
    }
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let paths: Vec<String> = state.images.edit.marked.iter().cloned().collect();
        let changed: Vec<String> = paths
            .iter()
            .filter(|p| state.image_override(p).is_some())
            .cloned()
            .collect();
        if !changed.is_empty()
            && ui
                .add(small(&format!("Reset {}", changed.len())))
                .on_hover_text("Puts the picked images back to the game's own")
                .clicked()
        {
            edits.push(Edit::Colour(Action::Reset(changed)));
        }
        let op = state.current_color_op();
        if ui
            .add_enabled(!op.is_identity(), small("Apply colour"))
            .on_hover_text(format!("{} on every picked image", op.label()))
            .on_disabled_hover_text(
                "Open an image, set a colour in its panel, then apply it to the picks",
            )
            .clicked()
        {
            edits.push(Edit::Colour(Action::ApplyColor(paths.clone())));
        }
        if ui
            .add(small(&format!("Export {picked}")))
            .on_hover_text("Saves the picked images as the game has them, into the exports folder")
            .clicked()
        {
            edits.push(Edit::ExportPicked(paths));
        }
    });
}

fn check_circle(painter: &egui::Painter, centre: egui::Pos2, radius: f32, on: bool) {
    if on {
        painter.circle_filled(centre, radius, ACCENT);
        icons::paint(
            painter,
            Rect::from_center_size(centre, Vec2::splat(radius * 1.4)),
            Icon::Check,
            ON_ACCENT,
        );
    } else {
        painter.circle(
            centre,
            radius,
            Color32::from_black_alpha(110),
            Stroke::new(1.5, TEXT.gamma_multiply(0.85)),
        );
    }
}

fn grid(
    ui: &mut Ui,
    state: &AppState,
    visible: &[&ImageEntry],
    thumbs: &mut Thumbs,
    edits: &mut Vec<Edit>,
) {
    if visible.is_empty() {
        ui.add_space(30.0);
        ui.vertical_centered(|ui| {
            let text = if state.images.changed_only {
                "You haven't changed any images here yet."
            } else {
                "No images match."
            };
            ui.label(RichText::new(text).color(WEAK));
        });
        return;
    }
    let tile_side = state.images.tile.side();
    let width = ui.available_width();
    let columns = (((width + GAP) / (tile_side + GAP)).floor() as usize).max(1);
    let size = ((width - GAP * (columns - 1) as f32) / columns as f32).floor() - 1.0;
    let rows = visible.len().div_ceil(columns);
    let side = ((size - 12.0) * ui.ctx().pixels_per_point()).round() as u32;
    let side = side.next_power_of_two().min(256);
    let picking = !state.images.edit.marked.is_empty();
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
                        let (response, pick) = tile(ui, state, entry, size, side, thumbs, picking);
                        if pick {
                            edits.push(Edit::Pick(entry.path.clone()));
                        } else if response.clicked() {
                            let keys = ui.input(|i| i.modifiers);
                            edits.push(Edit::Select(entry.path.clone(), keys));
                        }
                    }
                });
            }
            for row in [first, last.saturating_sub(1)] {
                for entry in visible.iter().skip(row * columns).take(columns) {
                    thumbs.get(&state.preview_picture(&entry.path, side));
                }
            }
        });
}

/// One tile; the bool is a click on its pick circle.
fn tile(
    ui: &mut Ui,
    state: &AppState,
    entry: &ImageEntry,
    size: f32,
    side: u32,
    thumbs: &mut Thumbs,
    picking: bool,
) -> (egui::Response, bool) {
    let (rect, response) = ui.allocate_exact_size(vec2(size, size + LABEL), Sense::click());
    let selected = state.images.selected.as_deref() == Some(entry.path.as_str());
    let marked = state.images.edit.marked.contains(&entry.path);
    let changed = state.image_override(&entry.path);
    let problem = state.image_problem(&entry.path);
    let image_rect = Rect::from_min_size(rect.min, vec2(size, size));
    let circle = Rect::from_center_size(image_rect.left_top() + vec2(13.0, 13.0), vec2(22.0, 22.0));
    let pick = ui
        .interact(circle, ui.id().with(("pick", &entry.path)), Sense::click())
        .on_hover_text(if marked { "Unpick" } else { "Pick" });
    let painter = ui.painter();
    let fill = if response.hovered() || pick.hovered() {
        CARD_HOVER
    } else {
        CARD
    };
    painter.rect_filled(image_rect, CornerRadius::same(6), fill);
    if marked {
        painter.rect_filled(
            image_rect,
            CornerRadius::same(6),
            ACCENT.gamma_multiply(0.08),
        );
    }
    let slot = thumbs.get(&state.preview_picture(&entry.path, side));
    let inner = image_rect.shrink((size * 0.1).clamp(6.0, 12.0));
    let mut reason = None;
    match slot {
        Some(Slot::Ready { texture, .. }) => {
            let tex = texture.size_vec2();
            let scale = (inner.width() / tex.x).min(inner.height() / tex.y);
            let shown = Rect::from_center_size(inner.center(), tex * scale);
            painter.image(
                texture.id(),
                shown,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        Some(Slot::Failed(why)) => {
            painter.rect_filled(
                inner.shrink(size * 0.12),
                CornerRadius::same(4),
                BAD.gamma_multiply(0.14),
            );
            painter.text(
                inner.center(),
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
                inner.shrink(size * 0.15),
                CornerRadius::same(4),
                BORDER.gamma_multiply(pulse),
            );
        }
    }
    if entry.kind == Target::Vector && size >= 80.0 {
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
    if marked || picking || response.hovered() || pick.hovered() {
        check_circle(painter, circle.center(), 8.0, marked);
    }
    let stroke = if selected {
        Stroke::new(2.0, ACCENT)
    } else if marked {
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
    let mut job = egui::text::LayoutJob::simple_singleline(
        entry.name().to_string(),
        FontId::proportional(10.5),
        if selected { TEXT } else { WEAK },
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(size);
    let galley = painter.layout_job(job);
    painter.galley(
        pos2(
            rect.center().x - galley.size().x / 2.0,
            image_rect.bottom() + 3.0,
        ),
        galley,
        TEXT,
    );
    let clicked_pick = pick.clicked();
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let response = response.on_hover_ui(|ui| {
        ui.label(RichText::new(entry.rel()).monospace().size(11.0));
        if let Some((w, h)) = state.images.slots.get(&entry.path) {
            ui.label(RichText::new(format!("{w} x {h}")).size(11.0).color(WEAK));
        }
        if changed.is_some() {
            ui.label(RichText::new("Changed by you").color(ACCENT));
        }
        if let Some(why) = problem {
            ui.colored_label(BAD, format!("Not used in the last build: {why}"));
        }
        if let Some(why) = &reason {
            ui.colored_label(BAD, format!("DeadTune can't show this image: {why}"));
        }
    });
    (response, clicked_pick)
}

/// Asks for the selected image's large pictures first, ahead of the tiles.
fn preview_pictures(state: &AppState, entry: &ImageEntry, thumbs: &mut Thumbs) {
    thumbs.get(&state.preview_picture(&entry.path, PREVIEW_SIDE));
    thumbs.get(&game_preview(entry));
    if let Some(file) = state.stored_image(&entry.path) {
        thumbs.get(&source_picture(file));
    }
}

fn game_preview(entry: &ImageEntry) -> Picture {
    Picture::Game {
        path: entry.path.clone(),
        side: PREVIEW_SIDE,
    }
}

/// The player's file as it is, before it is placed in the slot.
fn source_picture(file: PathBuf) -> Picture {
    Picture::Mine {
        file,
        side: PREVIEW_SIDE,
        frame: None,
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
                    Some(entry) => selected(ui, state, entry, thumbs, edits),
                    None => nothing_selected(ui, state, edits),
                });
        });
}

fn hovering_files(ui: &Ui) -> bool {
    ui.input(|i| !i.raw.hovered_files.is_empty())
}

/// A dashed box that says what dropping a file does, with "Choose file" for those who'd
/// rather click.
fn drop_zone(ui: &mut Ui, state: &AppState, title: &str, sub: &str, edits: &mut Vec<Edit>) {
    let hot = hovering_files(ui);
    let width = ui.available_width();
    let galley = ui.painter().layout(
        sub.to_string(),
        FontId::proportional(11.0),
        WEAK,
        width - 58.0,
    );
    let height = 58.0 + galley.size().y;
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        CornerRadius::same(6),
        if hot {
            ACCENT.gamma_multiply(0.12)
        } else {
            RAIL.gamma_multiply(0.6)
        },
    );
    let color = if hot {
        ACCENT
    } else {
        BORDER.gamma_multiply(1.6)
    };
    let r = rect.shrink(0.5);
    let corners = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.left_top(),
    ];
    painter.extend(Shape::dashed_line(
        &corners,
        Stroke::new(1.2, color),
        5.0,
        4.0,
    ));
    icons::paint(
        painter,
        Rect::from_center_size(rect.left_center() + vec2(24.0, 0.0), vec2(20.0, 20.0)),
        Icon::Upload,
        if hot { ACCENT } else { WEAK },
    );
    painter.text(
        rect.left_top() + vec2(46.0, 14.0),
        Align2::LEFT_TOP,
        title,
        FontId::new(13.0, theme::semibold()),
        TEXT,
    );
    let text_bottom = rect.top() + 33.0 + galley.size().y;
    painter.galley(rect.left_top() + vec2(46.0, 33.0), galley, WEAK);
    let button = Rect::from_min_size(
        pos2(rect.left() + 44.0, text_bottom + 3.0),
        vec2(120.0, 18.0),
    );
    let waiting = state.images.picker.is_some();
    let label = if waiting {
        "Choosing…"
    } else {
        "or choose a file"
    };
    let response = ui.interact(button, ui.id().with("drop_zone_choose"), Sense::click());
    ui.painter().text(
        button.left_center() + vec2(2.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(11.5),
        if response.hovered() { TEXT } else { ACCENT },
    );
    if !waiting
        && response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
    {
        edits.push(Edit::Choose);
    }
}

fn nothing_selected(ui: &mut Ui, state: &AppState, edits: &mut Vec<Edit>) {
    ui.add_space(4.0);
    ui.label(
        RichText::new("Pick an image")
            .size(15.0)
            .family(theme::semibold()),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new("Click any tile to see it large, recolour it, save a copy, or put your own picture in its place.")
            .size(12.0)
            .color(WEAK),
    );
    ui.add_space(12.0);
    drop_zone(
        ui,
        state,
        "Import a collection",
        "A folder or .zip of PNG and SVG files named like the game's",
        edits,
    );
    ui.add_space(12.0);
    widgets::caption(ui, "How it works");
    for line in [
        "Open an image, then drop a PNG (pictures) or SVG (vector icons) on the page.",
        "Your picture is sized to the game's slot. Fit, Fill or drag it into place.",
        "Tick the circle on tiles to pick several; export or recolour them together.",
        "Export › My changes saves everything you changed as one zip to share.",
        "Press Apply, then start Deadlock to see it. Reset puts the game's image back.",
    ] {
        ui.label(RichText::new(format!("· {line}")).size(12.0).color(WEAK));
    }
}

fn selected(
    ui: &mut Ui,
    state: &AppState,
    entry: &ImageEntry,
    thumbs: &mut Thumbs,
    edits: &mut Vec<Edit>,
) {
    let width = ui.available_width();
    header(ui, state, entry, edits);
    ui.add_space(8.0);
    let box_side = width.min((ui.ctx().content_rect().height() * 0.24).max(140.0));
    let yours = state
        .is_edited(&entry.path)
        .then(|| state.preview_picture(&entry.path, PREVIEW_SIDE));
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
                    widgets::caption(ui, "Yours, in game");
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
    ui.horizontal(|ui| {
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
    notice(ui, state, &entry.path);
    ui.add_space(12.0);
    replace_block(ui, state, entry, edits);
    placement(ui, state, entry, thumbs, edits);
    ui.add_space(12.0);
    let shown = state.shown_adjustments(&entry.path);
    let title = if shown.is_empty() {
        "Edit colours".to_string()
    } else {
        format!("Edit colours · {}", shown.len())
    };
    egui::CollapsingHeader::new(
        RichText::new(title)
            .size(12.5)
            .family(theme::semibold())
            .color(TEXT),
    )
    .id_salt(("images_colours", &entry.path))
    .default_open(!shown.is_empty())
    .show(ui, |ui| {
        colour(edits, |actions| {
            edit_view::section(ui, state, entry, actions)
        });
    });
    ui.add_space(12.0);
    widgets::caption(ui, "Save a copy");
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("Export PNG")
            .on_hover_text("The game's own image, into DeadTune's exports folder")
            .clicked()
        {
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
            edits.push(Edit::Open(state.exports_dir()));
        }
    });
}

fn header(ui: &mut Ui, state: &AppState, entry: &ImageEntry, edits: &mut Vec<Edit>) {
    ui.horizontal(|ui| {
        ui.add(
            egui::Label::new(
                RichText::new(entry.name())
                    .size(15.0)
                    .family(theme::semibold())
                    .color(TEXT),
            )
            .truncate(),
        );
        if state.image_override(&entry.path).is_some() {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::reset_pill(ui)
                    .on_hover_text("Back to the game's own image after Apply")
                    .clicked()
                {
                    edits.push(Edit::Reset(entry.path.clone()));
                }
                widgets::badge(ui, "Changed", ACCENT);
            });
        }
    });
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
    let Some(over) = state.image_override(&entry.path) else {
        return;
    };
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

fn replace_block(ui: &mut Ui, state: &AppState, entry: &ImageEntry, edits: &mut Vec<Edit>) {
    widgets::caption(ui, "Replace");
    let slot = state.images.slots.get(&entry.path);
    let (title, sub) = match (entry.kind, slot) {
        (Target::Raster, Some((w, h))) => (
            "Drop a PNG here".to_string(),
            format!("It becomes {w} x {h}; this size or bigger looks sharpest"),
        ),
        (Target::Raster, None) => (
            "Drop a PNG here".to_string(),
            "It is sized to the game's image".to_string(),
        ),
        (Target::Vector, _) => (
            "Drop an SVG here".to_string(),
            "Any size, it scales. A PNG works too, as a test".to_string(),
        ),
    };
    drop_zone(ui, state, &title, &sub, edits);
}

fn fit_hint(fit: Fit) -> &'static str {
    match fit {
        Fit::Original => "All of your picture shows; see-through edges fill the rest.",
        Fit::Fill => "Your picture covers the whole slot; what sticks out is cut off.",
        Fit::Stretch => "Squeezed to the slot's shape, so it may look distorted.",
        Fit::Own => {
            "Keeps your picture's own size. The game may draw it bigger or smaller than its image."
        }
    }
}

/// Fit, Fill, Stretch or own size, and for the first two a box to drag and zoom the picture
/// inside the slot. Only a replaced texture (a PNG on a picture) has this.
fn placement(
    ui: &mut Ui,
    state: &AppState,
    entry: &ImageEntry,
    thumbs: &mut Thumbs,
    edits: &mut Vec<Edit>,
) {
    let Some(fit) = state
        .image_override(&entry.path)
        .and_then(IconOverride::fit)
    else {
        return;
    };
    let (Some(slot), Some(file)) = (
        state.images.slots.get(&entry.path).copied(),
        state.stored_image(&entry.path),
    ) else {
        return;
    };
    ui.add_space(12.0);
    widgets::caption(ui, "Fit in the slot");
    let current = Fit::ALL.iter().position(|f| *f == fit).unwrap_or(0);
    if let Some(i) = widgets::segmented(ui, &["Fit", "Fill", "Stretch", "Own size"], current) {
        edits.push(Edit::Fit(Fit::ALL[i]));
    }
    ui.add_space(2.0);
    ui.label(RichText::new(fit_hint(fit)).size(11.5).color(WEAK));
    let Some(Slot::Ready { texture, facts }) = thumbs.get(&source_picture(file)) else {
        return;
    };
    let size = (facts.width, facts.height);
    let crop = state.shown_crop(&entry.path);
    let window = crop.unwrap_or_else(|| default_window(fit, size, slot));
    if matches!(fit, Fit::Original | Fit::Fill) {
        ui.add_space(6.0);
        position_editor(ui, &entry.path, &texture, size, slot, window, thumbs, edits);
        size_row(ui, &entry.path, size, slot, window, crop.is_some(), edits);
    }
    let scale = match fit {
        Fit::Original | Fit::Fill => window.scale(slot),
        Fit::Stretch => frame::fill_scale(size, slot),
        Fit::Own => 1.0,
    };
    let out = if fit == Fit::Own { size } else { slot };
    ui.label(
        RichText::new(format!(
            "Yours {} x {}  →  {} x {} in game",
            size.0, size.1, out.0, out.1
        ))
        .size(11.5)
        .color(WEAK),
    );
    if scale > 1.05 {
        ui.label(
            RichText::new(format!(
                "Scaled up {scale:.1}x, so it may look soft. A bigger picture stays sharp."
            ))
            .size(11.5)
            .color(WARN),
        );
    }
}

fn default_window(fit: Fit, size: (u32, u32), slot: (u32, u32)) -> Crop {
    let scale = match fit {
        Fit::Fill => frame::fill_scale(size, slot),
        _ => frame::fit_scale(size, slot),
    };
    let centre = (f64::from(size.0) / 2.0, f64::from(size.1) / 2.0);
    Crop::window(size, slot, scale, centre)
}

/// The slot drawn fixed with the player's picture behind it: drag moves the picture,
/// scrolling zooms it, the part outside the slot shows dimmed.
#[allow(clippy::too_many_arguments)]
fn position_editor(
    ui: &mut Ui,
    path: &str,
    texture: &egui::TextureHandle,
    size: (u32, u32),
    slot: (u32, u32),
    window: Crop,
    thumbs: &Thumbs,
    edits: &mut Vec<Edit>,
) {
    let width = ui.available_width();
    let height = (width * 0.62).clamp(150.0, 230.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(6), RAIL);
    let room = rect.shrink2(vec2(28.0, 26.0));
    let view = (room.width() / slot.0 as f32).min(room.height() / slot.1 as f32);
    let frame_rect = Rect::from_center_size(
        rect.center() - vec2(0.0, 6.0),
        vec2(slot.0 as f32 * view, slot.1 as f32 * view),
    );
    let k = frame_rect.width() / window.width as f32;
    let picture = Rect::from_min_size(
        frame_rect.min - vec2(window.x as f32 * k, window.y as f32 * k),
        vec2(size.0 as f32 * k, size.1 as f32 * k),
    );
    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    painter.image(texture.id(), picture, uv, Color32::from_white_alpha(70));
    let inside = ui.painter_at(frame_rect.intersect(rect));
    checker(&inside, frame_rect, thumbs);
    inside.image(texture.id(), picture, uv, Color32::WHITE);
    painter.rect_stroke(
        frame_rect,
        CornerRadius::ZERO,
        Stroke::new(1.5, ACCENT),
        StrokeKind::Outside,
    );
    painter.text(
        rect.center_bottom() - vec2(0.0, 8.0),
        Align2::CENTER_BOTTOM,
        "Drag to move · scroll to zoom",
        FontId::proportional(10.5),
        WEAK,
    );
    let scale = window.scale(slot);
    if response.dragged() {
        let delta = response.drag_delta() / k;
        let (cx, cy) = window.centre();
        let moved = Crop::window(
            size,
            slot,
            scale,
            (cx - f64::from(delta.x), cy - f64::from(delta.y)),
        );
        edits.push(Edit::Crop(path.to_string(), moved));
    }
    if response.drag_stopped() {
        edits.push(Edit::CommitCrop);
    }
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            ui.input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
            let zoomed = Crop::window(
                size,
                slot,
                scale * f64::from((scroll * 0.004).exp()),
                window.centre(),
            );
            edits.push(Edit::Crop(path.to_string(), zoomed));
        }
    }
    response.on_hover_cursor(if ui.input(|i| i.pointer.primary_down()) {
        egui::CursorIcon::Grabbing
    } else {
        egui::CursorIcon::Grab
    });
}

fn size_row(
    ui: &mut Ui,
    path: &str,
    size: (u32, u32),
    slot: (u32, u32),
    window: Crop,
    moved: bool,
    edits: &mut Vec<Edit>,
) {
    let fit_scale = frame::fit_scale(size, slot);
    let mut percent = (window.scale(slot) / fit_scale * 100.0).round() as i32;
    let before = percent;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Size").size(12.0).color(WEAK));
        ui.spacing_mut().slider_width = (ui.available_width() - 130.0).max(60.0);
        let response = ui
            .add(
                egui::Slider::new(&mut percent, SIZE_RANGE)
                    .logarithmic(true)
                    .suffix("%"),
            )
            .on_hover_text("100% shows all of your picture inside the slot");
        if percent != before {
            let scaled = Crop::window(
                size,
                slot,
                fit_scale * f64::from(percent) / 100.0,
                window.centre(),
            );
            edits.push(Edit::Crop(path.to_string(), scaled));
        }
        if response.drag_stopped() {
            edits.push(Edit::CommitCrop);
        }
        if moved
            && ui
                .add(egui::Button::new(RichText::new("Centre").size(11.0)).small())
                .on_hover_text("Back to the middle at the same size")
                .clicked()
        {
            let centre = (f64::from(size.0) / 2.0, f64::from(size.1) / 2.0);
            let centred = Crop::window(size, slot, window.scale(slot), centre);
            edits.push(Edit::Crop(path.to_string(), centred));
            edits.push(Edit::CommitCrop);
        }
    });
}

fn notice(ui: &mut Ui, state: &AppState, path: &str) {
    let Some(n) = state.images.notice.as_ref().filter(|n| n.path == path) else {
        return;
    };
    let color = tone_color(n.tone);
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

/// While files hover over the window: what dropping them will do, over the whole page.
fn drop_overlay(ui: &mut Ui, state: &AppState, library: &Library, area: Rect) {
    let mut hovered: Vec<Option<PathBuf>> =
        ui.input(|i| i.raw.hovered_files.iter().map(|f| f.path.clone()).collect());
    if let Some(lever) = &state.images.hover_lever {
        hovered = lever.iter().cloned().map(Some).collect();
    }
    if hovered.is_empty() {
        return;
    }
    let collection = hovered.len() > 1
        || hovered
            .iter()
            .flatten()
            .any(|p| p.is_dir() || p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")));
    let selected = state
        .images
        .selected
        .as_deref()
        .and_then(|p| library.entry(p));
    let (title, sub) = match (collection, selected) {
        (true, _) => (
            "Drop to import a collection".to_string(),
            "Files are matched to the game's images by name. You see what matches before anything changes.".to_string(),
        ),
        (false, Some(entry)) => (
            format!("Drop to replace {}", entry.name()),
            match state.images.slots.get(&entry.path) {
                Some((w, h)) if entry.kind == Target::Raster => {
                    format!("A PNG, sized to {w} x {h}. Several files or a zip import as a collection.")
                }
                _ => "Several files or a zip import as a collection.".to_string(),
            },
        ),
        (false, None) => (
            "Drop to import by name".to_string(),
            "Open an image first to replace just that one.".to_string(),
        ),
    };
    let painter = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("images_drop_overlay"),
    ));
    painter.rect_filled(area, CornerRadius::same(10), BG.gamma_multiply(0.93));
    let inner = area.shrink(14.0);
    let corners = [
        inner.left_top(),
        inner.right_top(),
        inner.right_bottom(),
        inner.left_bottom(),
        inner.left_top(),
    ];
    painter.extend(Shape::dashed_line(
        &corners,
        Stroke::new(2.0, ACCENT),
        10.0,
        6.0,
    ));
    icons::paint(
        &painter,
        Rect::from_center_size(inner.center() - vec2(0.0, 46.0), vec2(40.0, 40.0)),
        Icon::Upload,
        ACCENT,
    );
    painter.text(
        inner.center(),
        Align2::CENTER_CENTER,
        title,
        FontId::new(20.0, theme::semibold()),
        TEXT,
    );
    painter.text(
        inner.center() + vec2(0.0, 28.0),
        Align2::CENTER_CENTER,
        sub,
        FontId::proportional(13.0),
        WEAK,
    );
}
