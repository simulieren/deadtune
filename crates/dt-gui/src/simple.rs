//! The default, plain-language screens: find the game, welcome flow, and the simple view
//! (a left rail of sections, the selected section's settings as cards, an Apply bar).

use dt_core::bridge::ack::PushStatus;
use dt_core::bridge::execfile::ExecFileBridge;
use dt_core::catalog::{CatalogEntry, Impact, Kind};
use dt_core::doctor::CheckStatus;
use dt_core::practice::{Group, PracticeMode};
use dt_core::preset::{self, PresetId};
use dt_core::profile::BaseRef;
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Layout, Margin, Rect, RichText, Sense,
    Stroke, StrokeKind, Ui, vec2,
};

use crate::friendly::{self, Control, human_error};
use crate::icons::{self, Icon};
use crate::live::BridgeKind;
use crate::live_status;
use crate::settings::{TargetSource, View};
use crate::state::{
    AppState, Pending, Section, StartChoice, Status, Timing, Welcome, bool_text, fmt_num,
};
use crate::theme::{
    self, ACCENT, BAD, BORDER, CARD_HOVER, GOOD, ON_ACCENT, RAIL, TEXT, WARN, WEAK,
};
use crate::update::{RailClick, Tone};

fn big_button(ui: &mut Ui, enabled: bool, text: &str) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text).size(15.0).strong()).min_size(vec2(150.0, 36.0)),
    )
}

fn accent_button(ui: &mut Ui, enabled: bool, text: &str) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text).size(15.0).strong().color(ON_ACCENT))
            .fill(ACCENT)
            .min_size(vec2(140.0, 36.0)),
    )
}

fn status_line(ui: &mut Ui, status: &Option<Status>) {
    match status {
        Some(Status::Info(m)) => {
            ui.label(m);
        }
        Some(Status::Error(raw)) => {
            ui.colored_label(BAD, human_error(raw)).on_hover_text(raw);
        }
        None => {}
    }
}

/// Returns true when the player asks to try again with `input` (empty = search again).
pub fn find_game(ui: &mut Ui, input: &mut String, error: Option<&str>) -> bool {
    ui.add_space(24.0);
    ui.label(RichText::new("Welcome to DeadTune").text_style(theme::title()));
    ui.add_space(12.0);
    ui.colored_label(WARN, RichText::new("Couldn't find Deadlock").size(18.0));
    ui.label("Paste the folder where Deadlock is installed. It usually ends in steamapps\\common\\Deadlock.");
    ui.weak(
        "In Steam: right-click Deadlock, Manage, Browse local files, then copy the address bar.",
    );
    let response = ui.add(
        egui::TextEdit::singleline(input)
            .desired_width(560.0)
            .hint_text("C:\\Program Files (x86)\\Steam\\steamapps\\common\\Deadlock"),
    );
    if let Some(raw) = error
        && !input.trim().is_empty()
    {
        ui.colored_label(
            BAD,
            "That folder doesn't look like a Deadlock install. Pick the folder named Deadlock.",
        )
        .on_hover_text(raw);
    }
    let submit = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    ui.add_space(8.0);
    big_button(ui, true, "Retry").clicked() || submit
}

const CARD_WIDTH: f32 = 250.0;
const CARD_HEIGHT: f32 = 84.0;

fn card(ui: &mut Ui, selected: bool, title: &str, author: &str, blurb: &str) -> bool {
    let stroke = if selected {
        Stroke::new(2.0, ACCENT)
    } else {
        Stroke::new(1.0, BORDER)
    };
    let fill = if selected {
        ACCENT.gamma_multiply(0.12)
    } else {
        theme::CARD
    };
    let frame = theme::card().stroke(stroke).fill(fill).show(ui, |ui| {
        ui.vertical(|ui| {
            ui.set_width(CARD_WIDTH);
            ui.set_height(CARD_HEIGHT);
            ui.label(
                RichText::new(title)
                    .size(16.0)
                    .strong()
                    .family(theme::semibold()),
            );
            if !author.is_empty() {
                ui.weak(format!("by {author}"));
            }
            ui.label(blurb);
        });
    });
    frame.response.interact(Sense::click()).clicked()
}

pub fn welcome(ui: &mut Ui, state: &mut AppState) {
    let Some(step) = state.welcome.clone() else {
        return;
    };
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::BG).inner_margin(Margin::same(28)))
        .show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.label(RichText::new("Welcome to DeadTune").text_style(theme::title()));
                ui.add_space(8.0);
                match step {
                    Welcome::PickStart { choice } => pick_start(ui, state, choice),
                    Welcome::Done { needs_restart } => {
                        ui.colored_label(GOOD, RichText::new("All set").size(20.0));
                        if needs_restart {
                            ui.label(
                                RichText::new("Takes effect next time you start Deadlock.")
                                    .size(16.0),
                            );
                            if state.ctx.game_running {
                                ui.label(
                                    "Deadlock is running right now: restart it to see the difference.",
                                );
                            }
                        } else {
                            ui.label("Nothing was changed. Your current settings are kept as your starting point.");
                        }
                        ui.add_space(12.0);
                        if accent_button(ui, true, "Continue").clicked() {
                            state.finish_welcome();
                        }
                    }
                }
                status_line(ui, &state.status);
            });
        });
}

fn pick_start(ui: &mut Ui, state: &mut AppState, choice: Option<StartChoice>) {
    ui.colored_label(GOOD, RichText::new("Found Deadlock").size(18.0))
        .on_hover_text(state.paths.game_root.display().to_string());
    ui.add_space(12.0);
    ui.label(
        RichText::new("Pick a starting preset")
            .size(18.0)
            .strong()
            .family(theme::semibold()),
    );
    ui.weak("You can fine-tune everything afterwards.");
    let mut picked = None;
    let columns = ((ui.available_width() / (CARD_WIDTH + 44.0)) as usize).max(1);
    egui::Grid::new("start_cards")
        .spacing([12.0, 12.0])
        .show(ui, |ui| {
            let mut col = 0;
            let mut next = |ui: &mut Ui| {
                col += 1;
                if col % columns == 0 {
                    ui.end_row();
                }
            };
            let keep = choice == Some(StartChoice::KeepCurrent);
            if card(
                ui,
                keep,
                "Keep my current settings",
                "",
                "Change nothing now; start from what you have.",
            ) {
                picked = Some(StartChoice::KeepCurrent);
            }
            next(ui);
            for info in preset::all() {
                let Some(blurb) = friendly::preset_blurb(info.id) else {
                    continue;
                };
                let selected = choice == Some(StartChoice::Preset(info.id));
                if card(ui, selected, info.label, info.author, blurb) {
                    picked = Some(StartChoice::Preset(info.id));
                }
                next(ui);
            }
        });
    if let Some(choice) = picked
        && let Err(e) = state.choose_start(choice)
    {
        state.status = Some(Status::Error(e.to_string()));
    }
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        if accent_button(ui, choice.is_some(), "Apply").clicked()
            && let Err(e) = state.welcome_apply()
        {
            state.status = Some(Status::Error(e));
        }
        if choice.is_none() {
            ui.label("Pick a preset above.");
        }
    });
    ui.weak("Takes effect next time you start Deadlock. Your original files are backed up first.");
}

/// Apply with a plain-language result; what was applied becomes the saved profile.
pub fn apply(ctx: &egui::Context, state: &mut AppState) {
    match state.apply() {
        Ok(applied) => {
            let mut msg = String::from("Saved.");
            if applied.report.needs_restart {
                msg.push_str(" Takes effect next time you start Deadlock.");
            }
            if applied.report.pushed_live > 0 {
                msg.push_str(&format!(
                    " Some changes are ready now: press {} in game.",
                    state.settings.bind_key
                ));
            }
            if let Some(text) = applied.copy {
                ctx.copy_text(text);
            }
            state.status = Some(match state.save_profile() {
                Ok(_) => Status::Info(msg),
                Err(e) => Status::Error(format!("saving your profile: {e}")),
            });
        }
        Err(e) => state.status = Some(Status::Error(e)),
    }
}

