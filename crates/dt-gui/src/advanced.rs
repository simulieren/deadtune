//! The advanced view: header, tab strip, banner, footer, and the ConVars tab
//! (category rail, convar table, pending panel). The other tab pages live in `views`.

use std::time::Instant;

use dt_core::bridge::execfile::ExecFileBridge;
use dt_core::catalog::{ApplyClass, CatalogEntry, Kind};
use dt_core::preset;
use dt_core::profile::{BaseRef, builtin_suggestions};
use eframe::egui::{
    self, Align, Color32, CornerRadius, Layout, Margin, Rect, RichText, Sense, Stroke, TextStyle,
    Ui, vec2,
};

use crate::friendly;
use crate::live::BridgeKind;
use crate::live_status;
use crate::profiles;
use crate::relaunch::Relaunch;
use crate::settings::{Settings, TargetSource, View};
use crate::state::{
    AppState, PlanSummary, Scope, Setting, Status, Tab, bool_text, fmt_num, parse_bool,
};
use crate::theme::{ACCENT, BAD, BG, BORDER, CARD_HOVER, GOOD, RAIL, TEXT, WARN, WEAK, semibold};
use crate::views::{self, Edit};
use crate::widgets::{self, caption};

/// Restart-class changes: muted blue, distinct from the amber "changed" marker.
pub const RESTART: Color32 = Color32::from_rgb(132, 162, 214);

const RAIL_WIDTH: f32 = 220.0;
const PENDING_WIDTH: f32 = 300.0;
const ROW: f32 = 34.0;

/// The advanced view is denser than the simple one: smaller body text and controls.
fn compact_style(style: &mut egui::Style) {
    for (text_style, size) in [
        (TextStyle::Body, 12.5),
        (TextStyle::Button, 12.5),
        (TextStyle::Monospace, 12.0),
    ] {
        if let Some(font) = style.text_styles.get_mut(&text_style) {
            font.size = size;
        }
    }
    style.spacing.interact_size.y = 22.0;
    style.spacing.button_padding = vec2(8.0, 3.0);
    style.spacing.item_spacing = vec2(6.0, 5.0);
}

pub fn full_ui(ui: &mut Ui, state: &mut AppState, reopen: &mut Option<Settings>) {
    compact_style(ui.style_mut());
    egui::Panel::top("adv_header")
        .frame(
            egui::Frame::new()
                .fill(RAIL)
                .inner_margin(Margin::symmetric(12, 6)),
        )
        .show(ui, |ui| header(ui, state));
    egui::Panel::top("adv_tabs")
        .frame(egui::Frame::new().fill(RAIL).inner_margin(Margin {
            left: 6,
            right: 12,
            top: 0,
            bottom: 0,
        }))
        .show(ui, |ui| tab_row(ui, state));
    if state.banner.is_some() {
        egui::Panel::top("adv_banner")
            .frame(
                egui::Frame::new()
                    .fill(WARN.gamma_multiply(0.18))
                    .inner_margin(Margin::symmetric(12, 8)),
            )
            .show(ui, |ui| banner(ui, state));
    }
    if crate::update_view::wants_banner(state) {
        egui::Panel::top("adv_update_banner")
            .frame(crate::update_view::banner_frame(Margin::symmetric(12, 8)))
            .show(ui, |ui| crate::update_view::banner(ui, state));
    }
    if state.guard.failure.is_some() {
        egui::Panel::top("adv_guard_banner")
            .frame(
                egui::Frame::new()
                    .fill(BAD.gamma_multiply(0.18))
                    .inner_margin(Margin::symmetric(12, 8)),
            )
            .show(ui, |ui| crate::addons_view::guard_banner(ui, state));
    }
    egui::Panel::bottom("adv_footer")
        .frame(
            egui::Frame::new()
                .fill(RAIL)
                .inner_margin(Margin::symmetric(12, 3)),
        )
        .show(ui, |ui| footer(ui, state));
    let side = |margin| {
        egui::Frame::new()
            .fill(RAIL)
            .inner_margin(Margin::same(margin))
    };
    if matches!(state.ui.tab, Tab::ConVars | Tab::Video) {
        egui::Panel::right("adv_pending")
            .exact_size(PENDING_WIDTH)
            .resizable(false)
            .frame(side(12))
            .show(ui, |ui| pending(ui, state));
    }
    if state.ui.tab == Tab::ConVars {
        egui::Panel::left("adv_categories")
            .exact_size(RAIL_WIDTH)
            .resizable(false)
            .frame(side(10))
            .show(ui, |ui| categories(ui, state));
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(Margin {
                left: 16,
                right: 16,
                top: 12,
                bottom: 0,
            }))
            .show(ui, |ui| convar_table(ui, state));
        return;
    }
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(BG).inner_margin(Margin {
            left: 20,
            right: 20,
            top: 14,
            bottom: 0,
        }))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    ui.set_max_width(980.0);
                    match state.ui.tab {
                        Tab::Video => views::video(ui, state),
                        Tab::Hud => {
                            widgets::page_title(
                                ui,
                                "HUD",
                                "Move, resize and hide parts of the in-game HUD.",
                            );
                            crate::hud_view::hud(ui, state);
                        }
                        Tab::Profiles => views::profiles(ui, state),
                        Tab::Backups => views::backups(ui, state),
                        Tab::Bench => views::bench(ui, state),
                        Tab::Launch => views::launch(ui, state),
                        Tab::Settings => views::settings(ui, state, reopen),
                        Tab::ConVars => {}
                    }
                    ui.add_space(16.0);
                });
        });
}

