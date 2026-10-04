//! The simple view: preset on top, tabbed settings rows, a help panel for the focused row,
//! and a persistent action bar. Styled like an in-game video menu (see `theme`).

use dt_core::bridge::execfile::ExecFileBridge;
use dt_core::catalog::Impact;
use dt_core::preset::{self, PresetId};
use dt_core::profile::BaseRef;
use eframe::egui::{self, Margin, RichText, Sense, UiBuilder, vec2};

use crate::friendly::{self, Control, Row};
use crate::live::BridgeKind;
use crate::settings::{TargetSource, View};
use crate::simple::{apply, check_setup, status_line};
use crate::state::{AppState, SimpleTab, SimpleTool, Status, bool_text, fmt_num};
use crate::theme::{self, ACCENT, DIM, LINE, PANEL, TEXT};

const ROW_HEIGHT: f32 = 50.0;
const HELP_WIDTH: f32 = 340.0;

enum Edit {
    Set(String),
    Reset,
}

pub fn simple(ui: &mut egui::Ui, state: &mut AppState) {
    header(ui, state);
    banner(ui, state);
    preset_band(ui, state);
    tab_bar(ui, state);
    egui::Panel::bottom("tuner_actions")
        .frame(theme::frame(PANEL, Margin::symmetric(24, 12)))
        .show(ui, |ui| action_bar(ui, state));
    egui::Panel::right("tuner_help")
        .exact_size(HELP_WIDTH)
        .resizable(false)
        .frame(theme::frame(PANEL, Margin::same(22)))
        .show(ui, |ui| help_panel(ui, state));
    egui::CentralPanel::default()
        .frame(theme::frame(theme::BG, Margin::symmetric(24, 4)))
        .show(ui, |ui| rows_panel(ui, state));
    tool_window(ui.ctx(), state);
}

fn header(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Panel::top("tuner_header")
        .frame(theme::frame(PANEL, Margin::symmetric(24, 10)))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("DEADTUNE").size(21.0).strong().color(ACCENT));
                ui.label(RichText::new("Video settings").size(15.0).color(DIM));
                if state.ctx.game_running {
                    theme::pill(ui, "Deadlock is running", theme::OK);
                }
                if state.settings.source == TargetSource::RankedSafe {
                    theme::pill(ui, "Ranked-safe mode", theme::WARN);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::ghost_button(ui, "Advanced view").clicked() {
                        state.settings.view = View::Advanced;
                    }
                    ui.menu_button("Safety & tools", |ui| safety_menu(ui, state));
                    if !state.settings.bind_helper_dismissed
                        && state.settings.bridge == BridgeKind::ExecFile
                        && ui.button("Set up key bind").clicked()
                    {
                        state.ui.tool = Some(SimpleTool::KeyBind);
                    }
                });
            });
        });
}

fn safety_menu(ui: &mut egui::Ui, state: &mut AppState) {
    ui.set_min_width(280.0);
    if ui.button("Undo last change").clicked() {
        state.status = Some(match state.undo_last() {
            Ok(()) => {
                Status::Info("Undone. The game files are back to before your last Apply.".into())
            }
            Err(e) => Status::Info(e),
        });
        ui.close();
    }
    if ui.button("Restore original game files").clicked() {
        state.status = Some(match state.restore_original_files() {
            Ok(()) => {
                Status::Info("The game files are back to how they were before DeadTune.".into())
            }
            Err(e) => Status::Info(e),
        });
        ui.close();
    }
    let ranked = state.settings.source == TargetSource::RankedSafe;
    let label = if ranked {
        "Ranked-safe mode: ON (click to turn off)"
    } else {
        "Ranked-safe mode: off (click to turn on)"
    };
    if ui
        .button(label)
        .on_hover_text(
            "Puts the game's own performance settings back so matchmaking never complains. \
             Your video settings stay. Click again to return to your settings.",
        )
        .clicked()
    {
        state.status = Some(match state.toggle_ranked_safe() {
            Ok(_) if ranked => Status::Info(
                "Your settings are back. Takes effect next time you start Deadlock.".into(),
            ),
            Ok(_) => Status::Info(
                "Ranked-safe mode is on. Takes effect next time you start Deadlock.".into(),
            ),
            Err(e) => Status::Error(e),
        });
        ui.close();
    }
    ui.separator();
    if state.settings.bridge == BridgeKind::ExecFile && ui.button("Key bind setup").clicked() {
        state.ui.tool = Some(SimpleTool::KeyBind);
        ui.close();
    }
    if ui.button("Check setup").clicked() {
        state.ui.tool = Some(SimpleTool::CheckSetup);
        state.run_checks();
        ui.close();
    }
}