/// What the simple view asks of the state, collected while drawing and run afterwards.
enum Edit {
    Set(&'static str, String),
    Reset(Vec<&'static str>),
    ResetAll,
    Base(PresetId),
    Go(Section),
    Focus(&'static str),
    Pin(&'static str),
    Practice(PracticeMode),
    Advanced,
    Mini,
}

const RAIL_WIDTH: f32 = 224.0;
const HELP_WIDTH: f32 = 300.0;
/// Below this content width the help panel gives way to inline help lines under each label.
const HELP_PANEL_MIN_CONTENT: f32 = 1150.0;

/// What the main area shows: a rail section, or search results while the query is non-empty.
#[derive(Clone, Copy, PartialEq)]
enum Page {
    Section(Section),
    Results,
}

pub fn simple(ui: &mut Ui, state: &mut AppState) {
    let mut edits = Vec::new();
    if !state.ui.query.is_empty() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.ui.query.clear();
    }
    let page = if state.ui.query.trim().is_empty() {
        Page::Section(state.ui.section)
    } else {
        Page::Results
    };
    let content_width = ui.available_width() - RAIL_WIDTH;
    let help_panel = match page {
        Page::Section(s) => s.has_rows() && content_width >= HELP_PANEL_MIN_CONTENT,
        Page::Results => content_width >= HELP_PANEL_MIN_CONTENT,
    };
    if let Some(banner) = state.banner.clone() {
        egui::Panel::top("simple_banner")
            .frame(
                egui::Frame::new()
                    .fill(WARN.gamma_multiply(0.18))
                    .inner_margin(Margin::symmetric(16, 10)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let text = if banner.build.is_some() {
                        "Deadlock updated and reset your settings."
                    } else {
                        "Something else changed the game's settings."
                    };
                    ui.colored_label(WARN, RichText::new(text).strong());
                    if ui.button("Put my settings back").clicked() {
                        apply(ui.ctx(), state);
                    }
                    if ui.button("Ignore").clicked() {
                        state.banner = None;
                    }
                });
            });
    }
    if crate::update_view::wants_banner(state) {
        egui::Panel::top("simple_update_banner")
            .frame(crate::update_view::banner_frame(Margin::symmetric(16, 10)))
            .show(ui, |ui| crate::update_view::banner(ui, state));
    }
    if state.guard.failure.is_some() {
        egui::Panel::top("simple_guard_banner")
            .frame(
                egui::Frame::new()
                    .fill(BAD.gamma_multiply(0.18))
                    .inner_margin(Margin::symmetric(16, 10)),
            )
            .show(ui, |ui| crate::addons_view::guard_banner(ui, state));
    }
    egui::Panel::bottom("simple_apply")
        .frame(
            egui::Frame::new()
                .fill(RAIL)
                .inner_margin(Margin::symmetric(20, 10)),
        )
        .show(ui, |ui| apply_bar(ui, state));
    egui::Panel::left("simple_rail")
        .exact_size(RAIL_WIDTH)
        .resizable(false)
        .frame(
            egui::Frame::new()
                .fill(RAIL)
                .inner_margin(Margin::symmetric(12, 14)),
        )
        .show(ui, |ui| rail(ui, state, &mut edits));
    if help_panel {
        egui::Panel::right("simple_help")
            .exact_size(HELP_WIDTH)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(RAIL)
                    .inner_margin(Margin::symmetric(18, 18)),
            )
            .show(ui, |ui| help(ui, state, page));
    }
    let inline_help = !help_panel;
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::BG).inner_margin(Margin {
            left: 24,
            right: 24,
            top: 18,
            bottom: 0,
        }))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    header(ui, state, page, &mut edits);
                    match page {
                        Page::Results => results(ui, state, inline_help, &mut edits),
                        Page::Section(Section::Overview) => overview(ui, state, &mut edits),
                        Page::Section(Section::Hud) => crate::hud_view::layout_page(ui, state),
                        Page::Section(Section::Minimap) => crate::minimap_view::page(ui, state),
                        Page::Section(Section::Addons) => crate::addons_view::addons(ui, state),
                        Page::Section(Section::System) => system(ui, state),
                        Page::Section(Section::Safety) => safety(ui, state, &mut edits),
                        Page::Section(section) => {
                            settings_page(ui, state, section, inline_help, &mut edits)
                        }
                    }
                    ui.add_space(24.0);
                });
        });
    for edit in edits {
        match edit {
            Edit::Set(name, value) => {
                state.ui.focus = Some(name);
                if let Err(e) = state.set_convar(name, value) {
                    state.status = Some(Status::Error(e.to_string()));
                }
            }
            Edit::Reset(names) => state.reset_convars(names),
            Edit::ResetAll => state.reset_to_preset(),
            Edit::Base(id) => state.set_base(BaseRef::Preset(id)),
            Edit::Go(section) => {
                state.ui.section = section;
                state.ui.query.clear();
            }
            Edit::Focus(name) => state.ui.focus = Some(name),
            Edit::Pin(name) => state.toggle_pin(name),
            Edit::Practice(mode) => state.set_practice(mode),
            Edit::Advanced => state.settings.view = View::Advanced,
            Edit::Mini => crate::compact::enter(ui.ctx(), state),
        }
    }
}

fn section_changes(state: &AppState, section: Section) -> usize {
    match section {
        Section::Overview => state.tweak_count(),
        Section::Hud => state.hud_changed_count(),
        Section::Minimap => state.minimap_changed_count(),
        Section::Addons => state.addons_enabled_count(),
        Section::System => state.check_problems(),
        Section::Safety => 0,
        s => state.changed_count(friendly::section_names(s)),
    }
}

/// Small uppercase caption above a group of rows or a fact.
pub(crate) fn caption(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text.to_uppercase())
            .size(11.5)
            .strong()
            .color(WEAK),
    );
}

fn rail(ui: &mut Ui, state: &mut AppState, edits: &mut Vec<Edit>) {
    egui::Panel::bottom("simple_rail_footer")
        .frame(egui::Frame::new().inner_margin(Margin {
            top: 10,
            ..Margin::ZERO
        }))
        .show_separator_line(false)
        .show(ui, |ui| rail_footer(ui, state, edits));
    brand(ui, state, edits);
    ui.add_space(10.0);
    search_box(ui, &mut state.ui.query);
    ui.add_space(6.0);
    let searching = !state.ui.query.trim().is_empty();
    // Measured height of the two group headings with their spacing.
    let headings = 76.0;
    let rows = Section::ALL.len() as f32;
    let row_height = ((ui.available_height() - headings) / rows - 2.0).clamp(26.0, 30.0);
    let shown = state.ui.section;
    let memory = ui.id().with("rail_shown");
    let pass = ui.ctx().cumulative_pass_nr();
    // Layout settles over the first passes after a page change, so the reveal repeats briefly.
    let revealing = ui.ctx().data_mut(|d| {
        let since = match d.get_temp::<(Section, u64)>(memory) {
            Some((section, at)) if section == shown => at,
            _ => {
                d.insert_temp(memory, (shown, pass));
                pass
            }
        };
        pass - since < 4
    });
    let out = egui::ScrollArea::vertical()
        .auto_shrink(false)
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for (i, section) in Section::ALL.into_iter().enumerate() {
                let heading = match i {
                    1 => Some("Settings"),
                    6 => Some("More"),
                    _ => None,
                };
                if let Some(heading) = heading {
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        rail_caption(ui, heading);
                    });
                    ui.add_space(2.0);
                }
                let selected = !searching && shown == section;
                let response = nav_item(
                    ui,
                    section,
                    row_height,
                    selected,
                    section_changes(state, section),
                );
                if selected && revealing {
                    response.scroll_to_me_animation(None, egui::style::ScrollAnimation::none());
                }
                if response.clicked() {
                    edits.push(Edit::Go(section));
                }
            }
        });
    let hidden_below = out.content_size.y - out.state.offset.y - out.inner_rect.height();
    if out.state.offset.y > 1.0 {
        fade_edge(ui.painter(), out.inner_rect, Align::Min);
    }
    if hidden_below > 1.0 {
        fade_edge(ui.painter(), out.inner_rect, Align::Max);
    }
}

