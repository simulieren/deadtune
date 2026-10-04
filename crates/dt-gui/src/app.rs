//! The eframe shell: screens, per-frame polling, top bar, banner, footer and compact mode.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant, SystemTime};

use dt_core::launch;
use dt_core::locate::{self, GamePaths};
use dt_core::preset;
use dt_core::profile::Profile;
use eframe::egui::{self, Color32, RichText, ViewportCommand, WindowLevel};

use crate::live::PushOutcome;
use crate::relaunch::{self, Relaunch};
use crate::settings::{Settings, TargetSource, View};
use crate::state::{AppState, Mode, Status, Tab};
use crate::{Args, profiles, simple, theme, tuner, views};

pub const FULL_SIZE: [f32; 2] = [1280.0, 820.0];
pub const COMPACT_SIZE: [f32; 2] = [320.0, 560.0];
const GAME_POLL: Duration = Duration::from_secs(2);

pub enum Screen {
    FindGame {
        input: String,
        error: Option<String>,
        settings: Box<Settings>,
    },
    Main(Box<AppState>),
}

struct Screenshot {
    path: PathBuf,
    frames: u32,
    requested: bool,
    /// `DEADTUNE_SCREENSHOT_APPLY=1` clicks Apply (same code as the button) before the capture.
    apply: bool,
}

pub struct App {
    pub screen: Screen,
    data_dir: PathBuf,
    args: Args,
    saved_settings: Settings,
    game_poll: Option<Receiver<(bool, Option<SystemTime>)>>,
    screenshot: Option<Screenshot>,
    themed: Option<bool>,
}

fn resolve_paths(game_dir: Option<&Path>) -> Result<GamePaths, String> {
    match game_dir {
        Some(dir) => locate::from_game_root(dir).map_err(|e| e.to_string()),
        None => locate::locate().map_err(|e| e.to_string()),
    }
}

fn spawn_game_poll(ctx: egui::Context) -> Receiver<(bool, Option<SystemTime>)> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        loop {
            let running = launch::is_game_running();
            let started = if running {
                launch::game_started_at()
            } else {
                None
            };
            if tx.send((running, started)).is_err() {
                break;
            }
            ctx.request_repaint();
            std::thread::sleep(GAME_POLL);
        }
    });
    rx
}

impl App {
    pub fn new(
        data_dir: PathBuf,
        settings: Settings,
        args: Args,
        screenshot: Option<PathBuf>,
    ) -> App {
        let saved_settings = Settings::load(&data_dir);
        let mut app = App {
            screen: Screen::FindGame {
                input: String::new(),
                error: None,
                settings: Box::new(settings.clone()),
            },
            data_dir,
            args,
            saved_settings,
            game_poll: None,
            themed: None,
            screenshot: screenshot.map(|path| Screenshot {
                path,
                frames: 0,
                requested: false,
                apply: std::env::var_os("DEADTUNE_SCREENSHOT_APPLY").is_some_and(|v| v == "1"),
            }),
        };
        app.open_game(settings);
        app
    }

    /// Called once the window exists, for things that need the egui context.
    pub fn started(mut self, ctx: &egui::Context) -> App {
        self.game_poll = Some(spawn_game_poll(ctx.clone()));
        self
    }

    fn open_game(&mut self, settings: Settings) {
        let game_dir = settings.game_dir.clone();
        let opened = resolve_paths(game_dir.as_deref()).and_then(|paths| {
            AppState::open(paths, self.data_dir.clone(), settings.clone())
                .map_err(|e| e.to_string())
        });
        match opened {
            Ok(mut state) => {
                if let Err(e) = state.start_watch() {
                    state.status = Some(Status::Error(format!("file watcher: {e}")));
                }
                if self.args.compact {
                    state.ui.mode = Mode::Compact;
                }
                self.startup_profile(&mut state);
                self.screen = Screen::Main(Box::new(state));
            }
            Err(error) => {
                self.screen = Screen::FindGame {
                    input: game_dir
                        .map(|d| d.display().to_string())
                        .unwrap_or_default(),
                    error: Some(error),
                    settings: Box::new(settings),
                };
            }
        }
    }