fn banner(ui: &mut egui::Ui, state: &mut AppState) {
    let Some(banner) = state.banner.clone() else {
        return;
    };
    egui::Panel::top("tuner_banner")
        .frame(theme::frame(
            theme::WARN.gamma_multiply(0.16),
            Margin::symmetric(24, 8),
        ))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let text = if banner.build.is_some() {
                    "Deadlock updated and reset your settings."
                } else {
                    "Something else changed the game's settings."
                };
                ui.label(RichText::new(text).strong().color(theme::WARN));
                if ui.button("Put my settings back").clicked() {
                    apply(ui.ctx(), state);
                }
                if ui.button("Ignore").clicked() {
                    state.banner = None;
                }
            });
        });
}

fn presets() -> Vec<PresetId> {
    preset::all()
        .iter()
        .map(|p| p.id)
        .filter(|id| friendly::preset_blurb(*id).is_some())
        .collect()
}

fn preset_band(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Panel::top("tuner_preset")
        .frame(theme::frame(theme::BG, Margin::symmetric(24, 14)))
        .show(ui, |ui| {
            let list = presets();
            let current = match &state.profile.base {
                BaseRef::Preset(id) => list.iter().position(|p| p == id),
                BaseRef::File(_) => None,
            };
            let name = match &state.profile.base {
                BaseRef::Preset(id) => preset::info(*id).label.to_string(),
                BaseRef::File(_) => "My original settings".to_string(),
            };
            let mut pick = None;
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("PRESET").size(12.5).strong().color(DIM));
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        let size = vec2(38.0, 40.0);
                        if theme::arrow_button(ui, -1.0, current != Some(0), size).clicked() {
                            pick = Some(current.map_or(list.len() - 1, |i| i.saturating_sub(1)));
                        }
                        ui.scope(|ui| {
                            let v = ui.visuals_mut();
                            v.widgets.inactive.weak_bg_fill = theme::RAISED;
                            let name_w = ui
                                .painter()
                                .layout_no_wrap(
                                    name.clone(),
                                    egui::FontId::proportional(19.0),
                                    TEXT,
                                )
                                .size()
                                .x;
                            ui.spacing_mut().button_padding =
                                vec2(((260.0 - name_w) / 2.0).max(14.0), 9.0);
                            ui.menu_button(RichText::new(&name).size(19.0).strong(), |ui| {
                                ui.set_min_width(300.0);
                                for (i, id) in list.iter().enumerate() {
                                    let info = preset::info(*id);
                                    let text = format!("{}  ({})", info.label, info.author);
                                    let resp = ui.selectable_label(current == Some(i), text);
                                    let blurb = friendly::preset_blurb(*id).unwrap_or_default();
                                    if resp.on_hover_text(blurb).clicked() {
                                        pick = Some(i);
                                        ui.close();
                                    }
                                }
                            });
                        });
                        let last = current == Some(list.len() - 1);
                        if theme::arrow_button(ui, 1.0, !last, size).clicked() {
                            pick = Some(current.map_or(0, |i| (i + 1).min(list.len() - 1)));
                        }
                    });
                });
                ui.add_space(12.0);
                let blurb = match &state.profile.base {
                    BaseRef::Preset(id) => friendly::preset_blurb(*id).unwrap_or(""),
                    BaseRef::File(_) => "Your own settings from before DeadTune.",
                };
                ui.allocate_ui_with_layout(
                    vec2((ui.available_width() - 330.0).max(160.0), 40.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| ui.label(RichText::new(blurb).size(15.0).color(DIM)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let n = state.changed_count();
                    if n > 0 {
                        if ui
                            .button(RichText::new("Reset all").strong())
                            .on_hover_text("Put every setting back to what this preset uses.")
                            .clicked()
                        {
                            state.reset_to_preset();
                        }
                        ui.label(
                            RichText::new(format!(
                                "{n} {} changed from preset",
                                if n == 1 { "setting" } else { "settings" }
                            ))
                            .color(ACCENT),
                        );
                    } else {
                        ui.label(RichText::new("Matches the preset").color(DIM));
                    }
                });
            });
            if let Some(i) = pick {
                state.set_base(BaseRef::Preset(list[i]));
            }
        });
}

