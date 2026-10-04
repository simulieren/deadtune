//! The eframe shell: screens, per-frame polling, top bar, banner and footer.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant, SystemTime};

use dt_core::addons::AddonId;
use dt_core::launch;
use dt_core::locate::{self, GamePaths};
use dt_core::profile::Profile;
use eframe::egui::{self, ViewportCommand};

use crate::live::PushOutcome;
use crate::relaunch::{self, Relaunch};
use crate::settings::{Settings, View};
use crate::state::{AppState, HudPage, MinimapPreset, Mode, Section, Status, Tab};
use crate::{Args, advanced, compact, profiles, simple, views};

pub const FULL_SIZE: [f32; 2] = [1280.0, 820.0];
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
}

fn resolve_paths(game_dir: Option<&Path>) -> Result<GamePaths, String> {
    match game_dir {
        Some(dir) => locate::from_game_root(dir).map_err(|e| e.to_string()),
        None => locate::locate().map_err(|e| e.to_string()),
    }
}

fn spawn_game_poll(ctx: egui::Context) -> Receiver<(bool, Option<SystemTime>)> {
    let (tx, rx) = channel();
    // Screenshot lever: `DEADTUNE_FAKE_GAME=1` reports the game as running.
    let fake = std::env::var_os("DEADTUNE_FAKE_GAME").is_some_and(|v| v == "1");
    std::thread::spawn(move || {
        loop {
            let running = fake || launch::is_game_running();
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
        crate::theme::install(ctx);
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
                // Screenshot lever: `DEADTUNE_SECTION=shadows` opens that simple-view section.
                if let Ok(name) = std::env::var("DEADTUNE_SECTION")
                    && let Some(section) = Section::ALL
                        .into_iter()
                        .find(|s| s.label().to_lowercase().starts_with(&name.to_lowercase()))
                {
                    state.ui.section = section;
                }
                // `DEADTUNE_TAB=video` opens the advanced view on that tab.
                if let Ok(name) = std::env::var("DEADTUNE_TAB")
                    && let Some(tab) = Tab::ALL
                        .into_iter()
                        .find(|t| t.label().to_lowercase().starts_with(&name.to_lowercase()))
                {
                    state.settings.view = View::Advanced;
                    state.ui.tab = tab;
                }
                if let Ok(query) = std::env::var("DEADTUNE_SEARCH") {
                    state.ui.query = query;
                }
                if let Ok(name) = std::env::var("DEADTUNE_HUD_SELECT")
                    && !name.is_empty()
                {
                    state.ui.hud_selected = dt_core::hud::elements::ELEMENTS
                        .iter()
                        .find(|s| s.label.to_lowercase().starts_with(&name.to_lowercase()))
                        .map(|s| s.id);
                }
                // `DEADTUNE_HUD_PAGE=colors` opens the HUD tab's Minimap colours page;
                // `DEADTUNE_MINIMAP_PRESET=colourblind` (or `contrast`) applies a colour preset.
                if std::env::var("DEADTUNE_HUD_PAGE").is_ok_and(|v| v.starts_with("colo")) {
                    state.ui.hud_page = HudPage::Colors;
                }
                self.startup_profile(&mut state);
                if let Ok(name) = std::env::var("DEADTUNE_MINIMAP_PRESET") {
                    let preset = MinimapPreset::ALL.into_iter().find(|p| {
                        p.label()
                            .to_lowercase()
                            .replace(['-', ' '], "")
                            .contains(&name.to_lowercase())
                    });
                    state.apply_minimap_preset(preset);
                }
                // `DEADTUNE_ADDONS=particle_disabler,blur_disabler` turns addons on after loading;
                // `DEADTUNE_ADDON_EXPAND=particle_disabler` unfolds that card's options.
                if let Ok(list) = std::env::var("DEADTUNE_ADDONS") {
                    for id in list.split(',').filter_map(|s| AddonId::parse(s.trim())) {
                        state.set_addon_enabled(id, true);
                    }
                }
                if let Ok(id) = std::env::var("DEADTUNE_ADDON_EXPAND") {
                    state.ui.addon_expanded = AddonId::parse(id.trim());
                }
                // Screenshot lever: `DEADTUNE_FAKE_PUSH=waiting|confirmed|mixed|timeout` shows
                // that live-status; `DEADTUNE_FAKE_BOOT=1` pretends the boot cfg ran.
                if let Ok(kind) = std::env::var("DEADTUNE_FAKE_PUSH") {
                    fake_push(&mut state, &kind);
                }
                if std::env::var_os("DEADTUNE_FAKE_BOOT").is_some_and(|v| v == "1") {
                    state.ack.boot = Some(dt_core::bridge::ack::Boot {
                        version: env!("CARGO_PKG_VERSION").into(),
                        at: SystemTime::now(),
                    });
                }
                // Screenshot lever: `DEADTUNE_SET=fps_max=144,r_shadows=true` edits after loading.
                if let Ok(list) = std::env::var("DEADTUNE_SET") {
                    for (name, value) in list.split(',').filter_map(|kv| kv.split_once('=')) {
                        if let Err(e) = state.set_convar(name.trim(), value.trim().to_string()) {
                            state.status = Some(Status::Error(e.to_string()));
                        }
                    }
                }
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
        state.poll_checks();
        state.poll_build();
        if state.texture_build.is_some() {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
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
                && let Err(e) = state.launch_game()
            {
                state.relaunch = Relaunch::Failed(e);
            }
        }
        if let Some(result) = state.tick_live(Instant::now()) {
            report_push(ctx, state, result);
        }
        if state.live_push.is_pending() {
            ctx.request_repaint_after(crate::live::DEBOUNCE);
        }
        state.poll_conlog(Instant::now());
        if state.ack.is_waiting() {
            ctx.request_repaint_after(Duration::from_millis(250));
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

    fn screenshot(&mut self, ctx: &egui::Context) {
        let Some(job) = &mut self.screenshot else {
            return;
        };
        job.frames += 1;
        ctx.request_repaint();
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

fn fake_push(state: &mut AppState, kind: &str) {
    use dt_core::bridge::ack::{Outcome, PushStatus};
    let results = |items: &[(&str, Outcome)]| {
        items
            .iter()
            .map(|(n, o)| (n.to_string(), o.clone()))
            .collect()
    };
    let status = match kind {
        "waiting" => PushStatus::Waiting {
            since: Instant::now() - Duration::from_secs(3),
            count: 3,
        },
        "confirmed" => PushStatus::Confirmed {
            at: SystemTime::now(),
            results: results(&[
                ("fps_max", Outcome::Applied("240".into())),
                ("r_farz", Outcome::Applied("6000".into())),
                ("r_shadows", Outcome::Applied("false".into())),
            ]),
        },
        "mixed" => PushStatus::Confirmed {
            at: SystemTime::now(),
            results: results(&[
                ("fps_max", Outcome::Applied("240".into())),
                ("r_farz", Outcome::Applied("6000".into())),
                ("r_xyz", Outcome::Rejected("Unknown command 'r_xyz'".into())),
            ]),
        },
        "timeout" => PushStatus::TimedOut {
            after: Duration::from_secs(10),
            count: 3,
        },
        _ => return,
    };
    state.ack.inject(status);
}

/// A sent batch leaves the status line to the live-status indicator, which follows the reply.
pub fn report_push(ctx: &egui::Context, state: &mut AppState, result: Result<PushOutcome, String>) {
    state.status = match result {
        Ok(PushOutcome::Sent { .. }) => None,
        Ok(PushOutcome::Copy(text)) => {
            ctx.copy_text(text);
            Some(Status::Info(
                "console commands copied; paste into the console (F7)".into(),
            ))
        }
        Err(e) => Some(Status::Error(format!("push failed: {e}"))),
    };
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);
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
                (Mode::Compact, _) => compact::ui(ui, state),
                (Mode::Full, View::Simple) => simple::simple(ui, state),
                (Mode::Full, View::Advanced) => advanced::full_ui(ui, state, &mut reopen),
            },
        }
        if let Some(settings) = reopen {
            self.open_game(settings);
        }
        self.screenshot(&ctx);
    }
}