fn header(ui: &mut Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        crate::icons::brand(ui, 17.0, crate::theme::RAIL);
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.add_space(18.0);
        profile_picker(ui, state);
        ui.add_space(10.0);
        base_picker(ui, state);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .button(RichText::new("Compact").size(12.0))
                .on_hover_text("Small always-on-top window with favourites")
                .clicked()
            {
                crate::compact::enter(ui.ctx(), state);
            }
            if widgets::segmented(ui, &["Simple", "Advanced"], 1) == Some(0) {
                state.settings.view = View::Simple;
            }
            ui.add_space(10.0);
            status_chips(ui, state);
            ui.add_space(6.0);
            live_status::launch_control(ui, state, live_status::Fit::Chip);
        });
    });
}

fn status_chips(ui: &mut Ui, state: &mut AppState) {
    let sandbox = state.ctx.in_sandbox;
    if widgets::chip(ui, "Sandbox", ACCENT, Some(sandbox))
        .on_hover_text(
            "Tick while you are in hideout or sandbox: cheat-flagged convars are console-settable \
             there, so they go live instead of waiting for the next launch.",
        )
        .clicked()
    {
        state.set_in_sandbox(!sandbox);
    }
    let ranked = state.settings.source == TargetSource::RankedSafe;
    let label = if ranked {
        "Ranked-safe on"
    } else {
        "Ranked-safe"
    };
    if widgets::chip(ui, label, WARN, Some(ranked))
        .on_hover_text(
            "One click writes the stock ConVars block (video.txt kept); one click goes back to \
             your profile.",
        )
        .clicked()
    {
        state.status = Some(match state.toggle_ranked_safe() {
            Ok(_) if ranked => Status::Info("profile restored".into()),
            Ok(_) => Status::Info("stock ConVars restored (ranked-safe)".into()),
            Err(e) => Status::Error(e),
        });
    }
}