/// A gradient into the rail colour at the top (`Min`) or bottom (`Max`) edge of `rect`.
fn fade_edge(painter: &egui::Painter, rect: Rect, edge: Align) {
    let height = 28.0;
    let (band, top, bottom) = match edge {
        Align::Max => (
            Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - height), rect.max),
            RAIL.gamma_multiply(0.0),
            RAIL,
        ),
        _ => (
            Rect::from_min_max(rect.min, egui::pos2(rect.right(), rect.top() + height)),
            RAIL,
            RAIL.gamma_multiply(0.0),
        ),
    };
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(band.left_top(), top);
    mesh.colored_vertex(band.right_top(), top);
    mesh.colored_vertex(band.right_bottom(), bottom);
    mesh.colored_vertex(band.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
}

fn rail_caption(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text.to_uppercase())
            .size(10.5)
            .family(theme::semibold())
            .color(WEAK.gamma_multiply(0.75)),
    );
}

fn section_icon(section: Section) -> Icon {
    match section {
        Section::Overview => Icon::Overview,
        Section::Display => Icon::Display,
        Section::Shadows => Icon::Shadows,
        Section::Effects => Icon::Effects,
        Section::World => Icon::World,
        Section::Performance => Icon::Performance,
        Section::Hud => Icon::Hud,
        Section::Minimap => Icon::Minimap,
        Section::Addons => Icon::Addons,
        Section::System => Icon::System,
        Section::Safety => Icon::Safety,
    }
}

/// Logo mark, wordmark and the current preset, which links to Overview.
fn brand(ui: &mut Ui, state: &AppState, edits: &mut Vec<Edit>) {
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        let (mark, _) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(mark, CornerRadius::same(7), ACCENT);
        for (i, h) in [9.0, 15.0, 11.0].into_iter().enumerate() {
            let x = mark.left() + 8.0 + i as f32 * 6.0;
            painter.rect_filled(
                Rect::from_center_size(egui::pos2(x, mark.center().y), vec2(3.0, h)),
                CornerRadius::same(2),
                ON_ACCENT,
            );
        }
        ui.add_space(4.0);
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label(
            RichText::new("Dead")
                .size(19.0)
                .family(theme::semibold())
                .color(TEXT),
        );
        ui.label(
            RichText::new("Tune")
                .size(19.0)
                .family(theme::semibold())
                .color(ACCENT),
        );
    });
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        let text = format!("Preset: {}", state.preset_label());
        let response = ui
            .add(
                egui::Label::new(RichText::new(&text).size(11.5).color(WEAK))
                    .truncate()
                    .sense(Sense::click()),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Change the preset on Overview");
        if response.hovered() {
            let r = response.rect;
            ui.painter().line_segment(
                [r.left_bottom(), r.right_bottom()],
                Stroke::new(1.0, WEAK.gamma_multiply(0.6)),
            );
        }
        if response.clicked() {
            edits.push(Edit::Go(Section::Overview));
        }
    });
}

/// Launch, the two other views, and the version with what the updater is doing.
fn rail_footer(ui: &mut Ui, state: &mut AppState, edits: &mut Vec<Edit>) {
    if state.settings.source == TargetSource::RankedSafe {
        dot_label(ui, WARN, "Ranked-safe mode on");
        ui.add_space(4.0);
    }
    if state.live.practice.any() {
        dot_label(ui, WARN, "Practice mode on");
        ui.add_space(4.0);
    }
    ui.horizontal(|ui| {
        live_status::launch_control(ui, state, live_status::Fit::Wide);
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let half = (ui.available_width() - 4.0) / 2.0;
        if rail_link(ui, Icon::MiniWindow, "Mini window", half)
            .on_hover_text("A small always-on-top window for tweaking while you play")
            .clicked()
        {
            edits.push(Edit::Mini);
        }
        if rail_link(ui, Icon::Sliders, "Advanced", half)
            .on_hover_text("Every setting, profiles, backups, benchmarks")
            .clicked()
        {
            edits.push(Edit::Advanced);
        }
    });
    ui.add_space(8.0);
    let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(line, CornerRadius::ZERO, BORDER);
    ui.add_space(6.0);
    version_line(ui, state, edits);
}

fn rail_link(ui: &mut Ui, icon: Icon, label: &str, width: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 28.0), Sense::click());
    let t = ui
        .ctx()
        .animate_bool_responsive(response.id.with("hover"), response.hovered());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(6), CARD_HOVER.gamma_multiply(t));
    let color = lerp_color(WEAK, TEXT, t);
    let font = FontId::proportional(12.0);
    let galley = painter.layout_no_wrap(label.to_string(), font, color);
    let content = 14.0 + 6.0 + galley.size().x;
    let left = rect.center().x - content / 2.0;
    icons::paint(
        painter,
        Rect::from_center_size(egui::pos2(left + 7.0, rect.center().y), vec2(14.0, 14.0)),
        icon,
        color,
    );
    painter.galley(
        egui::pos2(left + 20.0, rect.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn version_line(ui: &mut Ui, state: &mut AppState, edits: &mut Vec<Edit>) {
    let status = state.update.state.rail_status(crate::update::AVAILABLE);
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                .size(11.5)
                .color(WEAK.gamma_multiply(0.8)),
        )
        .on_hover_text(format!("DeadTune {}", crate::update::this_version()));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            let idle = !state.update.state.busy()
                && !matches!(state.update.state, crate::update::UpdateState::Ready { .. });
            let recheck = ui
                .add_enabled_ui(crate::update::AVAILABLE && idle, |ui| {
                    crate::widgets::icon_button(ui, true, WEAK, |p, r, c| {
                        icons::paint(p, r, Icon::Refresh, c)
                    })
                })
                .inner
                .on_hover_text("Check for updates")
                .on_disabled_hover_text(if crate::update::AVAILABLE {
                    "Already busy with an update"
                } else {
                    crate::update::UNAVAILABLE
                });
            if recheck.clicked() {
                state.check_update(true);
            }
            let color = match status.tone {
                Tone::Quiet => WEAK,
                Tone::Good => GOOD,
                Tone::Accent => ACCENT,
                Tone::Bad => BAD,
            };
            let clickable = status.click != RailClick::Nothing;
            let mut text = RichText::new(&status.text).size(11.5).color(color);
            if status.tone == Tone::Accent {
                text = text.family(theme::semibold());
            }
            let response = ui.add(egui::Label::new(text).sense(if clickable {
                Sense::click()
            } else {
                Sense::hover()
            }));
            if status.busy {
                ui.add(egui::Spinner::new().size(11.0).color(color));
            } else {
                let (dot, _) = ui.allocate_exact_size(vec2(8.0, 12.0), Sense::hover());
                ui.painter().circle_filled(dot.center(), 3.0, color);
            }
            if !clickable {
                return;
            }
            let hover = match (&state.update.state, status.click) {
                (crate::update::UpdateState::Failed { message, .. }, _) => message.clone(),
                (_, RailClick::Check) => "Check for a new version now".to_string(),
                _ => "Open the Updates card".to_string(),
            };
            let response = response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(hover);
            if response.hovered() {
                let r = response.rect;
                ui.painter().line_segment(
                    [r.left_bottom(), r.right_bottom()],
                    Stroke::new(1.0, color.gamma_multiply(0.6)),
                );
            }
            if response.clicked() {
                match status.click {
                    RailClick::Check => state.check_update(true),
                    RailClick::OpenUpdates => edits.push(Edit::Go(Section::Safety)),
                    RailClick::Nothing => {}
                }
            }
        });
    });
}

fn lerp_color(from: Color32, to: Color32, t: f32) -> Color32 {
    let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
    Color32::from_rgb(
        mix(from.r(), to.r()),
        mix(from.g(), to.g()),
        mix(from.b(), to.b()),
    )
}