    /// `--profile` wins; otherwise the power-source profile is applied while the game is closed.
    fn startup_profile(&self, state: &mut AppState) {
        if let Some(path) = &self.args.profile {
            match std::fs::read_to_string(path)
                .map_err(|e| e.to_string())
                .and_then(|t| Profile::from_toml(&t).map_err(|e| e.to_string()))
            {
                Ok(p) => state.switch_profile(p, false),
                Err(e) => state.status = Some(Status::Error(format!("{}: {e}", path.display()))),
            }
            return;
        }
        let source = dt_core::power::power_source();
        let Some(name) = state.settings.power.choose(source).map(str::to_string) else {
            return;
        };
        if launch::is_game_running() {
            return;
        }
        let Some(profile) = profiles::load(&state.profiles_dir(), &name) else {
            state.status = Some(Status::Error(format!("auto profile {name} not found")));
            return;
        };
        state.switch_profile(profile, true);
        let applied = state.preview.as_ref().is_ok_and(|p| !p.is_empty());
        if applied && let Err(e) = state.apply() {
            state.status = Some(Status::Error(format!("auto profile: {e}")));
            return;
        }
        state.status = Some(Status::Info(format!(
            "{source:?} power: switched to {name}"
        )));
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let Screen::Main(state) = &mut self.screen else {
            return;
        };
        state.poll_watch();
        let polls: Vec<_> = self
            .game_poll
            .as_ref()
            .map(|rx| rx.try_iter().collect())
            .unwrap_or_default();
        for (running, started_at) in polls {
            state.observe_game(running, started_at);
            let relaunch = std::mem::take(&mut state.relaunch);
            let (next, action) = relaunch.step(relaunch::Observation {
                running,
                started_at,
                now: Instant::now(),
                wall: SystemTime::now(),
            });
            state.relaunch = next;
            if action == Some(relaunch::Action::Launch)
                && let Err(e) = launch::launch(&state.settings.launch)
            {
                state.relaunch = Relaunch::Failed(e.to_string());
            }
        }
        if let Some(result) = state.tick_live(Instant::now()) {
            report_push(ctx, state, result);
        }
        if state.live_push.is_pending() {
            ctx.request_repaint_after(crate::live::DEBOUNCE);
        }
        if state.relaunch.is_active() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        #[cfg(feature = "remote")]
        crate::remote::sync(state);
        if state.settings != self.saved_settings {
            match state.settings.save(&self.data_dir) {
                Ok(()) => self.saved_settings = state.settings.clone(),
                Err(e) => state.status = Some(Status::Error(format!("saving settings: {e}"))),
            }
        }
    }

    /// The simple screens use the game-menu theme; Advanced and Compact keep egui's default.
    fn sync_theme(&mut self, ctx: &egui::Context) {
        let themed = match &self.screen {
            Screen::FindGame { .. } => true,
            Screen::Main(s) => {
                s.welcome.is_some() || (s.ui.mode == Mode::Full && s.settings.view == View::Simple)
            }
        };
        if self.themed == Some(themed) {
            return;
        }
        let mut style = egui::Style::default();
        if themed {
            theme::apply(&mut style);
        }
        ctx.set_global_style(style);
        self.themed = Some(themed);
    }

    fn screenshot(&mut self, ctx: &egui::Context) {
        let Some(job) = &mut self.screenshot else {
            return;
        };
        job.frames += 1;
        ctx.request_repaint();
        if job.frames == 2
            && let Screen::Main(state) = &mut self.screen
            && let Ok(name) = std::env::var("DEADTUNE_SCREENSHOT_TAB")
            && let Some(tab) = crate::state::SimpleTab::ALL
                .into_iter()
                .find(|t| t.label().eq_ignore_ascii_case(&name))
        {
            state.ui.simple_tab = tab;
        }
        if job.apply
            && job.frames == 5
            && let Screen::Main(state) = &mut self.screen
        {
            match state.settings.view {
                View::Simple => simple::apply(ctx, state),
                View::Advanced => {
                    views::run_apply(ctx, state);
                }
            }
        }
        if job.frames == 20 && !job.requested {
            job.requested = true;
            ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::UserData::default()));
        }
        let image = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
            let png = crate::png::encode_rgba(image.size[0] as u32, image.size[1] as u32, &rgba);
            if let Err(e) = std::fs::write(&job.path, png) {
                eprintln!("DEADTUNE_SCREENSHOT {}: {e}", job.path.display());
            }
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }
}

pub fn report_push(ctx: &egui::Context, state: &mut AppState, result: Result<PushOutcome, String>) {
    state.status = Some(match result {
        Ok(PushOutcome::Sent { bridge, count }) => {
            Status::Info(format!("pushed {count} via {bridge}"))
        }
        Ok(PushOutcome::Copy(text)) => {
            ctx.copy_text(text);
            Status::Info("console commands copied; paste into the console (F7)".into())
        }
        Err(e) => Status::Error(format!("push failed: {e}")),
    });
}