fn profile_picker(ui: &mut Ui, state: &mut AppState) {
    let mut picked = None;
    ui.label(RichText::new("Profile").small().color(WEAK));
    let name = if state.is_dirty() {
        format!("{} *", state.profile.name)
    } else {
        state.profile.name.clone()
    };
    egui::ComboBox::from_id_salt("profile")
        .selected_text(name)
        .width(150.0)
        .show_ui(ui, |ui| {
            for p in profiles::list(&state.profiles_dir()) {
                if ui
                    .selectable_label(p.name == state.profile.name, &p.name)
                    .clicked()
                {
                    picked = Some((p, true));
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
        .add_enabled(
            state.is_dirty(),
            egui::Button::new(RichText::new("Save").size(12.0)),
        )
        .on_hover_text("Save the profile to disk")
        .clicked()
    {
        views::save_profile(state);
    }
    if let Some((profile, on_disk)) = picked {
        state.switch_profile(profile, on_disk);
    }
}

fn base_picker(ui: &mut Ui, state: &mut AppState) {
    let mut picked = None;
    ui.label(RichText::new("Base").small().color(WEAK));
    egui::ComboBox::from_id_salt("base")
        .selected_text(views::base_label(&state.profile.base))
        .width(210.0)
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
        ui.colored_label(BAD, "!").on_hover_text(e);
    }
    if let Some(base) = picked {
        state.set_base(base);
    }
}

fn tab_row(ui: &mut Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        let labels: Vec<&str> = Tab::ALL.iter().map(|t| t.label()).collect();
        let selected = Tab::ALL
            .iter()
            .position(|t| *t == state.ui.tab)
            .unwrap_or(0);
        if let Some(i) = widgets::tab_bar(ui, &labels, selected) {
            state.ui.tab = Tab::ALL[i];
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if let Relaunch::Failed(e) = &state.relaunch {
                widgets::chip(ui, "Relaunch failed", BAD, None).on_hover_text(e);
            }
            if let Some(elapsed) = state.relaunch.elapsed(Instant::now()) {
                widgets::chip(
                    ui,
                    &format!("Relaunching {}s", elapsed.as_secs()),
                    WARN,
                    None,
                );
            }
            if let Some(pending) = &state.pending_restart {
                widgets::chip(
                    ui,
                    &format!("Restart to load {}", pending.names.len()),
                    WARN,
                    None,
                )
                .on_hover_text(pending.names.join("\n"));
            }
        });
    });
}

fn banner(ui: &mut Ui, state: &mut AppState) {
    let Some(banner) = state.banner.clone() else {
        return;
    };
    ui.horizontal(|ui| {
        let text = match &banner.build {
            Some((_, to)) => format!(
                "Game updated (build {}), your config was overwritten. Re-apply?",
                to.as_deref().unwrap_or("?")
            ),
            None => "gameinfo.gi was changed outside DeadTune. Re-apply?".to_string(),
        };
        ui.colored_label(WARN, RichText::new(text).strong());
        if ui.button("Re-apply").clicked() {
            state.status = Some(match state.apply() {
                Ok(_) => Status::Info("re-applied".into()),
                Err(e) => Status::Error(e),
            });
        }
        if ui.button("Dismiss").clicked() {
            state.banner = None;
        }
    });
    egui::CollapsingHeader::new(RichText::new("What changed").small()).show(ui, |ui| {
        egui::ScrollArea::both()
            .max_height(200.0)
            .show(ui, |ui| views::diff_view(ui, &banner.diff));
    });
}

fn footer(ui: &mut Ui, state: &AppState) {
    ui.horizontal(|ui| {
        match &state.status {
            Some(Status::Info(m)) => {
                ui.label(RichText::new(m).size(11.5));
            }
            Some(Status::Error(m)) => {
                ui.label(RichText::new(m).size(11.5).color(BAD));
            }
            None => {}
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let mut authors: Vec<&str> = preset::all()
                .iter()
                .map(|p| p.author)
                .filter(|a| !a.is_empty())
                .collect();
            authors.sort();
            authors.dedup();
            ui.label(
                RichText::new(format!(
                    "DeadTune {} · GPL-3.0 · Presets by {} · Inter (OFL-1.1) and Hack (MIT) fonts · Not affiliated with Valve",
                    env!("CARGO_PKG_VERSION"),
                    authors.join(", ")
                ))
                .size(10.5)
                .color(WEAK.gamma_multiply(0.7)),
            )
            .on_hover_text(crate::simple::NOT_AFFILIATED);
        });
    });
}