/// Full-width text box styled like the other inputs, with a clear button once it has text.
pub(crate) fn search_box(ui: &mut Ui, query: &mut String) {
    let height = 32.0;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    ui.painter().rect(
        rect,
        CornerRadius::same(6),
        theme::CARD,
        Stroke::new(1.0, BORDER),
        StrokeKind::Inside,
    );
    let clear = rect.right_center() - vec2(14.0, 0.0);
    icons::paint(
        ui.painter(),
        Rect::from_center_size(rect.left_center() + vec2(15.0, 0.0), vec2(14.0, 14.0)),
        Icon::Search,
        WEAK,
    );
    let text_rect = Rect::from_min_max(
        rect.left_top() + vec2(28.0, 0.0),
        egui::pos2(clear.x - 12.0, rect.bottom()),
    );
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(text_rect)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.add(
                egui::TextEdit::singleline(query)
                    .hint_text("Search settings")
                    .desired_width(f32::INFINITY)
                    .frame(egui::Frame::NONE),
            )
            .on_hover_text("Finds a setting by name or by what its help says. Esc clears.");
        },
    );
    if query.is_empty() {
        return;
    }
    let response = ui
        .interact(
            Rect::from_center_size(clear, vec2(18.0, 18.0)),
            ui.id().with("clear_search"),
            Sense::click(),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Clear");
    let color = if response.hovered() { TEXT } else { WEAK };
    let r = 3.5;
    let painter = ui.painter();
    painter.line_segment(
        [clear + vec2(-r, -r), clear + vec2(r, r)],
        Stroke::new(1.5, color),
    );
    painter.line_segment(
        [clear + vec2(-r, r), clear + vec2(r, -r)],
        Stroke::new(1.5, color),
    );
    if response.clicked() {
        query.clear();
    }
}

fn dot_label(ui: &mut Ui, color: Color32, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        let (rect, _) = ui.allocate_exact_size(vec2(10.0, 16.0), Sense::hover());
        ui.painter().circle_filled(rect.center(), 4.0, color);
        ui.label(RichText::new(text).small().color(WEAK));
    });
}

fn nav_item(
    ui: &mut Ui,
    section: Section,
    height: f32,
    selected: bool,
    changes: usize,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    let hover = ui
        .ctx()
        .animate_bool_responsive(response.id.with("hover"), response.hovered() && !selected);
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, CornerRadius::same(7), CARD_HOVER);
        painter.rect_filled(
            Rect::from_min_size(rect.min + vec2(0.0, 7.0), vec2(3.0, rect.height() - 14.0)),
            CornerRadius::same(2),
            ACCENT,
        );
    } else if hover > 0.0 {
        painter.rect_filled(
            rect,
            CornerRadius::same(7),
            CARD_HOVER.gamma_multiply(0.7 * hover),
        );
    }
    let resting = WEAK.gamma_multiply(1.1);
    let (icon_color, text_color) = if selected {
        (ACCENT, TEXT)
    } else {
        let c = lerp_color(resting, TEXT, hover);
        (c, c)
    };
    icons::paint(
        painter,
        Rect::from_center_size(rect.left_center() + vec2(22.0, 0.0), vec2(16.0, 16.0)),
        section_icon(section),
        icon_color,
    );
    painter.text(
        rect.left_center() + vec2(40.0, 0.0),
        Align2::LEFT_CENTER,
        section.label(),
        if selected {
            FontId::new(13.5, theme::semibold())
        } else {
            FontId::proportional(13.5)
        },
        text_color,
    );
    if changes > 0 {
        let text = changes.to_string();
        let center = rect.right_center() - vec2(18.0, 0.0);
        let badge = Rect::from_center_size(center, vec2(10.0 + 7.0 * text.len() as f32, 18.0));
        painter.rect_filled(badge, CornerRadius::same(255), ACCENT.gamma_multiply(0.18));
        painter.text(
            center,
            Align2::CENTER_CENTER,
            text,
            FontId::new(11.0, theme::semibold()),
            ACCENT,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn header(ui: &mut Ui, state: &AppState, page: Page, edits: &mut Vec<Edit>) {
    let section = match page {
        Page::Section(s) => s,
        Page::Results => {
            let found = friendly::search(&state.ui.query).len();
            let subtitle = match found {
                0 => format!("Nothing matches \"{}\".", state.ui.query.trim()),
                1 => "1 setting matches. Esc or clear the box to go back.".to_string(),
                n => format!("{n} settings match. Esc or clear the box to go back."),
            };
            ui.label(RichText::new("Results").text_style(theme::title()).strong());
            ui.label(RichText::new(subtitle).color(WEAK));
            ui.add_space(12.0);
            return;
        }
    };
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new(section.label())
                    .text_style(theme::title())
                    .strong(),
            );
            ui.label(RichText::new(section.subtitle()).color(WEAK));
        });
        let names = friendly::section_names(section);
        let changed = state.changed_count(names.iter().copied());
        if section != Section::Overview && changed > 0 {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .button(format!("Reset {changed} in this section"))
                    .on_hover_text("Puts these settings back to what your preset uses")
                    .clicked()
                {
                    edits.push(Edit::Reset(names));
                }
            });
        }
    });
    if state.settings.source == TargetSource::RankedSafe && section != Section::Safety {
        ui.add_space(6.0);
        egui::Frame::new()
            .fill(WARN.gamma_multiply(0.14))
            .corner_radius(CornerRadius::same(theme::RADIUS))
            .inner_margin(Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.colored_label(
                    WARN,
                    "Ranked-safe mode is on: the game uses its own settings. Changes here are kept for later.",
                );
            });
    }
    ui.add_space(12.0);
}

pub(crate) fn card_title(ui: &mut Ui, title: &str) {
    ui.label(
        RichText::new(title)
            .text_style(egui::TextStyle::Heading)
            .strong(),
    );
    ui.add_space(2.0);
}

fn overview(ui: &mut Ui, state: &AppState, edits: &mut Vec<Edit>) {
    hero(ui, state, edits);
    ui.add_space(12.0);
    let wide = ui.available_width() >= 760.0;
    let status_width = 300.0;
    let gap = 14.0;
    if wide {
        ui.horizontal_top(|ui| {
            let left = ui.available_width() - status_width - gap;
            ui.vertical(|ui| {
                ui.set_width(left);
                key_settings(ui, state, edits);
            });
            ui.add_space(gap - ui.spacing().item_spacing.x);
            ui.vertical(|ui| {
                ui.set_width(status_width);
                status_card(ui, state, edits);
            });
        });
    } else {
        key_settings(ui, state, edits);
        ui.add_space(gap);
        status_card(ui, state, edits);
    }
}

/// "What do you want?": four goal cards, the tweak readout, and a dropdown for every preset.
fn hero(ui: &mut Ui, state: &AppState, edits: &mut Vec<Edit>) {
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            card_title(ui, "What do you want?");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let tweaks = state.tweak_count();
                if tweaks > 0 {
                    if ui
                        .button("Reset all to preset")
                        .on_hover_text("Drops every tweak and goes back to the preset as is")
                        .clicked()
                    {
                        edits.push(Edit::ResetAll);
                    }
                    ui.label(RichText::new(format!("{tweaks} tweaked")).color(ACCENT));
                } else {
                    ui.label(RichText::new("No tweaks").color(WEAK));
                }
            });
        });
        ui.add_space(4.0);
        let gap = 10.0;
        let n = friendly::GOALS.len() as f32;
        let width = ((ui.available_width() - gap * (n - 1.0)) / n - 0.5).floor();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (id, title, sub) in friendly::GOALS {
                let selected = state.profile.base == BaseRef::Preset(*id);
                if goal_card(ui, width, selected, title, sub) {
                    edits.push(Edit::Base(*id));
                }
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let (name, blurb) = match &state.profile.base {
                BaseRef::Preset(id) => {
                    let info = preset::info(*id);
                    (
                        format!("{} by {}", info.label, info.author),
                        friendly::preset_blurb(*id).unwrap_or(""),
                    )
                }
                BaseRef::File(_) => (
                    "My original settings".to_string(),
                    "Your files as they were before DeadTune.",
                ),
            };
            ui.label(RichText::new("Preset:").color(WEAK));
            ui.label(RichText::new(name).strong());
            ui.label(RichText::new(blurb).color(WEAK));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                egui::ComboBox::from_id_salt("all_presets")
                    .selected_text("All presets")
                    .width(150.0)
                    .show_ui(ui, |ui| {
                        for info in preset::all() {
                            let Some(blurb) = friendly::preset_blurb(info.id) else {
                                continue;
                            };
                            let selected = state.profile.base == BaseRef::Preset(info.id);
                            if ui
                                .selectable_label(
                                    selected,
                                    format!("{} ({})", info.label, info.author),
                                )
                                .on_hover_text(blurb)
                                .clicked()
                            {
                                edits.push(Edit::Base(info.id));
                            }
                        }
                    });
            });
        });
    });
}