fn tab_bar(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Panel::top("tuner_tabs")
        .frame(theme::frame(theme::BG, Margin::symmetric(24, 0)))
        .show(ui, |ui| {
            let font = egui::FontId::proportional(16.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for tab in SimpleTab::ALL {
                    let changed = friendly::rows_in(tab)
                        .filter(|r| state.is_changed(r.name))
                        .count();
                    let label = tab.label().to_uppercase();
                    let selected = state.ui.simple_tab == tab;
                    let text_w = ui
                        .painter()
                        .layout_no_wrap(label.clone(), font.clone(), TEXT)
                        .size()
                        .x;
                    let badge_w = if changed > 0 { 26.0 } else { 0.0 };
                    let (rect, resp) =
                        ui.allocate_exact_size(vec2(text_w + 36.0 + badge_w, 44.0), Sense::click());
                    let ink = if selected {
                        TEXT
                    } else if resp.hovered() {
                        theme::TEXT.gamma_multiply(0.8)
                    } else {
                        DIM
                    };
                    let mid = rect.left() + 18.0 + text_w / 2.0;
                    ui.painter().text(
                        egui::pos2(mid, rect.center().y),
                        egui::Align2::CENTER_CENTER,
                        label,
                        font.clone(),
                        ink,
                    );
                    if changed > 0 {
                        let c = egui::pos2(rect.left() + 18.0 + text_w + 17.0, rect.center().y);
                        ui.painter().circle_filled(c, 9.0, ACCENT);
                        ui.painter().text(
                            c,
                            egui::Align2::CENTER_CENTER,
                            changed.to_string(),
                            egui::FontId::proportional(12.0),
                            theme::ACCENT_INK,
                        );
                    }
                    if selected {
                        let bar = egui::Rect::from_min_max(
                            egui::pos2(rect.left(), rect.bottom() - 3.0),
                            rect.right_bottom(),
                        );
                        ui.painter().rect_filled(bar, 0.0, ACCENT);
                    }
                    if resp.clicked() {
                        state.ui.simple_tab = tab;
                    }
                }
            });
            let line = ui.max_rect().left_bottom();
            ui.painter().hline(
                line.x - 24.0..=ui.ctx().content_rect().right(),
                ui.min_rect().bottom(),
                egui::Stroke::new(1.0, LINE),
            );
        });
}

fn rows_panel(ui: &mut egui::Ui, state: &mut AppState) {
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.add_space(6.0);
            if state.settings.source == TargetSource::RankedSafe {
                ui.label(
                    RichText::new(
                        "Ranked-safe mode is on: changes here are kept for later and not applied.",
                    )
                    .color(theme::WARN),
                );
            }
            let mut edits: Vec<(&'static str, Edit)> = Vec::new();
            let mut group = "";
            for row in friendly::rows_in(state.ui.simple_tab) {
                if row.group != group {
                    group = row.group;
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(group.to_uppercase())
                            .size(12.5)
                            .strong()
                            .color(DIM),
                    );
                    ui.add_space(2.0);
                }
                if let Some(edit) = setting_row(ui, state, row) {
                    edits.push((row.name, edit));
                }
            }
            ui.add_space(12.0);
            for (name, edit) in edits {
                state.ui.focus = Some(name);
                match edit {
                    Edit::Set(v) => {
                        if let Err(e) = state.set_convar(name, v) {
                            state.status = Some(Status::Error(e.to_string()));
                        }
                    }
                    Edit::Reset => state.revert_convar(name),
                }
            }
        });
}