fn categories(ui: &mut Ui, state: &mut AppState) {
    crate::simple::search_box(ui, &mut state.ui.search);
    ui.add_space(8.0);
    let counts = state.rail_counts();
    let scope = &mut state.ui.scope;
    let mut pick = |ui: &mut Ui, target: Scope, label: &str, count: usize, changed: usize| {
        if widgets::nav_item(ui, label, *scope == target, count, changed).clicked() {
            *scope = target;
        }
    };
    pick(ui, Scope::All, "All", counts.all.rows, counts.all.changed);
    pick(ui, Scope::Changed, "Changed in profile", counts.changed, 0);
    pick(ui, Scope::Favourites, "Favourites", counts.favourites, 0);
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        caption(ui, "Categories");
    });
    ui.add_space(2.0);
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            for (cat, count) in &counts.categories {
                pick(
                    ui,
                    Scope::Category(cat.clone()),
                    cat,
                    count.rows,
                    count.changed,
                );
            }
        });
}

/// Column x ranges of the convar table, shared by the sticky header and every row.
struct Columns {
    star: Rect,
    name: Rect,
    value: Rect,
    class: Rect,
    state: Rect,
    disable: Rect,
}

impl Columns {
    const GAP: f32 = 10.0;
    const STAR: f32 = 20.0;
    const VALUE: f32 = 196.0;
    const CLASS: f32 = 58.0;
    const STATE: f32 = 62.0;
    const DISABLE: f32 = 20.0;

    fn layout(row: Rect) -> Columns {
        let fixed = Self::STAR + Self::VALUE + Self::CLASS + Self::STATE + Self::DISABLE;
        let name_width = (row.width() - fixed - 5.0 * Self::GAP).clamp(160.0, 440.0);
        let mut x = row.left() + 6.0;
        let mut take = |w: f32| {
            let r = Rect::from_x_y_ranges(x..=x + w, row.y_range());
            x += w + Self::GAP;
            r
        };
        Columns {
            star: take(Self::STAR),
            name: take(name_width),
            value: take(Self::VALUE),
            class: take(Self::CLASS),
            state: take(Self::STATE),
            disable: take(Self::DISABLE),
        }
    }
}

fn scope_title(scope: &Scope) -> &str {
    match scope {
        Scope::All => "All convars",
        Scope::Changed => "Changed in profile",
        Scope::Favourites => "Favourites",
        Scope::Category(c) => c,
    }
}

fn convar_table(ui: &mut Ui, state: &mut AppState) {
    let rows = state.visible_rows();
    let changed = state.changed_count(rows.iter().map(String::as_str));
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(scope_title(&state.ui.scope))
                .size(15.0)
                .strong()
                .family(semibold()),
        );
        let mut text = format!("{} convars", rows.len());
        if changed > 0 {
            text.push_str(&format!(" · {changed} changed"));
        }
        ui.label(RichText::new(text).small().color(WEAK));
    });
    if state.settings.source == TargetSource::RankedSafe {
        ui.label(
            RichText::new("Ranked-safe mode: edits are saved to the profile but not applied.")
                .small()
                .color(WARN),
        );
    }
    ui.add_space(4.0);
    let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
    let cols = Columns::layout(head);
    let painter = ui.painter();
    for (rect, text) in [
        (cols.name, "Name"),
        (cols.value, "Value"),
        (cols.class, "Applies"),
        (cols.state, "State"),
    ] {
        painter.text(
            rect.left_center(),
            egui::Align2::LEFT_CENTER,
            text.to_uppercase(),
            egui::FontId::proportional(10.5),
            WEAK,
        );
    }
    painter.hline(head.x_range(), head.bottom(), Stroke::new(1.0, BORDER));
    if rows.is_empty() {
        ui.add_space(16.0);
        ui.label(
            RichText::new(match state.ui.scope {
                Scope::Changed => "Nothing changed from the preset yet.",
                Scope::Favourites => "No favourites yet. Hover a row and click its star.",
                _ => "Nothing matches the search.",
            })
            .color(WEAK),
        );
        return;
    }
    let mut edits = Vec::new();
    ui.spacing_mut().item_spacing.y = 0.0;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show_rows(ui, ROW, rows.len(), |ui, range| {
            for name in &rows[range] {
                if let Some(edit) = convar_row(ui, state, name) {
                    edits.push((name.clone(), edit));
                }
            }
        });
    for (name, edit) in edits {
        views::apply_edit(state, &name, edit);
    }
}