pub fn set_mode(ctx: &egui::Context, state: &mut AppState, mode: Mode) {
    state.ui.mode = mode;
    let (level, size) = match mode {
        Mode::Compact => (WindowLevel::AlwaysOnTop, COMPACT_SIZE),
        Mode::Full => (WindowLevel::Normal, FULL_SIZE),
    };
    ctx.send_viewport_cmd(ViewportCommand::WindowLevel(level));
    ctx.send_viewport_cmd(ViewportCommand::InnerSize(size.into()));
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);
        self.sync_theme(&ctx);
        let mut reopen = None;
        match &mut self.screen {
            Screen::FindGame {
                input,
                error,
                settings,
            } => {
                egui::CentralPanel::default().show(ui, |ui| {
                    if simple::find_game(ui, input, error.as_deref()) {
                        let mut settings = (**settings).clone();
                        let dir = input.trim();
                        settings.game_dir = (!dir.is_empty()).then(|| PathBuf::from(dir));
                        reopen = Some(settings);
                    }
                });
            }
            Screen::Main(state) if state.welcome.is_some() => simple::welcome(ui, state),
            Screen::Main(state) => match (state.ui.mode, state.settings.view) {
                (Mode::Compact, _) => compact_ui(ui, state),
                (Mode::Full, View::Simple) => tuner::simple(ui, state),
                (Mode::Full, View::Advanced) => full_ui(ui, state, &mut reopen),
            },
        }
        if let Some(settings) = reopen {
            self.open_game(settings);
        }
        self.screenshot(&ctx);
    }
}

fn full_ui(ui: &mut egui::Ui, state: &mut AppState, reopen: &mut Option<Settings>) {
    egui::Panel::top("top").show(ui, |ui| top_bar(ui, state));
    if state.banner.is_some() {
        egui::Panel::top("banner").show(ui, |ui| banner(ui, state));
    }
    egui::Panel::bottom("footer").show(ui, |ui| footer(ui, state));
    match state.ui.tab {
        Tab::ConVars => {
            egui::Panel::left("categories")
                .default_size(200.0)
                .show(ui, |ui| views::categories(ui, state));
            egui::Panel::right("pending")
                .default_size(380.0)
                .show(ui, |ui| views::pending(ui, state));
            egui::CentralPanel::default().show(ui, |ui| views::convar_list(ui, state));
        }
        Tab::Video => {
            egui::Panel::right("pending")
                .default_size(380.0)
                .show(ui, |ui| views::pending(ui, state));
            egui::CentralPanel::default().show(ui, |ui| views::video(ui, state));
        }
        tab => {
            egui::CentralPanel::default().show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| match tab {
                    Tab::Hud => crate::hud_view::hud(ui, state),
                    Tab::Profiles => views::profiles(ui, state),
                    Tab::Backups => views::backups(ui, state),
                    Tab::Bench => views::bench(ui, state),
                    Tab::Launch => views::launch(ui, state),
                    Tab::Settings => views::settings(ui, state, reopen),
                    Tab::ConVars | Tab::Video => {}
                })
            });
        }
    }
}