fn setting_row(ui: &mut egui::Ui, state: &mut AppState, row: &'static Row) -> Option<Edit> {
    let changed = state.is_changed(row.name);
    let value = state.current_value(row.name).unwrap_or_default();
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::hover());
    if ui.rect_contains_pointer(rect) {
        state.ui.focus = Some(row.name);
    }
    let focused = state.ui.focus == Some(row.name);
    let painter = ui.painter();
    if focused {
        painter.rect_filled(rect, 3.0, PANEL);
    }
    painter.hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(1.0, theme::RAISED),
    );
    if changed {
        let bar = egui::Rect::from_min_size(
            rect.left_top() + vec2(0.0, 6.0),
            vec2(3.0, ROW_HEIGHT - 12.0),
        );
        painter.rect_filled(bar, 1.5, ACCENT);
    }
    painter.text(
        rect.left_center() + vec2(14.0, 0.0),
        egui::Align2::LEFT_CENTER,
        row.label,
        egui::FontId::proportional(16.0),
        if changed { ACCENT } else { TEXT },
    );

    let ctrl_left = rect.left() + (width * 0.36).max(240.0);
    let ctrl_w = (width * 0.40).clamp(300.0, 380.0);
    let ctrl_rect =
        egui::Rect::from_min_size(egui::pos2(ctrl_left, rect.top() + 9.0), vec2(ctrl_w, 32.0));
    let mut edit = ui
        .scope_builder(UiBuilder::new().max_rect(ctrl_rect), |ui| {
            control(ui, row, &value, ctrl_w)
        })
        .inner
        .map(Edit::Set);

    if changed {
        let reset_rect = egui::Rect::from_min_max(
            egui::pos2(ctrl_rect.right() + 14.0, rect.top() + 9.0),
            egui::pos2(rect.right() - 6.0, rect.bottom() - 9.0),
        );
        let preset_text = state
            .preset_value(row.name)
            .map(|v| row.describe(&v))
            .unwrap_or_else(|| "engine default".into());
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(reset_rect)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
            |ui| {
                if ui
                    .button("Reset")
                    .on_hover_text(format!("Back to the preset: {preset_text}"))
                    .clicked()
                {
                    edit = Some(Edit::Reset);
                }
                ui.add(
                    egui::Label::new(
                        RichText::new(format!("Preset: {preset_text}"))
                            .size(13.0)
                            .color(DIM),
                    )
                    .truncate(),
                );
            },
        );
    }
    edit
}

fn control(ui: &mut egui::Ui, row: &Row, value: &str, ctrl_w: f32) -> Option<String> {
    match &row.control {
        Control::Levels(opts) => {
            let idx = friendly::nearest_level(opts, value);
            let labels: Vec<&str> = opts.iter().map(|(_, l)| *l).collect();
            let picked = if theme::segmented_width(ui, &labels) <= ctrl_w {
                theme::segmented(ui, &labels, idx)
            } else {
                let current = idx.map_or("Custom", |i| labels[i]);
                theme::arrow_select(ui, current, idx, labels.len(), ctrl_w.min(300.0))
            };
            picked.map(|i| opts[i].0.to_string())
        }
        Control::Toggle | Control::ToggleInverted => {
            let on = friendly::switch_on(&row.control, value);
            let inverted = matches!(row.control, Control::ToggleInverted);
            theme::segmented(ui, &["Off", "On"], Some(usize::from(on)))
                .map(|i| bool_text((i == 1) != inverted, Some(value)))
        }
        Control::Slider {
            lo,
            hi,
            step,
            unit,
            zero,
        } => {
            let mut v: f64 = value.trim().parse().unwrap_or(*lo);
            ui.spacing_mut().slider_width = ctrl_w - 130.0;
            let (unit, zero) = (*unit, *zero);
            let slider = egui::Slider::new(&mut v, *lo..=*hi)
                .step_by(*step)
                .custom_formatter(move |n, _| friendly::slider_text(unit, zero, n))
                .custom_parser(move |s| {
                    let digits: String = s.chars().filter(char::is_ascii_digit).collect();
                    digits.parse().ok().or_else(|| zero.map(|_| 0.0))
                });
            ui.add(slider).changed().then(|| fmt_num(v, true))
        }
    }
}

fn help_panel(ui: &mut egui::Ui, state: &mut AppState) {
    let tab = state.ui.simple_tab;
    let row = state
        .ui
        .focus
        .and_then(friendly::row_for)
        .filter(|r| r.tab == tab)
        .or_else(|| friendly::rows_in(tab).next());
    let Some(row) = row else { return };
    ui.label(
        RichText::new(row.group.to_uppercase())
            .size(12.5)
            .strong()
            .color(DIM),
    );
    ui.label(RichText::new(row.label).size(24.0).strong());
    ui.add_space(6.0);
    let impact = state
        .catalog
        .get(row.name)
        .map_or(Impact::Unknown, |e| e.impact);
    let (bars, word, tip) = match impact {
        Impact::High => (3, "High", "One of the biggest FPS levers."),
        Impact::Medium => (2, "Medium", "A noticeable FPS difference."),
        Impact::Low => (1, "Low", "A small FPS difference."),
        Impact::Unknown => (0, "Unknown", ""),
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new("FPS IMPACT").size(12.5).strong().color(DIM));
        theme::fps_meter(ui, bars);
        ui.label(RichText::new(word).strong().color(ACCENT));
    });
    if !tip.is_empty() {
        ui.label(RichText::new(tip).size(13.0).color(DIM));
    }
    ui.add_space(10.0);
    ui.label(RichText::new(row.help).size(15.5));
    ui.add_space(14.0);
    ui.separator();
    ui.add_space(6.0);
    let preset = state
        .preset_value(row.name)
        .map(|v| row.describe(&v))
        .unwrap_or_else(|| "Engine default".into());
    let yours = state
        .current_value(row.name)
        .map(|v| row.describe(&v))
        .unwrap_or_default();
    fact(ui, "In this preset", &preset);
    fact(ui, "Your setting", &yours);
    let when = if state.is_live_now(row.name) {
        format!("Instantly in game: press {}", state.settings.bind_key)
    } else {
        "Next time you start Deadlock".to_string()
    };
    fact(ui, "Takes effect", &when);
}