/// The friendly label, or the first sentence of the catalog notes.
fn subtitle(name: &str, entry: Option<&CatalogEntry>) -> Option<String> {
    if let Some(row) = friendly::row(name) {
        return Some(row.label.to_string());
    }
    let notes = entry?.notes.trim();
    let first = notes
        .split(". ")
        .next()
        .unwrap_or(notes)
        .trim_end_matches('.');
    (!first.is_empty()).then(|| first.to_string())
}

/// A cell: a child Ui that does not move the parent's cursor, so cells never shift the row.
fn cell(ui: &mut Ui, rect: Rect, layout: Layout, add: impl FnOnce(&mut Ui)) {
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(layout));
    add(&mut child);
}

fn child(ui: &mut Ui, rect: Rect, add: impl FnOnce(&mut Ui)) {
    cell(ui, rect, Layout::left_to_right(Align::Center), add);
}

fn convar_row(ui: &mut Ui, state: &AppState, name: &str) -> Option<Edit> {
    let entry = state.catalog.get(name);
    let denied = state.catalog.is_denied(name);
    let setting = state.setting(name);
    let changed = state.is_changed(name);
    let favourite = state.settings.favourites.contains(name);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::hover());
    let hovered = ui.rect_contains_pointer(rect);
    let cols = Columns::layout(rect);
    let painter = ui.painter();
    if hovered {
        painter.rect_filled(rect, CornerRadius::same(4), CARD_HOVER.gamma_multiply(0.7));
    }
    painter.hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0, BORDER.gamma_multiply(0.6)),
    );
    if changed {
        painter.rect_filled(
            Rect::from_min_size(rect.min + vec2(0.0, 6.0), vec2(3.0, ROW - 12.0)),
            CornerRadius::same(2),
            ACCENT,
        );
    }
    let mut edit = None;

    child(ui, cols.star, |ui| {
        let color = if favourite { ACCENT } else { WEAK };
        let tip = if favourite {
            "Remove from favourites"
        } else {
            "Add to favourites (compact mode, phone remote)"
        };
        if widgets::icon_button(
            ui,
            favourite || hovered,
            color,
            widgets::star_glyph(favourite),
        )
        .on_hover_text(tip)
        .clicked()
        {
            edit = Some(Edit::Favourite);
        }
    });

    let sub = subtitle(name, entry);
    let (name_rect, layout) = match sub {
        Some(_) => (
            cols.name.shrink2(vec2(0.0, 3.0)),
            Layout::top_down(Align::Min),
        ),
        None => (cols.name, Layout::left_to_right(Align::Center)),
    };
    cell(ui, name_rect, layout, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let color = if changed { ACCENT } else { TEXT };
        let mut text = RichText::new(name).monospace().size(12.0).color(color);
        if setting == Setting::CommentedOut {
            text = text.strikethrough();
        }
        ui.add(egui::Label::new(text).truncate())
            .on_hover_ui(|ui| views::entry_hover(ui, name, entry));
        if let Some(sub) = sub {
            ui.add(egui::Label::new(RichText::new(sub).size(10.5).color(WEAK)).truncate());
        }
    });

    child(ui, cols.value, |ui| {
        if setting == Setting::CommentedOut {
            let default = entry
                .and_then(|e| e.default.as_deref())
                .map_or(String::new(), |d| format!(", engine default {d}"));
            ui.label(
                RichText::new(format!("line disabled{default}"))
                    .small()
                    .italics()
                    .color(WEAK),
            );
            return;
        }
        ui.add_enabled_ui(!denied, |ui| {
            let value = state.current_value(name);
            if let Some(v) = value_control(ui, name, entry, value.as_deref()) {
                edit = Some(Edit::Set(v));
            }
        });
    });

    child(ui, cols.class, |ui| {
        let (text, color, tip) = if denied {
            (
                "Locked",
                BAD,
                "Denylisted: it shows information competitive play should not have. DeadTune never changes it.",
            )
        } else if state.catalog.is_gameinfo_ignored(name) {
            (
                "Ignored",
                WARN,
                "Ignored: the engine flags this gameinfo_cannot_override, so Deadlock skips it in gameinfo.gi. If it is cheat-flagged, the console still takes it in hideout or sandbox.",
            )
        } else {
            match state.catalog.apply_class(name) {
                ApplyClass::Live => (
                    "Live",
                    GOOD,
                    "Live: the running game picks this up from the console, anywhere. It is also saved to gameinfo.gi.",
                ),
                ApplyClass::LiveCheat => (
                    "Cheat",
                    WARN,
                    "Cheat-flagged: the console accepts it only in hideout or sandbox (tick Sandbox up top). Otherwise it waits for the next launch.",
                ),
                ApplyClass::Restart => (
                    "Restart",
                    RESTART,
                    "Restart: read from gameinfo.gi when Deadlock starts.",
                ),
            }
        };
        widgets::badge(ui, text, color).on_hover_text(tip);
    });

    child(ui, cols.state, |ui| match &setting {
        Setting::Override(_) | Setting::CommentedOut => {
            let preset = state
                .preset_value(name)
                .unwrap_or_else(|| "engine default".into());
            if widgets::reset_pill(ui)
                .on_hover_text(format!("Back to the preset: {preset}"))
                .clicked()
            {
                edit = Some(Edit::Revert);
            }
        }
        Setting::Base(_) => {
            ui.label(RichText::new("preset").small().color(WEAK))
                .on_hover_text("Set by the base preset");
        }
        Setting::EngineDefault => {
            ui.label(
                RichText::new("default")
                    .small()
                    .color(WEAK.gamma_multiply(0.7)),
            )
            .on_hover_text("Not in the preset: the engine default applies");
        }
    });

    let can_disable = !denied && matches!(setting, Setting::Base(_) | Setting::CommentedOut);
    if can_disable {
        child(ui, cols.disable, |ui| {
            let off = setting == Setting::CommentedOut;
            let (color, tip) = if off {
                (ACCENT, "Enable this line again")
            } else {
                (
                    WEAK,
                    "Disable this line (comment out): the engine default applies",
                )
            };
            if widgets::icon_button(ui, off || hovered, color, widgets::disable_glyph)
                .on_hover_text(tip)
                .clicked()
            {
                edit = Some(if off { Edit::Revert } else { Edit::Comment });
            }
        });
    }
    edit
}