fn goal_card(ui: &mut Ui, width: f32, selected: bool, title: &str, sub: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 56.0), Sense::click());
    let painter = ui.painter();
    let (fill, stroke) = if selected {
        (ACCENT.gamma_multiply(0.14), Stroke::new(2.0, ACCENT))
    } else if response.hovered() {
        (CARD_HOVER, Stroke::new(1.0, WEAK.gamma_multiply(0.5)))
    } else {
        (theme::BG, Stroke::new(1.0, BORDER))
    };
    painter.rect(
        rect,
        CornerRadius::same(theme::RADIUS),
        fill,
        stroke,
        StrokeKind::Inside,
    );
    painter.text(
        rect.center() - vec2(0.0, 9.0),
        Align2::CENTER_CENTER,
        title,
        FontId::proportional(15.5),
        if selected { ACCENT } else { TEXT },
    );
    painter.text(
        rect.center() + vec2(0.0, 10.0),
        Align2::CENTER_CENTER,
        sub,
        FontId::proportional(11.5),
        WEAK,
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

fn key_settings(ui: &mut Ui, state: &AppState, edits: &mut Vec<Edit>) {
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        card_title(ui, "Biggest FPS wins");
        rows(ui, state, friendly::KEY_SETTINGS, true, edits);
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Everything else:").small().color(WEAK));
            for section in Section::ALL.into_iter().filter(|s| s.has_rows()) {
                if ui
                    .add(egui::Button::new(RichText::new(section.label()).small()).frame(false))
                    .clicked()
                {
                    edits.push(Edit::Go(section));
                }
            }
        });
    });
}

fn status_card(ui: &mut Ui, state: &AppState, edits: &mut Vec<Edit>) {
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        card_title(ui, "Status");
        let changed = state.tweak_count();
        status_item(ui, if changed > 0 { ACCENT } else { GOOD }, |ui| {
            ui.label(match changed {
                0 => "Using your preset as is".to_string(),
                1 => "1 setting changed from your preset".to_string(),
                n => format!("{n} settings changed from your preset"),
            });
        });
        if let Some(pending) = &state.pending_restart {
            status_item(ui, WARN, |ui| {
                ui.label(format!(
                    "Restart Deadlock to load {} saved changes",
                    pending.names.len()
                ));
            });
        }
        let ranked = state.settings.source == TargetSource::RankedSafe;
        status_item(ui, if ranked { WARN } else { WEAK }, |ui| {
            ui.label(if ranked {
                "Ranked-safe mode is on"
            } else {
                "Ranked-safe mode is off"
            });
            if ui.small_button("Safety options").clicked() {
                edits.push(Edit::Go(Section::Safety));
            }
        });
        if state.live.practice.any() {
            status_item(ui, WARN, |ui| {
                ui.label("Practice mode is on: Deadlock may refuse to find matches");
                if ui.small_button("Performance").clicked() {
                    edits.push(Edit::Go(Section::Performance));
                }
            });
        }
        if state.settings.bridge != BridgeKind::Clipboard {
            let key = &state.settings.bind_key;
            let (color, text, action) = match state.ack.status() {
                PushStatus::Confirmed { at, .. } => (
                    GOOD,
                    format!(
                        "Instant changes confirmed by Deadlock at {}",
                        live_status::clock(*at)
                    ),
                    None,
                ),
                PushStatus::Waiting { .. } => (
                    ACCENT,
                    format!("Instant changes: waiting for you to press {key} in game"),
                    None,
                ),
                _ if state.settings.live_verified => {
                    (GOOD, format!("Instant changes: press {key} in game"), None)
                }
                _ if state.ack.boot.is_some() => (
                    ACCENT,
                    format!("Instant changes ready: press {key} once in game to test"),
                    Some("Test"),
                ),
                _ => (
                    WEAK,
                    "Instant changes are not set up".to_string(),
                    Some("Set up (1 minute)"),
                ),
            };
            status_item(ui, color, |ui| {
                ui.label(text);
                if let Some(label) = action
                    && ui.small_button(label).clicked()
                {
                    edits.push(Edit::Go(Section::Safety));
                }
            });
        }
    });
}

fn status_item(ui: &mut Ui, color: Color32, add: impl FnOnce(&mut Ui)) {
    ui.horizontal_top(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(10.0, 20.0), Sense::hover());
        ui.painter().circle_filled(rect.center(), 4.0, color);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            add(ui)
        });
    });
    ui.add_space(4.0);
}

fn results(ui: &mut Ui, state: &AppState, inline_help: bool, edits: &mut Vec<Edit>) {
    let found = friendly::search(&state.ui.query);
    for section in Section::ALL.into_iter().filter(|s| s.has_rows()) {
        let names: Vec<&'static str> = found
            .iter()
            .map(|r| r.name)
            .filter(|n| friendly::section_of(n) == Some(section))
            .collect();
        if names.is_empty() {
            continue;
        }
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                caption(ui, section.label());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add(egui::Button::new(RichText::new("Open section").small()).frame(false))
                        .clicked()
                    {
                        edits.push(Edit::Go(section));
                    }
                });
            });
            ui.add_space(4.0);
            rows(ui, state, &names, inline_help, edits);
        });
        ui.add_space(12.0);
    }
}

fn settings_page(
    ui: &mut Ui,
    state: &AppState,
    section: Section,
    inline_help: bool,
    edits: &mut Vec<Edit>,
) {
    let groups = friendly::groups(section);
    let columns = if ui.available_width() >= 1100.0 && groups.len() > 1 {
        2
    } else {
        1
    };
    if columns == 1 {
        for group in groups {
            group_card(ui, state, group, inline_help, edits);
            ui.add_space(12.0);
        }
        if section == Section::Performance {
            practice_card(ui, state, edits);
        }
        return;
    }
    // Greedy split by row count keeps the two columns close in height.
    let mut split: [Vec<&friendly::Group>; 2] = [Vec::new(), Vec::new()];
    let mut heights = [0usize; 2];
    for group in groups {
        let col = if heights[0] <= heights[1] { 0 } else { 1 };
        heights[col] += group.names.len() + 1;
        split[col].push(group);
    }
    ui.columns(2, |cols| {
        for (ui, groups) in cols.iter_mut().zip(&split) {
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                for group in groups {
                    group_card(ui, state, group, inline_help, edits);
                    ui.add_space(12.0);
                }
            });
        }
    });
    if section == Section::Performance {
        practice_card(ui, state, edits);
    }
}

/// SceneSystem shortcuts from the SideLock config. The game's matchmaking check refuses them,
/// so the card carries the warning itself.
fn practice_card(ui: &mut Ui, state: &AppState, edits: &mut Vec<Edit>) {
    let mode = state.profile.practice;
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        card_title(ui, "Practice mode (bots, sandbox, unranked)");
        ui.label(
            RichText::new(
                "Bigger FPS gains from the game's own rendering setup, beyond the settings above. \
                 Method from the SideLock config. Takes effect next time you start Deadlock.",
            )
            .color(WEAK),
        );
        ui.add_space(10.0);
        let rows = [
            (
                Group::Shadows,
                "Turn off shadows",
                "No shadow maps at all. The biggest saving on a weak graphics card.",
            ),
            (
                Group::Fog,
                "Turn off fog",
                "No volumetric, cubemap or gradient fog.",
            ),
            (
                Group::Batching,
                "Faster batching",
                "Fewer sorted draw calls and a smaller transform buffer. A small CPU saving.",
            ),
        ];
        for (group, label, help) in rows {
            let on = mode.get(group);
            ui.horizontal(|ui| {
                if switch(ui, on).clicked() {
                    let mut next = mode;
                    next.set(group, !on);
                    edits.push(Edit::Practice(next));
                }
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    let text = RichText::new(label).size(14.0).strong();
                    ui.label(if on { text.color(ACCENT) } else { text.color(TEXT) });
                    ui.label(RichText::new(help).small().color(WEAK));
                });
            });
            ui.add_space(8.0);
        }
        egui::Frame::new()
            .fill(WARN.gamma_multiply(0.14))
            .corner_radius(CornerRadius::same(theme::RADIUS))
            .inner_margin(Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.colored_label(
                    WARN,
                    "Deadlock may refuse to find matches while this is on. Turn on Ranked-safe mode \
                     or switch this off before queueing.",
                );
            });
    });
}

