//! The Launch options panel: renderer and intro toggles, extra options with a per-option check
//! against this game build, and the full text to paste into Steam.

use dt_core::launch;
use dt_core::launch_options::{self, Renderer, Verdict};
use eframe::egui::{self, Color32, RichText, Ui};

use crate::state::{AppState, Status};
use crate::theme::{BAD, GOOD, TEXT, WARN, WEAK};
use crate::widgets;

/// The panel in its own window, opened from the Launch button's menu.
pub fn window(ctx: &egui::Context, state: &mut AppState) {
    let mut open = state.ui.launch_options_open;
    egui::Window::new("Launch options")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(560.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .frame(egui::Frame::window(&ctx.global_style()).inner_margin(egui::Margin::same(16)))
        .show(ctx, |ui| {
            let room = ctx.content_rect().height() - 120.0;
            egui::ScrollArea::vertical()
                .max_height(room)
                .min_scrolled_height(room)
                .show(ui, |ui| panel(ui, state));
        });
    state.ui.launch_options_open &= open;
}

pub fn panel(ui: &mut Ui, state: &mut AppState) {
    ui.label(
        RichText::new(
            "DeadTune starts Deadlock through Steam with these options. Anything typed into \
             Steam's own Launch Options box (right-click Deadlock > Properties > General) still \
             applies on top of them.",
        )
        .color(WEAK),
    );
    ui.add_space(10.0);

    egui::Grid::new("launch_options_grid")
        .num_columns(2)
        .spacing([16.0, 10.0])
        .show(ui, |ui| {
            ui.label("Renderer");
            ui.vertical(|ui| {
                let labels = Renderer::ALL.map(Renderer::label);
                let current = Renderer::ALL
                    .iter()
                    .position(|r| *r == state.settings.launch.renderer)
                    .unwrap_or(0);
                if let Some(i) = widgets::segmented(ui, &labels, current) {
                    state.settings.launch.renderer = Renderer::ALL[i];
                }
                widgets::hint(ui, renderer_hint(state.settings.launch.renderer));
            });
            ui.end_row();

            ui.label("Skip intro video");
            ui.horizontal(|ui| {
                if widgets::switch(ui, state.settings.launch.skip_intro).clicked() {
                    state.settings.launch.skip_intro ^= true;
                }
                widgets::hint(ui, "-novid");
            });
            ui.end_row();

            ui.label("Console window");
            ui.horizontal(|ui| {
                if widgets::switch(ui, state.settings.console_window).clicked() {
                    state.settings.console_window ^= true;
                }
                widgets::hint(ui, "-console, opens the game's console on start");
            });
            ui.end_row();

            ui.label("Extra options");
            extra_field(ui, state);
            ui.end_row();
        });

    ui.add_space(12.0);
    widgets::caption(ui, "What the game gets");
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
        for checked in launch_options::check(&state.launch_args().args) {
            let (mut long, mut color) = describe(&checked.verdict);
            let mut summary = checked.verdict.summary();
            let setting = checked.arg.flag.strip_prefix('+').filter(|n| *n != "exec");
            if checked.verdict == Verdict::Console && setting.is_some_and(|n| state.sets_convar(n))
            {
                summary = "also set by your settings".into();
                long = "Your preset or profile sets this too. This launch option wins over the \
                        preset's value, and DeadTune's own changes win over it at start. Keep it \
                        in one place."
                    .into();
                color = WARN;
            }
            let text = format!("{}   {summary}", checked.arg.text());
            widgets::chip(ui, &text, color, None).on_hover_text(long);
        }
    });

    ui.add_space(12.0);
    widgets::caption(ui, "Starting from Steam instead");
    widgets::hint(
        ui,
        "Paste this into Steam's Launch Options so live changes still work when you start the \
         game there.",
    );
    ui.add_space(4.0);
    let text = state.steam_launch_options();
    ui.horizontal(|ui| {
        egui::Frame::new()
            .fill(crate::theme::RAIL)
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.add(egui::Label::new(RichText::new(&text).monospace().color(TEXT)).wrap());
            });
    });
    ui.add_space(4.0);
    if ui.button("Copy for Steam").clicked() {
        ui.ctx().copy_text(text);
        state.status = Some(Status::Info(
            "Copied. Paste it into Deadlock's Launch Options in Steam.".into(),
        ));
    }
}

/// Typing goes into a draft so spaces and open quotes survive; the model takes the tokens.
fn extra_field(ui: &mut Ui, state: &mut AppState) {
    let id = ui.id().with("launch_extra_draft");
    let saved = launch::command_line(&state.settings.launch.extra);
    let mut draft = ui
        .data(|d| d.get_temp::<String>(id))
        .unwrap_or_else(|| saved.clone());
    let response = ui.add(
        egui::TextEdit::multiline(&mut draft)
            .hint_text("-high +fps_max 0")
            .font(egui::TextStyle::Monospace)
            .desired_rows(1)
            .desired_width(ui.available_width().min(560.0)),
    );
    if response.changed() {
        state.settings.launch.extra = launch_options::split_command_line(&draft);
    }
    if response.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, draft));
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
    }
}

fn renderer_hint(renderer: Renderer) -> &'static str {
    match renderer {
        Renderer::Default => "Whatever the game picks on its own.",
        Renderer::Vulkan => {
            "The first start with Vulkan builds its shader cache, so expect stutter for a few \
             minutes."
        }
        Renderer::Dx11 => "Forces DirectX 11.",
    }
}

fn describe(verdict: &Verdict) -> (String, Color32) {
    match verdict {
        Verdict::Managed => (
            "DeadTune needs this to send live changes and read the game's replies.".into(),
            WEAK,
        ),
        Verdict::Known => ("In this game build's list of launch options.".into(), GOOD),
        Verdict::Console => (
            "Runs this console command or setting when the game starts.".into(),
            GOOD,
        ),
        Verdict::Unknown => (
            "Not in this game build's list of launch options, so the game most likely ignores \
             it. Many tips online come from older Source games."
                .into(),
            WARN,
        ),
        Verdict::Conflicts { with } => (
            format!("This and {with} ask for opposite things. Keep one."),
            BAD,
        ),
    }
}