/// The table's value control: switch, number field with range hint, combo or text box.
fn value_control(
    ui: &mut Ui,
    name: &str,
    entry: Option<&CatalogEntry>,
    value: Option<&str>,
) -> Option<String> {
    let kind = entry.map_or(&Kind::String, |e| &e.kind);
    let text = value.unwrap_or("");
    match kind {
        Kind::Bool => {
            let on = parse_bool(text);
            let clicked = widgets::switch(ui, on).clicked();
            ui.label(
                RichText::new(if on { "On" } else { "Off" })
                    .small()
                    .color(if on { TEXT } else { WEAK }),
            );
            clicked.then(|| bool_text(!on, value))
        }
        Kind::Int | Kind::Float => {
            let integer = matches!(kind, Kind::Int);
            let mut v: f64 = text.trim().parse().unwrap_or(0.0);
            let step = entry.and_then(|e| e.step).filter(|s| *s > 0.0);
            let speed = step.unwrap_or(if integer { 1.0 } else { 0.01 });
            let range = entry.and_then(|e| e.range).filter(|[lo, hi]| lo < hi);
            let mut drag = egui::DragValue::new(&mut v)
                .speed(speed)
                .max_decimals(if integer { 0 } else { 3 });
            if let Some([lo, hi]) = range {
                drag = drag.range(lo..=hi).clamp_existing_to_range(false);
            }
            let response = ui.add_sized(vec2(84.0, 20.0), drag);
            if let Some([lo, hi]) = range {
                ui.label(
                    RichText::new(format!(
                        "{} – {}",
                        fmt_num(lo, integer),
                        fmt_num(hi, integer)
                    ))
                    .size(10.5)
                    .color(WEAK),
                );
            }
            response.changed().then(|| fmt_num(v, integer))
        }
        Kind::Enum { options } => {
            let mut picked = None;
            egui::ComboBox::from_id_salt(("enum", name))
                .selected_text(text)
                .width(150.0)
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
            ui.add(egui::TextEdit::singleline(&mut edit).desired_width(150.0))
                .changed()
                .then_some(edit)
        }
    }
}