fn group_card(
    ui: &mut Ui,
    state: &AppState,
    group: &friendly::Group,
    inline_help: bool,
    edits: &mut Vec<Edit>,
) {
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        caption(ui, group.title);
        ui.add_space(4.0);
        rows(ui, state, group.names, inline_help, edits);
    });
}

/// `inline_help` puts the help line under each label; without it the help panel explains
/// the hovered row and rows stay one line tall.
fn rows(
    ui: &mut Ui,
    state: &AppState,
    names: &[&'static str],
    inline_help: bool,
    edits: &mut Vec<Edit>,
) {
    for (i, name) in names.iter().enumerate() {
        if i > 0 {
            let y = ui.cursor().top() + 1.0;
            let x = ui.max_rect().x_range();
            ui.painter()
                .hline(x, y, Stroke::new(1.0, BORDER.gamma_multiply(0.8)));
            ui.add_space(if inline_help { 8.0 } else { 4.0 });
        }
        setting_row(ui, state, name, inline_help, edits);
        ui.add_space(if inline_help { 4.0 } else { 2.0 });
    }
}

fn setting_row(
    ui: &mut Ui,
    state: &AppState,
    name: &'static str,
    inline_help: bool,
    edits: &mut Vec<Edit>,
) {
    let (Some(row), Some(entry)) = (friendly::row(name), state.catalog.get(name)) else {
        return;
    };
    let value = state.current_value(name).unwrap_or_default();
    let preset = state.preset_value(name).unwrap_or_default();
    let changed = state.is_changed(name);
    let total = ui.available_width();
    let control_width = (total * 0.5).clamp(220.0, 400.0);
    let left_width = total - control_width - 16.0;
    let top = ui.cursor().top();
    let focus_fill = ui.painter().add(egui::Shape::Noop);
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(vec2(left_width, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(left_width);
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.horizontal(|ui| {
                let label = RichText::new(row.label).size(14.0).strong();
                ui.label(if changed {
                    label.color(ACCENT)
                } else {
                    label.color(TEXT)
                });
                if changed {
                    let was = friendly::display(row.control, &preset);
                    if ui
                        .add(
                            egui::Button::new(RichText::new("Reset").small().color(ACCENT))
                                .fill(ACCENT.gamma_multiply(0.14))
                                .corner_radius(CornerRadius::same(255))
                                .min_size(vec2(0.0, 18.0)),
                        )
                        .on_hover_text(format!("Back to your preset: {was}"))
                        .clicked()
                    {
                        edits.push(Edit::Reset(vec![name]));
                    }
                }
                let pinned = state.is_pinned(name);
                let quick = friendly::KEY_SETTINGS.contains(&name);
                if !quick
                    && (pinned || state.ui.focus == Some(name))
                    && crate::compact::pin_button(ui, pinned).clicked()
                {
                    edits.push(Edit::Pin(name));
                }
            });
            if inline_help {
                ui.label(RichText::new(row.help).small().color(WEAK));
            }
            if entry.gameinfo_ignored {
                ui.label(
                    RichText::new("Deadlock ignores this in gameinfo.gi")
                        .small()
                        .color(WARN),
                )
                .on_hover_text("Ignored: the engine flags this gameinfo_cannot_override, so Deadlock skips it in gameinfo.gi. If it is cheat-flagged, the console still takes it in hideout or sandbox.");
            }
            if changed {
                let was = friendly::display(row.control, &preset);
                ui.label(RichText::new(format!("Preset: {was}")).small().color(WEAK));
            }
        });
        ui.add_space(16.0 - ui.spacing().item_spacing.x);
        ui.allocate_ui_with_layout(
            vec2(control_width, 26.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.set_width(control_width);
                if let Some(v) = control(ui, row.control, entry, &value, &preset) {
                    edits.push(Edit::Set(name, v));
                }
            },
        );
    });
    let bottom = ui.cursor().top();
    let x = ui.max_rect().left() - 10.0;
    let row_rect =
        Rect::from_x_y_ranges(x..=ui.max_rect().right() + 10.0, top - 4.0..=bottom - 2.0);
    if ui.rect_contains_pointer(row_rect) {
        edits.push(Edit::Focus(name));
    }
    if !inline_help && state.ui.focus == Some(name) {
        ui.painter().set(
            focus_fill,
            egui::Shape::rect_filled(row_rect, CornerRadius::same(4), CARD_HOVER),
        );
    }
    if changed {
        ui.painter().rect_filled(
            Rect::from_x_y_ranges(x..=x + 3.0, top..=bottom - 2.0),
            CornerRadius::same(2),
            ACCENT,
        );
    }
}

pub(crate) fn control(
    ui: &mut Ui,
    control: Control,
    entry: &CatalogEntry,
    value: &str,
    preset: &str,
) -> Option<String> {
    let integer = matches!(entry.kind, Kind::Int);
    match control {
        Control::Toggle { invert } => {
            let on = friendly::toggle_on(control, value);
            let clicked = switch(ui, on).clicked();
            ui.label(RichText::new(if on { "On" } else { "Off" }).color(if on {
                TEXT
            } else {
                WEAK
            }));
            clicked.then(|| bool_text(on == invert, Some(value)))
        }
        Control::Levels(levels) => {
            let current = friendly::level_index(levels, value);
            let marked = friendly::level_index(levels, preset);
            segmented(ui, levels, current, marked).map(|i| fmt_num(levels[i].0, integer))
        }
        Control::Slider { .. } => {
            let [lo, hi] = entry.range.unwrap_or([0.0, 1.0]);
            let step = entry.step.unwrap_or(0.0);
            let mut v: f64 = value.trim().parse().unwrap_or(lo);
            let readout = 104.0;
            ui.spacing_mut().slider_width = ui.available_width() - readout - 8.0;
            let before = v;
            let mut slider = egui::Slider::new(&mut v, lo..=hi).show_value(false);
            if integer {
                slider = slider.integer();
            }
            let response = ui.add(slider);
            let mut edited = (response.changed() && v != before)
                .then(|| fmt_num(friendly::snap(v, [lo, hi], step), integer));
            if let Ok(p) = preset.trim().parse::<f64>()
                && (lo..=hi).contains(&p)
            {
                let rect = response.rect;
                let r = rect.height() / 2.5;
                let x = egui::lerp(
                    rect.left() + r..=rect.right() - r,
                    ((p - lo) / (hi - lo)) as f32,
                );
                ui.painter().line_segment(
                    [
                        egui::pos2(x, rect.top() + 2.0),
                        egui::pos2(x, rect.top() + 7.0),
                    ],
                    Stroke::new(2.0, WEAK),
                );
            }
            // The readout is its own box so typed text ("144", "Unlimited", "2 km") lands
            // exactly, while rail drags still snap to the step grid.
            let mut typed = before;
            ui.allocate_ui_with_layout(
                vec2(readout, 24.0),
                Layout::right_to_left(Align::Center),
                |ui| {
                    let speed = if step > 0.0 {
                        step
                    } else if integer {
                        1.0
                    } else {
                        0.01
                    };
                    let box_response = ui
                        .add(
                            egui::DragValue::new(&mut typed)
                                .range(lo..=hi)
                                .speed(speed)
                                .custom_formatter(move |x, _| {
                                    friendly::display(control, &fmt_num(x, integer))
                                })
                                .custom_parser(move |text| friendly::parse(control, text)),
                        )
                        .on_hover_text("Click to type a value");
                    if box_response.changed() && typed != before {
                        let snapped = if box_response.dragged() {
                            friendly::snap(typed, [lo, hi], step)
                        } else {
                            typed.clamp(lo, hi)
                        };
                        edited = Some(fmt_num(snapped, integer));
                    }
                },
            );
            edited
        }
    }
}