fn top_bar(ui: &mut egui::Ui, state: &mut AppState) {
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.heading("DeadTune");
        ui.separator();
        views::profile_picker(ui, state);
        views::base_picker(ui, state);
        ui.separator();
        let ranked = state.settings.source == TargetSource::RankedSafe;
        let label = if ranked { "Ranked-safe: ON" } else { "Ranked-safe: off" };
        let button = egui::Button::new(label).selected(ranked);
        if ui
            .add(button)
            .on_hover_text("One click restores the stock ConVars block (video.txt kept); one click goes back to your profile.")
            .clicked()
        {
            match state.toggle_ranked_safe() {
                Ok(_) => {
                    let msg = if ranked { "profile restored" } else { "stock ConVars restored (ranked-safe)" };
                    state.status = Some(Status::Info(msg.into()));
                }
                Err(e) => state.status = Some(Status::Error(e)),
            }
        }
        let mut sandbox = state.ctx.in_sandbox;
        if ui
            .checkbox(&mut sandbox, "In hideout/sandbox")
            .on_hover_text("Cheat-flagged convars are console-settable in hideout and sandbox, not in matchmaking.")
            .changed()
        {
            state.set_in_sandbox(sandbox);
        }
        ui.separator();
        if state.ctx.game_running {
            ui.colored_label(Color32::LIGHT_GREEN, "Game running");
        } else {
            ui.weak("Game not running");
        }
        if let Some(pending) = &state.pending_restart {
            ui.colored_label(Color32::GOLD, format!("Restart pending ({})", pending.names.len()))
                .on_hover_text(pending.names.join("\n"));
        }
        if let Some(elapsed) = state.relaunch.elapsed(Instant::now()) {
            ui.colored_label(Color32::GOLD, format!("Relaunching... {}s", elapsed.as_secs()));
        }
        if let Relaunch::Failed(e) = &state.relaunch {
            ui.colored_label(Color32::LIGHT_RED, format!("Relaunch failed: {e}"));
        }
        ui.separator();
        if ui.button("Simple view").clicked() {
            state.settings.view = View::Simple;
        }
        if ui.button("Compact").on_hover_text("Small always-on-top window with favourites").clicked() {
            set_mode(ui.ctx(), state, Mode::Compact);
        }
    });
    ui.horizontal(|ui| {
        for tab in Tab::ALL {
            ui.selectable_value(&mut state.ui.tab, tab, tab.label());
        }
    });
    ui.add_space(2.0);
}

fn banner(ui: &mut egui::Ui, state: &mut AppState) {
    let Some(banner) = state.banner.clone() else {
        return;
    };
    egui::Frame::new()
        .fill(Color32::from_rgb(90, 60, 10))
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let text = match &banner.build {
                    Some((_, to)) => format!(
                        "Game updated (build {}), your config was overwritten. Re-apply?",
                        to.as_deref().unwrap_or("?")
                    ),
                    None => "gameinfo.gi was changed outside DeadTune. Re-apply?".to_string(),
                };
                ui.label(RichText::new(text).strong().color(Color32::WHITE));
                if ui.button("Re-apply").clicked() {
                    match state.apply() {
                        Ok(_) => state.status = Some(Status::Info("re-applied".into())),
                        Err(e) => state.status = Some(Status::Error(e)),
                    }
                }
                if ui.button("Dismiss").clicked() {
                    state.banner = None;
                }
            });
            egui::CollapsingHeader::new("What changed").show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| views::diff_view(ui, &banner.diff));
            });
        });
}

fn footer(ui: &mut egui::Ui, state: &AppState) {
    ui.horizontal_wrapped(|ui| {
        let mut authors: Vec<&str> = preset::all()
            .iter()
            .map(|p| p.author)
            .filter(|a| !a.is_empty())
            .collect();
        authors.sort();
        authors.dedup();
        ui.weak(format!(
            "DeadTune {} | GPL-3.0 | Presets by {}",
            env!("CARGO_PKG_VERSION"),
            authors.join(", ")
        ));
        if let Some(status) = &state.status {
            ui.separator();
            match status {
                Status::Info(m) => ui.label(m),
                Status::Error(m) => ui.colored_label(Color32::LIGHT_RED, m),
            };
        }
    });
}

fn compact_ui(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Panel::bottom("compact_footer").show(ui, |ui| {
        ui.horizontal(|ui| {
            if ui
                .button("Push")
                .on_hover_text("Send live changes through the active bridge")
                .clicked()
            {
                let result = state.push_now();
                report_push(ui.ctx(), state, result);
            }
            let can_apply = state.preview.as_ref().is_ok_and(|p| !p.is_empty());
            if ui
                .add_enabled(can_apply, egui::Button::new("Apply"))
                .clicked()
            {
                views::run_apply(ui.ctx(), state);
            }
            if ui.button("Full").clicked() {
                set_mode(ui.ctx(), state, Mode::Full);
            }
        });
        if let Some(status) = &state.status {
            match status {
                Status::Info(m) => ui.small(m),
                Status::Error(m) => ui.colored_label(Color32::LIGHT_RED, m),
            };
        }
    });
    egui::CentralPanel::default().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.strong(&state.profile.name);
            if state.is_dirty() {
                ui.weak("(edited)");
            }
            if state.ctx.game_running {
                ui.colored_label(Color32::LIGHT_GREEN, "running");
            }
        });
        ui.add(egui::TextEdit::singleline(&mut state.ui.search).hint_text("Filter favourites"));
        let rows = state.visible_rows();
        if rows.is_empty() {
            ui.weak("No favourites yet. Star convars in the full view.");
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for name in rows {
                views::compact_row(ui, state, &name);
            }
        });
    });
}