enum PendingAction {
    Apply,
    ApplyRelaunch,
    Push,
    RevertAll,
}

fn summary_row(ui: &mut Ui, color: Color32, count: usize, label: &str, names: &[String]) {
    let dim = count == 0;
    let response = ui
        .horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(10.0, 18.0), Sense::hover());
            ui.painter()
                .circle_filled(rect.center(), 3.5, if dim { BORDER } else { color });
            ui.label(RichText::new(count.to_string()).strong().color(if dim {
                WEAK
            } else {
                TEXT
            }));
            ui.label(RichText::new(label).small().color(WEAK));
        })
        .response;
    if !names.is_empty() {
        let shown: Vec<&str> = names.iter().take(30).map(String::as_str).collect();
        let more = names.len().saturating_sub(shown.len());
        let mut tip = shown.join("\n");
        if more > 0 {
            tip.push_str(&format!("\n... and {more} more"));
        }
        response.on_hover_text(RichText::new(tip).monospace().size(11.0));
    }
}

fn pending(ui: &mut Ui, state: &mut AppState) {
    let mut action = None;
    ui.label(
        RichText::new("Pending changes")
            .size(15.0)
            .strong()
            .family(semibold()),
    );
    let plan = match &state.preview {
        Err(e) => {
            ui.label(RichText::new(format!("Cannot build the plan: {e}")).color(BAD));
            return;
        }
        Ok(plan) => plan.clone(),
    };
    let running = state.ctx.game_running;
    let summary = PlanSummary::of(&plan, running);
    let total = summary.live_now + summary.queued + summary.next_launch;
    let headline = if plan.is_empty() {
        "Files match the profile.".to_string()
    } else if total == 0 {
        "HUD files change.".to_string()
    } else {
        format!("{total} settings change when you apply.")
    };
    ui.label(RichText::new(headline).small().color(WEAK));
    if state.settings.source == TargetSource::RankedSafe {
        ui.label(
            RichText::new("Target: stock ConVars block (ranked-safe).")
                .small()
                .color(WARN),
        );
    }
    ui.add_space(6.0);
    let live: Vec<String> = plan
        .live
        .iter()
        .map(|c| format!("{} = {}", c.name, c.value))
        .collect();
    let (live_now, live_later) = if running {
        (live, Vec::new())
    } else {
        (Vec::new(), live)
    };
    let next_launch: Vec<String> = plan
        .restart
        .iter()
        .cloned()
        .chain(plan.video_changes.iter().map(|(k, v)| format!("{k} = {v}")))
        .chain(live_later)
        .collect();
    summary_row(ui, GOOD, summary.live_now, "live now", &live_now);
    summary_row(
        ui,
        WARN,
        summary.queued,
        "queued until sandbox",
        &plan.queued_cheat,
    );
    summary_row(
        ui,
        RESTART,
        summary.next_launch,
        "next launch",
        &next_launch,
    );
    summary_row(
        ui,
        WARN,
        summary.ignored,
        "ignored by the game",
        &plan.ignored,
    );
    summary_row(ui, BAD, summary.refused, "refused (denylist)", &plan.denied);
    if running && !plan.is_empty() {
        ui.label(
            RichText::new("Deadlock is running: file changes load next launch.")
                .small()
                .color(WARN),
        );
    }
    ui.add_space(8.0);
    ui.spacing_mut().item_spacing.y = 4.0;
    if widgets::primary_button(ui, !plan.is_empty(), "Apply")
        .on_hover_text("Write gameinfo.gi and video.txt (backed up first) and push live changes")
        .clicked()
    {
        action = Some(PendingAction::Apply);
    }
    if widgets::wide_button(ui, !state.relaunch.is_active(), "Apply + relaunch")
        .on_hover_text("Write the files, close the game, start it again through Steam")
        .clicked()
    {
        action = Some(PendingAction::ApplyRelaunch);
    }
    if widgets::wide_button(ui, true, "Push live")
        .on_hover_text("Send live-class changes through the active bridge without writing files")
        .clicked()
    {
        action = Some(PendingAction::Push);
    }
    if widgets::wide_button(ui, state.is_dirty(), "Revert all")
        .on_hover_text("Throw away unsaved profile edits")
        .clicked()
    {
        action = Some(PendingAction::RevertAll);
    }
    ui.add_space(4.0);
    live_status::push_status(ui, state, false);
    if state.settings.bridge == BridgeKind::ExecFile {
        ui.add_space(6.0);
        let hint = ExecFileBridge::bind_hint(&state.settings.bind_key);
        ui.label(
            RichText::new("Live key: bound by the boot cfg when Deadlock starts from DeadTune, or by hand (F7)")
                .size(10.5)
                .color(WEAK),
        );
        ui.horizontal(|ui| {
            egui::Frame::new()
                .fill(BG)
                .corner_radius(CornerRadius::same(4))
                .inner_margin(Margin::symmetric(6, 2))
                .show(ui, |ui| {
                    ui.label(RichText::new(&hint).monospace().size(11.0));
                });
            if ui
                .small_button("Copy")
                .on_hover_text("Copy the bind line")
                .clicked()
            {
                ui.ctx().copy_text(hint.clone());
                state.status = Some(Status::Info("bind line copied".into()));
            }
        });
    }
    if !state.preview_diff.is_empty() {
        ui.add_space(8.0);
        let lines = state.preview_diff.lines().count();
        egui::CollapsingHeader::new(RichText::new(format!("Diff ({lines} lines)")).small())
            .default_open(true)
            .show_unindented(ui, |ui| {
                egui::Frame::new()
                    .fill(BG)
                    .corner_radius(CornerRadius::same(4))
                    .inner_margin(Margin::same(6))
                    .show(ui, |ui| {
                        egui::ScrollArea::both()
                            .auto_shrink(false)
                            .show(ui, |ui| views::diff_view(ui, &state.preview_diff));
                    });
            });
    }
    let ctx = ui.ctx().clone();
    match action {
        Some(PendingAction::Apply) => {
            views::run_apply(&ctx, state);
        }
        Some(PendingAction::ApplyRelaunch) => views::run_apply_relaunch(&ctx, state),
        Some(PendingAction::Push) => {
            let result = state.push_now();
            crate::app::report_push(&ctx, state, result);
        }
        Some(PendingAction::RevertAll) => state.revert_all(),
        None => {}
    }
}