pub(crate) fn switch(ui: &mut Ui, on: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(40.0, 22.0), Sense::click());
    let t = ui.ctx().animate_bool_responsive(response.id, on);
    let fill = if on {
        ACCENT
    } else {
        ui.visuals().widgets.inactive.bg_fill
    };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(255), fill);
    let x = egui::lerp(rect.left() + 11.0..=rect.right() - 11.0, t);
    painter.circle_filled(
        egui::pos2(x, rect.center().y),
        7.5,
        if on { ON_ACCENT } else { TEXT },
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Returns the clicked level; `marked` gets a dot, showing what the preset uses.
fn segmented(
    ui: &mut Ui,
    levels: &[(f64, &str)],
    current: Option<usize>,
    marked: Option<usize>,
) -> Option<usize> {
    let gap = 2.0;
    let n = levels.len() as f32;
    let width = (ui.available_width() - gap * (n - 1.0)) / n;
    let mut picked = None;
    ui.spacing_mut().item_spacing.x = gap;
    for (i, (_, label)) in levels.iter().enumerate() {
        let (rect, response) = ui.allocate_exact_size(vec2(width, 26.0), Sense::click());
        let selected = current == Some(i);
        let r = 6;
        let corner = CornerRadius {
            nw: if i == 0 { r } else { 2 },
            sw: if i == 0 { r } else { 2 },
            ne: if i + 1 == levels.len() { r } else { 2 },
            se: if i + 1 == levels.len() { r } else { 2 },
        };
        let w = &ui.visuals().widgets;
        let fill = if selected {
            ACCENT
        } else if response.hovered() {
            w.hovered.weak_bg_fill
        } else {
            w.inactive.weak_bg_fill
        };
        let painter = ui.painter();
        painter.rect_filled(rect, corner, fill);
        let color = if selected {
            ON_ACCENT
        } else {
            TEXT.gamma_multiply(0.85)
        };
        let font = FontId::proportional(13.0);
        painter.text(rect.center(), Align2::CENTER_CENTER, *label, font, color);
        if marked == Some(i) && !selected {
            painter.circle_filled(rect.center_top() + vec2(0.0, 5.0), 2.0, WEAK);
        }
        let mut response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        if marked == Some(i) {
            response = response.on_hover_text("What your preset uses");
        }
        if response.clicked() && !selected {
            picked = Some(i);
        }
    }
    picked
}

fn apply_bar(ui: &mut Ui, state: &mut AppState) {
    let file_changes = state
        .preview
        .as_ref()
        .map(|p| p.live.len() + p.queued_cheat.len() + p.restart.len() + p.video_changes.len())
        .map_err(Clone::clone);
    let pending = state.pending();
    let ready = pending != Pending::Nothing;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            let tweaks = |n: usize| match n {
                1 => "1 tweak".to_string(),
                n => format!("{n} tweaks"),
            };
            let headline = match &pending {
                Pending::Nothing => "Everything is applied".to_string(),
                Pending::Preset { label, tweaks: 0 } => format!("Switching to {label} preset"),
                Pending::Preset { label, tweaks: n } => {
                    format!("Ready to apply: {label} preset + {}", tweaks(*n))
                }
                Pending::Tweaks(n) => format!("Ready to apply: {}", tweaks(*n)),
                Pending::Other => "Ready to apply: HUD, addon or practice mode changes".to_string(),
            };
            match &file_changes {
                Err(raw) => {
                    ui.colored_label(BAD, human_error(raw))
                        .on_hover_text(raw.as_str());
                }
                Ok(_) => {
                    let text = RichText::new(headline).size(15.0).strong();
                    ui.label(if ready { text.color(ACCENT) } else { text });
                }
            }
            let key = &state.settings.bind_key;
            let when = match state.timing() {
                Timing::Nothing => match &state.pending_restart {
                    Some(p) => format!("Restart Deadlock to load {} saved changes.", p.names.len()),
                    None => "Pick a preset or change a setting, then Apply.".into(),
                },
                Timing::NextLaunch => "Takes effect next time you start Deadlock.".into(),
                Timing::Instant => format!("Takes effect right away: press {key} in game."),
                Timing::Mixed { now, later } => format!(
                    "{now} take effect right away (press {key} in game), {later} next time you start Deadlock."
                ),
            };
            let detail = match file_changes {
                Ok(n) if n > 1 && ready => format!("{n} settings in the game files change. {when}"),
                _ => when,
            };
            ui.label(RichText::new(detail).small().color(WEAK));
            live_status::push_status(ui, state, false);
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if accent_button(ui, ready, "Apply").clicked() {
                apply(ui.ctx(), state);
            }
            if state.is_dirty()
                && ui
                    .add(egui::Button::new("Discard").min_size(vec2(90.0, 36.0)))
                    .on_hover_text("Throw away changes you haven't applied")
                    .clicked()
            {
                state.revert_all();
            }
            ui.add_space(12.0);
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add_space(24.0);
                status_line(ui, &state.status);
            });
        });
    });
}

/// The right-hand panel on settings pages: what the hovered (or last edited) row does.
fn help(ui: &mut Ui, state: &AppState, page: Page) {
    let in_page = |name: &str| match page {
        Page::Section(s) => friendly::section_of(name) == Some(s),
        Page::Results => friendly::search(&state.ui.query)
            .iter()
            .any(|r| r.name == name),
    };
    let row = state
        .ui
        .focus
        .filter(|n| in_page(n))
        .and_then(friendly::row)
        .or_else(|| match page {
            Page::Section(s) => friendly::section_names(s)
                .first()
                .and_then(|n| friendly::row(n)),
            Page::Results => friendly::search(&state.ui.query).first().copied(),
        });
    let Some(row) = row else {
        ui.label(RichText::new("Hover a setting to read what it does.").color(WEAK));
        return;
    };
    let section = friendly::section_of(row.name).map_or("", Section::label);
    let group = friendly::group_of(row.name).map_or("", |g| g.title);
    caption(ui, &format!("{section} · {group}"));
    ui.label(
        RichText::new(row.label)
            .size(20.0)
            .strong()
            .family(theme::semibold()),
    );
    ui.add_space(8.0);
    let (bars, word, tip) = match state
        .catalog
        .get(row.name)
        .map_or(Impact::Unknown, |e| e.impact)
    {
        Impact::High => (3, "High", "One of the biggest FPS levers."),
        Impact::Medium => (2, "Medium", "A noticeable FPS difference."),
        Impact::Low => (1, "Low", "A small FPS difference."),
        Impact::Unknown => (0, "Unknown", "Nobody has measured this one."),
    };
    ui.horizontal(|ui| {
        caption(ui, "FPS impact");
        fps_meter(ui, bars);
        ui.label(RichText::new(word).strong().color(ACCENT));
    });
    ui.label(RichText::new(tip).small().color(WEAK));
    ui.add_space(12.0);
    ui.label(RichText::new(row.help).size(14.0));
    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);
    let preset = state
        .preset_value(row.name)
        .map(|v| friendly::display(row.control, &v))
        .unwrap_or_else(|| "Game default".into());
    let yours = state
        .current_value(row.name)
        .map(|v| friendly::display(row.control, &v))
        .unwrap_or_default();
    fact(ui, "In this preset", &preset);
    fact(ui, "Your setting", &yours);
    let when = if state.is_live_now(row.name) {
        format!("Right away: press {} in game", state.settings.bind_key)
    } else {
        "Next time you start Deadlock".to_string()
    };
    fact(ui, "Takes effect", &when);
}

fn fact(ui: &mut Ui, label: &str, value: &str) {
    caption(ui, label);
    ui.label(RichText::new(value).size(14.5).strong());
    ui.add_space(8.0);
}

fn fps_meter(ui: &mut Ui, filled: usize) {
    let (rect, _) = ui.allocate_exact_size(vec2(40.0, 14.0), Sense::hover());
    for i in 0..3 {
        let bar = Rect::from_min_size(
            rect.left_top() + vec2(i as f32 * 14.0, 0.0),
            vec2(10.0, 14.0),
        );
        let color = if i < filled { ACCENT } else { BORDER };
        ui.painter().rect_filled(bar, CornerRadius::same(2), color);
    }
}

/// Runs the checks the first time the page opens, so there is always something to read.
fn system(ui: &mut Ui, state: &mut AppState) {
    if state.checks.is_none() && !state.checks_running() {
        state.run_checks();
    }
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        check_setup(ui, state, true);
    });
}