fn fact(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(label).size(13.0).color(DIM));
    });
    ui.label(RichText::new(value).size(15.5).strong());
    ui.add_space(4.0);
}

fn action_bar(ui: &mut egui::Ui, state: &mut AppState) {
    let (n, live) = state.preview.as_ref().map_or((0, 0), |p| {
        (
            p.live.len() + p.queued_cheat.len() + p.restart.len() + p.video_changes.len(),
            p.live.len(),
        )
    });
    let plan = state.preview.as_ref().map(|_| ()).map_err(Clone::clone);
    ui.horizontal(|ui| {
        match &plan {
            Ok(_) => {
                let (text, color) = match n {
                    0 => ("No pending changes".to_string(), DIM),
                    1 => ("1 pending change".to_string(), ACCENT),
                    n => (format!("{n} pending changes"), ACCENT),
                };
                ui.vertical(|ui| {
                    ui.label(RichText::new(text).size(17.0).strong().color(color));
                    ui.label(
                        RichText::new(effect_text(state, n, live))
                            .size(13.0)
                            .color(DIM),
                    );
                });
            }
            Err(raw) => {
                ui.colored_label(theme::BAD, friendly::human_error(raw))
                    .on_hover_text(raw.as_str());
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if theme::primary_button(ui, n > 0, "Apply").clicked() {
                apply(ui.ctx(), state);
            }
            if state.is_dirty() && theme::ghost_button(ui, "Discard changes").clicked() {
                state.revert_all();
            }
            ui.vertical(|ui| status_line(ui, &state.status));
        });
    });
}

fn effect_text(state: &AppState, pending: usize, live: usize) -> String {
    if let Some(p) = &state.pending_restart {
        return format!(
            "Restart Deadlock to load {} saved change(s).",
            p.names.len()
        );
    }
    if pending == 0 {
        return "Everything is applied.".into();
    }
    let key = &state.settings.bind_key;
    match (live, pending - live) {
        (l, 0) if l > 0 => format!("Takes effect right away: press {key} in game."),
        (l, rest) if l > 0 => {
            format!("{l} take effect right away with {key}; {rest} on your next Deadlock launch.")
        }
        _ => "Takes effect next time you start Deadlock.".into(),
    }
}

fn tool_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(tool) = state.ui.tool else { return };
    let mut open = true;
    let title = match tool {
        SimpleTool::KeyBind => "Bind a key for instant changes",
        SimpleTool::CheckSetup => "Check setup",
    };
    egui::Window::new(title)
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| match tool {
            SimpleTool::KeyBind => bind_helper(ui, state),
            SimpleTool::CheckSetup => {
                if state.checks.is_none() {
                    state.run_checks();
                }
                check_setup(ui, state, true);
            }
        });
    if !open {
        state.ui.tool = None;
    }
}

fn bind_helper(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label("Some settings (like the FPS limit) can change while you play. Do this once:");
    ui.label("1. In Deadlock, open the console (F7) and paste this line:");
    let line = ExecFileBridge::bind_hint(&state.settings.bind_key);
    ui.horizontal(|ui| {
        ui.code(RichText::new(&line).size(15.0));
        if ui.button("Copy").clicked() {
            ui.ctx().copy_text(line.clone());
            state.status = Some(Status::Info(
                "Copied. Paste it into the Deadlock console.".into(),
            ));
        }
    });
    ui.label(format!(
        "2. After changing a setting here, press {} in game to load it.",
        state.settings.bind_key
    ));
    if ui.button("Done, hide the reminder").clicked() {
        state.settings.bind_helper_dismissed = true;
        state.ui.tool = None;
    }
}