fn safety(ui: &mut Ui, state: &mut AppState, edits: &mut Vec<Edit>) {
    let wide = ui.available_width() >= 900.0;
    let undo = |ui: &mut Ui, state: &mut AppState| {
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            card_title(ui, "Undo and restore");
            ui.label(
                RichText::new(
                    "Your original files were backed up before DeadTune changed anything.",
                )
                .color(WEAK),
            );
            ui.add_space(6.0);
            if ui
                .add(egui::Button::new("Undo last change").min_size(vec2(190.0, 30.0)))
                .clicked()
            {
                state.status = Some(match state.undo_last() {
                    Ok(()) => Status::Info(
                        "Undone. The game files are back to before your last Apply.".into(),
                    ),
                    Err(e) => Status::Info(e),
                });
            }
            ui.label(
                RichText::new("Click again to step further back.")
                    .small()
                    .color(WEAK),
            );
            ui.add_space(6.0);
            if ui
                .add(egui::Button::new("Restore original game files").min_size(vec2(190.0, 30.0)))
                .clicked()
            {
                state.status = Some(match state.restore_original_files() {
                    Ok(()) => Status::Info(
                        "The game files are back to how they were before DeadTune.".into(),
                    ),
                    Err(e) => Status::Info(e),
                });
            }
            ui.label(
                RichText::new("Puts the game exactly back to how it was before DeadTune.")
                    .small()
                    .color(WEAK),
            );
        });
    };
    let ranked = |ui: &mut Ui, state: &mut AppState| {
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            card_title(ui, "Ranked-safe mode");
            let on = state.settings.source == TargetSource::RankedSafe;
            let clicked = ui
                .horizontal(|ui| {
                    let clicked = switch(ui, on).clicked();
                    ui.label(RichText::new(if on { "On" } else { "Off" }).strong());
                    clicked
                })
                .inner;
            ui.label(
                RichText::new(
                    "Puts the game's own performance settings back so matchmaking never complains. \
                     Your video settings stay. Turn it off to go back to your settings.",
                )
                .color(WEAK),
            );
            if clicked {
                state.status = Some(match state.toggle_ranked_safe() {
                    Ok(_) if on => Status::Info(
                        "Your settings are back. Takes effect next time you start Deadlock.".into(),
                    ),
                    Ok(_) => Status::Info(
                        "Ranked-safe mode is on. Takes effect next time you start Deadlock.".into(),
                    ),
                    Err(e) => Status::Error(e),
                });
            }
        });
    };
    if wide {
        ui.columns(2, |cols| {
            let top_left = Layout::top_down(Align::Min);
            cols[0].with_layout(top_left, |ui| undo(ui, state));
            cols[1].with_layout(top_left, |ui| ranked(ui, state));
        });
    } else {
        undo(ui, state);
        ui.add_space(12.0);
        ranked(ui, state);
    }
    ui.add_space(12.0);
    if state.settings.bridge != BridgeKind::Clipboard {
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            instant_changes(ui, state);
        });
        ui.add_space(12.0);
    }
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new("Something not working?").strong());
            if ui.button("Open System check").clicked() {
                edits.push(Edit::Go(Section::System));
            }
        });
    });
    ui.add_space(12.0);
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        card_title(ui, "Updates");
        crate::update_view::settings(ui, state, true);
    });
    ui.add_space(10.0);
    ui.label(
        RichText::new("DeadTune is free software (GPL-3.0). Presets by their authors, credited in Advanced view > Settings. Inter font under the SIL OFL 1.1, Hack font under the MIT licence.")
            .small()
            .color(WEAK),
    );
    ui.horizontal(|ui| {
        if ui.button("Open advanced view").clicked() {
            state.settings.view = View::Advanced;
        }
    });
}

fn mono_box(ui: &mut Ui, text: &str) {
    egui::Frame::new()
        .fill(RAIL)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| ui.monospace(text));
}

/// Two steps with live ticks: the boot cfg ran (seen in the console log), and a push came back.
fn instant_changes(ui: &mut Ui, state: &mut AppState) {
    card_title(ui, "Instant changes");
    let key = state.settings.bind_key.clone();
    ui.label(
        RichText::new(
            "Some settings, like the FPS limit, change while you play. DeadTune sends them to the \
             game and reads its console log to confirm each one.",
        )
        .color(WEAK),
    );
    ui.add_space(8.0);
    let booted = state.ack.boot.is_some();
    ui.horizontal_top(|ui| {
        live_status::step_mark(ui, 1, booted);
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Start Deadlock from DeadTune").strong());
                if !state.ctx.game_running {
                    live_status::launch_control(ui, state, live_status::Fit::Chip);
                }
            });
            ui.label(
                RichText::new(if booted {
                    "Deadlock loaded DeadTune's boot file."
                } else {
                    "Or, if you start the game from Steam: right-click Deadlock > Properties > \
                     General > Launch Options, and paste:"
                })
                .small()
                .color(WEAK),
            );
            if !booted {
                let text =
                    dt_core::bridge::boot::steam_launch_options(state.settings.console_window);
                ui.horizontal(|ui| {
                    mono_box(ui, &text);
                    if ui.button("Copy").clicked() {
                        ui.ctx().copy_text(text.clone());
                        state.status = Some(Status::Info(
                            "Copied. Paste it into Deadlock's Launch Options in Steam.".into(),
                        ));
                    }
                });
            }
        });
    });
    ui.add_space(6.0);
    let verified = state.settings.live_verified;
    ui.horizontal_top(|ui| {
        live_status::step_mark(ui, 2, verified);
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Press {key} once in game to test")).strong());
                let send = ui
                    .add_enabled(!state.ack.is_waiting(), egui::Button::new("Send test"))
                    .on_hover_text(
                        "Writes a harmless batch that only asks the game for its FPS limit, then \
                         waits for the reply in the console log.",
                    );
                if send.clicked()
                    && let Err(e) = state.send_test()
                {
                    state.status = Some(Status::Error(e));
                }
            });
            if !live_status::push_status(ui, state, false) {
                ui.label(
                    RichText::new(if verified {
                        "Verified: the game answered a push from DeadTune."
                    } else {
                        "Click Send test, then press the key in game. The tick appears when the \
                         game answers."
                    })
                    .small()
                    .color(WEAK),
                );
            }
        });
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if switch(ui, state.settings.console_window).clicked() {
            state.settings.console_window = !state.settings.console_window;
        }
        ui.label(RichText::new("Also open the game console (-console)").small());
    });
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Key bind, in case you want it by hand (console, F7):")
                .small()
                .color(WEAK),
        );
        let line = ExecFileBridge::bind_hint(&key);
        ui.label(RichText::new(&line).monospace().size(11.0));
        if ui.small_button("Copy").clicked() {
            ui.ctx().copy_text(line);
        }
    });
}

/// `plain` hides the technical detail behind a tooltip.
pub fn check_setup(ui: &mut Ui, state: &mut AppState, plain: bool) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Check setup")
                .text_style(egui::TextStyle::Heading)
                .strong(),
        );
        let running = state.checks_running();
        let label = match (running, state.checks.is_some()) {
            (true, _) => "Checking…",
            (false, true) => "Check again",
            (false, false) => "Run checks",
        };
        if ui.add_enabled(!running, egui::Button::new(label)).clicked() {
            state.run_checks();
        }
        if running {
            ui.spinner();
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        if ui
            .button("Copy diagnostic report")
            .on_hover_text(
                "Plain text for a bug report: DeadTune version, the gameinfo.gi SearchPaths block, what is in \
                 game/citadel/addons and who owns it, DeadTune's records, the launch guard, the last launch \
                 arguments, the last 80 console.log lines and a read-back of every installed pak.",
            )
            .clicked()
        {
            ui.ctx().copy_text(state.diagnostic_report());
            state.status = Some(Status::Info(
                "Diagnostic report copied. Paste it into your notes or the bug report.".into(),
            ));
        }
    });
    let Some(checks) = &state.checks else { return };
    for check in checks {
        let (color, mark) = match check.status {
            CheckStatus::Pass => (GOOD, "OK"),
            CheckStatus::Warn => (WARN, "Warning"),
            CheckStatus::Fail => (BAD, "Problem"),
        };
        ui.horizontal(|ui| {
            ui.colored_label(color, RichText::new(mark).strong());
            let name = ui.label(RichText::new(check.name).strong());
            if plain {
                name.on_hover_text(&check.detail);
            } else {
                ui.weak(&check.detail);
            }
        });
        if check.status != CheckStatus::Pass
            && let Some(fix) = &check.fix
        {
            ui.label(format!("    What to do: {fix}"));
        }
        if check.status != CheckStatus::Pass
            && let Some(uri) = check.link
        {
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                ui.hyperlink_to("Open Windows Settings", uri);
            });
        }
    }
}
