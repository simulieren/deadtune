//! The eframe shell: screens, per-frame polling, top bar, banner and footer.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant, SystemTime};

use dt_core::addons::AddonId;
use dt_core::addons::guard::Pak;
use dt_core::hud::install::HudRollback;
use dt_core::launch;
use dt_core::locate::{self, GamePaths};
use dt_core::profile::Profile;
use eframe::egui::{self, ViewportCommand};

use crate::live::PushOutcome;
use crate::relaunch::{self, Relaunch};
use crate::settings::{Settings, View};
use crate::state::{AppState, HudPage, MinimapPreset, Mode, Section, Status, Tab, TopBarPreview};
use crate::update::{self, UpdateState};
use crate::{Args, advanced, compact, profiles, simple};
use dt_core::hud::topbar::TopBarPreset;

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
    /// Frames spent waiting for the UI images page to finish decoding.
    waited: u32,
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
    away: Away,
    /// Shared with the open game's state; served on 127.0.0.1 for the live HUD's page.
    web: dt_core::hud::web_bridge::Bridge,
}

/// How long the window can sit unfocused (behind the game) before DeadTune gives memory
/// back as if it were minimized.
const AWAY_AFTER: Duration = Duration::from_secs(60);

/// Whether the window is out of sight, for giving memory back once per absence.
#[derive(Default)]
struct Away {
    unfocused_since: Option<Instant>,
    /// Pictures were released this absence; the trim waits a frame so the freed textures
    /// are gone first.
    released: bool,
    trimmed: bool,
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
            away: Away::default(),
            web: Default::default(),
            screenshot: screenshot.map(|path| Screenshot {
                path,
                frames: 0,
                waited: 0,
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
        if !fake_running() {
            self.game_poll = Some(spawn_game_poll(ctx.clone()));
        }
        if let Ok(exe) = update::exe() {
            dt_core::update::cleanup(exe);
        }
        let wake = ctx.clone();
        let _ = dt_core::hud::web_bridge::serve(
            self.web.clone(),
            dt_core::hud::web_bridge::PORT,
            move || wake.request_repaint(),
        );
        if let Screen::Main(state) = &mut self.screen
            && update::AVAILABLE
            && state.update.state == UpdateState::Idle
            && update::should_check(
                &state.settings.update,
                chrono::Utc::now(),
                dt_core::update::Current::this_build().as_ref(),
            )
        {
            state.check_update(false);
        }
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
                state.web = self.web.clone();
                if let Some(status) = state.sync_ingame() {
                    state.status = Some(status);
                }
                if let Err(e) = state.start_watch() {
                    state.status = Some(Status::Error(format!("file watcher: {e}")));
                }
                if self.args.compact {
                    state.ui.mode = Mode::Compact;
                }
                // Screenshot lever: `DEADTUNE_SECTION=shadows` opens that simple-view section.
                if let Ok(name) = std::env::var("DEADTUNE_SECTION")
                    && let Some(section) = find_section(&name)
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
                // `DEADTUNE_STATS_SELECT=souls` opens the Player stats page on that part.
                if let Ok(name) = std::env::var("DEADTUNE_STATS_SELECT") {
                    let name = name.to_lowercase();
                    state.ui.stats_selected = dt_core::hud::player_stats::StatsPart::ALL
                        .into_iter()
                        .find(|p| {
                            p.spec()
                                .label
                                .to_lowercase()
                                .split_whitespace()
                                .any(|w| w.starts_with(&name))
                        });
                }
                // `DEADTUNE_STATS_LOOK=items:rounded,souls:mono` applies part looks.
                if let Ok(value) = std::env::var("DEADTUNE_STATS_LOOK") {
                    let mut style = state.profile.hud.player_stats.clone();
                    for (part, look) in value.split(',').filter_map(|kv| kv.split_once(':')) {
                        let part = dt_core::hud::player_stats::StatsPart::ALL
                            .into_iter()
                            .find(|p| p.spec().label.to_lowercase().contains(part.trim()));
                        let found = part.and_then(|part| {
                            part.looks()
                                .iter()
                                .find(|l| l.label.to_lowercase().starts_with(look.trim()))
                                .map(|l| l.applied(&style, part))
                        });
                        match found {
                            Some(next) => style = next,
                            None => eprintln!("DEADTUNE_STATS_LOOK: no look {part:?}:{look}"),
                        }
                    }
                    state.set_player_stats_style(style);
                }
                // `DEADTUNE_HUD_BACKDROP=off` hides the layout preview's game screenshot,
                // `=40` draws it at 40% opacity.
                match std::env::var("DEADTUNE_HUD_BACKDROP").as_deref() {
                    Ok("off") => state.ui.hud_backdrop.shown = false,
                    Ok(pct) => {
                        if let Ok(pct) = pct.parse::<f32>() {
                            state.ui.hud_backdrop.opacity = (pct / 100.0).clamp(0.1, 1.0);
                        }
                    }
                    Err(_) => {}
                }
                // `DEADTUNE_HUD_PAGE=colors` opens the Minimap colours page in either view,
                // `DEADTUNE_HUD_PAGE=top` the Top bar page, `DEADTUNE_HUD_PAGE=ingame` the
                // In-game settings page;
                // `DEADTUNE_MINIMAP_PRESET=colourblind` (or `contrast`) applies a colour preset.
                match std::env::var("DEADTUNE_HUD_PAGE").as_deref() {
                    Ok(v) if v.starts_with("colo") => {
                        state.ui.hud_page = HudPage::Colors;
                        state.ui.section = Section::Minimap;
                    }
                    Ok(v) if v.starts_with("top") => {
                        state.ui.hud_page = HudPage::TopBar;
                        state.ui.section = Section::TopBar;
                    }
                    Ok(v) if v.starts_with("in") => {
                        state.ui.hud_page = HudPage::Ingame;
                        state.ui.section = Section::Ingame;
                    }
                    Ok(v) if v.starts_with("im") => {
                        state.ui.hud_page = HudPage::Images;
                        state.ui.section = Section::Images;
                    }
                    _ => {}
                }
                self.startup_profile(&mut state);
                hud_edit_lever(&mut state);
                images_levers(&mut state);
                // Screenshot lever: `DEADTUNE_HEALTH_PRESET=pill` opens the Health bar page on
                // that style (its name in lower case, spaces dropped).
                if let Ok(name) = std::env::var("DEADTUNE_HEALTH_PRESET") {
                    use dt_core::hud::health_style::HealthPreset;
                    match HealthPreset::ALL
                        .into_iter()
                        .find(|p| p.label().to_lowercase().replace(' ', "") == name)
                    {
                        Some(preset) => state.set_health_style(preset.style()),
                        None => eprintln!("DEADTUNE_HEALTH_PRESET: no style {name}"),
                    }
                }
                // `DEADTUNE_PREVIEW_IMAGES=<folder>` draws the HUD previews from a "Save all
                // images" folder first; `DEADTUNE_PREVIEW_SHAPES=1` draws only their shapes.
                state.hud_art.from = std::env::var_os("DEADTUNE_PREVIEW_IMAGES")
                    .filter(|v| !v.is_empty())
                    .map(PathBuf::from);
                state.hud_art.shapes_only =
                    std::env::var_os("DEADTUNE_PREVIEW_SHAPES").is_some_and(|v| v == "1");
                if let Ok(name) = std::env::var("DEADTUNE_MINIMAP_PRESET") {
                    let preset = MinimapPreset::ALL.into_iter().find(|p| {
                        p.label()
                            .to_lowercase()
                            .replace(['-', ' '], "")
                            .contains(&name.to_lowercase())
                    });
                    state.apply_minimap_preset(preset);
                }
                // `DEADTUNE_TOP_BAR_PRESET=fight` (any prefix of a preset label) applies a top
                // bar preset; `DEADTUNE_TOP_BAR_PREVIEW=none` shows every mock hero in vision.
                if let Ok(name) = std::env::var("DEADTUNE_TOP_BAR_PRESET")
                    && let Some(preset) = TopBarPreset::ALL
                        .into_iter()
                        .find(|p| p.label().to_lowercase().starts_with(&name.to_lowercase()))
                {
                    state.apply_top_bar_preset(preset);
                }
                if std::env::var("DEADTUNE_TOP_BAR_PREVIEW").is_ok_and(|v| v == "none") {
                    state.ui.top_bar_preview = TopBarPreview {
                        missing_enemy: false,
                        dead_hero: false,
                    };
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
                // `DEADTUNE_BASE=sidelock` picks that preset; `DEADTUNE_OPEN_PRESETS=1` opens
                // the Overview's "All presets" dropdown.
                if let Ok(key) = std::env::var("DEADTUNE_BASE")
                    && let Some(info) = dt_core::preset::all().iter().find(|p| p.id.key() == key)
                {
                    state.set_base(dt_core::profile::BaseRef::Preset(info.id));
                }
                state.ui.open_presets = std::env::var_os("DEADTUNE_OPEN_PRESETS").is_some();
                // `DEADTUNE_FAKE_TRIAL=failed:vindicta_scope,hud` shows the launch guard's
                // failure banner, details open, for those paks (every addon if none are
                // listed) with a captured FATAL line; `restored:hud` shows the HUD put back to
                // its last working copy instead of removed.
                if let Ok(spec) = std::env::var("DEADTUNE_FAKE_TRIAL")
                    && let Some((kind, list)) = spec
                        .strip_prefix("failed")
                        .map(|l| ("failed", l))
                        .or_else(|| spec.strip_prefix("restored").map(|l| ("restored", l)))
                {
                    let ids = match fake_paks(list) {
                        ids if ids.is_empty() => AddonId::ALL.map(Pak::Addon).to_vec(),
                        ids => ids,
                    };
                    state.inject_trial_failure(
                        ids,
                        Some(
                            "FATAL ERROR: Unable to read default keybinding configuration user_keys_default"
                                .into(),
                        ),
                    );
                    if kind == "restored"
                        && let Some(hud) = state.guard.failure.as_mut().and_then(|f| f.hud.as_mut())
                    {
                        hud.rollback = HudRollback::Restored;
                    }
                    state.ui.guard_details = true;
                }
                // Screenshot lever: `DEADTUNE_FAKE_PUSH=waiting|confirmed|mixed|timeout` shows
                // that live-status; `DEADTUNE_FAKE_BOOT=1` pretends the boot cfg ran.
                if let Ok(kind) = std::env::var("DEADTUNE_FAKE_PUSH") {
                    fake_push(&mut state, &kind);
                }
                // `DEADTUNE_FAKE_RUNNING=1` pretends Deadlock is running (no game polling).
                if fake_running() {
                    state.observe_game(true, None);
                }
                // `DEADTUNE_FAKE_LIVE_HUD=off|not_installed|closed|waiting|waiting_long|
                // waiting_page|stale|stale_base|live|error` puts the HUD pages' live preview
                // line in that state.
                if let Ok(kind) = std::env::var("DEADTUNE_FAKE_LIVE_HUD") {
                    fake_live_hud(&mut state, &kind);
                }
                // `DEADTUNE_FAKE_LIVE_CHECK=showing|shown|asking|<sample>` opens "Check live
                // preview" at that point; samples are `dt_core::hud::live_check::sample`'s.
                if let Ok(kind) = std::env::var("DEADTUNE_FAKE_LIVE_CHECK") {
                    fake_live_check(&mut state, &kind);
                }
                // `DEADTUNE_FAKE_STATUS=error:<raw>`, `warn:<raw>` or `info:<text>`.
                if let Some((kind, text)) =
                    std::env::var("DEADTUNE_FAKE_STATUS").ok().and_then(|v| {
                        v.split_once(':')
                            .map(|(k, t)| (k.to_string(), t.to_string()))
                    })
                {
                    state.status = match kind.as_str() {
                        "error" => Some(Status::Error(text)),
                        "warn" => Some(Status::Warn(text)),
                        _ => Some(Status::Info(text)),
                    };
                }
                // `DEADTUNE_LAUNCH_ARGS="-vulkan -nosplash"` replaces the launch options;
                // `DEADTUNE_LAUNCH_OPTIONS=1` opens their window.
                if let Ok(line) = std::env::var("DEADTUNE_LAUNCH_ARGS") {
                    state.settings.launch = dt_core::launch_options::LaunchOptions::from_args(
                        &dt_core::launch_options::split_command_line(&line),
                    );
                }
                state.ui.launch_options_open =
                    std::env::var_os("DEADTUNE_LAUNCH_OPTIONS").is_some_and(|v| v == "1");
                // `DEADTUNE_FAKE_UPDATE=0.9.0` offers that version instead of checking.
                if let Some(version) = std::env::var("DEADTUNE_FAKE_UPDATE")
                    .ok()
                    .and_then(|v| v.parse().ok())
                {
                    state.update.state = UpdateState::Available(update::fake_release(version));
                }
                if std::env::var_os("DEADTUNE_FAKE_BOOT").is_some_and(|v| v == "1") {
                    state.ack.boot = Some(dt_core::bridge::ack::Boot {
                        version: env!("CARGO_PKG_VERSION").into(),
                        at: SystemTime::now(),
                    });
                }
                // `DEADTUNE_FAKE_SNAPSHOT=running` shows the Game files page mid-snapshot.
                if std::env::var("DEADTUNE_FAKE_SNAPSHOT").is_ok_and(|v| v == "running") {
                    state.inject_snapshot_running();
                }
                // `DEADTUNE_SNAPSHOT_IMAGES=hud` picks the Game files page's image scope.
                if let Some(scope) = std::env::var("DEADTUNE_SNAPSHOT_IMAGES")
                    .ok()
                    .and_then(|v| dt_core::snapshot::ImageScope::parse(&v))
                {
                    state.settings.snapshots.selection.images = scope;
                }
                // Screenshot lever: `DEADTUNE_SET=fps_max=144,r_shadows=true` edits after loading.
                if let Ok(list) = std::env::var("DEADTUNE_SET") {
                    for (name, value) in list.split(',').filter_map(|kv| kv.split_once('=')) {
                        if let Err(e) = state.set_convar(name.trim(), value.trim().to_string()) {
                            state.status = Some(Status::Error(e.to_string()));
                        }
                    }
                }
                // `DEADTUNE_PRACTICE=shadows,fog` turns those practice mode groups on.
                if let Ok(list) = std::env::var("DEADTUNE_PRACTICE") {
                    let mut mode = state.profile.practice;
                    for group in dt_core::practice::Group::ALL {
                        if list.split(',').any(|g| g.trim() == group.id()) {
                            mode.set(group, true);
                        }
                    }
                    state.set_practice(mode);
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
        if state.ranked_safe_blocks_auto_profile() {
            state.status = Some(Status::Info(format!(
                "Ranked-safe mode is on, so the {name} profile for {source:?} power was not switched in."
            )));
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
        state.poll_snapshot();
        state.poll_images_export();
        if state.snapshot_job.is_some() || state.images_export_running() {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        if state.poll_update() {
            crate::update_view::restart(ctx, state);
        }
        if state.update.state.busy() {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
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
        if state.live_hud.busy() {
            ctx.request_repaint_after(Duration::from_millis(50));
        } else if state.live_hud.connected() {
            ctx.request_repaint_after(Duration::from_secs(1));
        }
        state.tick_live_check(Instant::now());
        if state.live_check.active() {
            ctx.request_repaint_after(Duration::from_millis(200));
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

    /// Frees pictures no page drew lately; when the window is minimized, or unfocused for
    /// [`AWAY_AFTER`], frees them all and trims the working set, once per absence.
    fn rest(&mut self, ctx: &egui::Context, frame_start: Instant, minimized: bool, focused: bool) {
        let away = &mut self.away;
        away.unfocused_since = if focused {
            None
        } else {
            away.unfocused_since.or(Some(frame_start))
        };
        let unfocused_for = away
            .unfocused_since
            .map(|since| frame_start.saturating_duration_since(since));
        let gone = minimized || unfocused_for.is_some_and(|d| d >= AWAY_AFTER);
        if !gone {
            away.released = false;
            away.trimmed = false;
            if let Some(d) = unfocused_for {
                ctx.request_repaint_after(AWAY_AFTER.saturating_sub(d));
            }
        }
        let release_all = gone && !away.released;
        if let Screen::Main(state) = &mut self.screen {
            state.release_pictures(ctx, frame_start, release_all);
        }
        if release_all {
            away.released = true;
            ctx.request_repaint();
        } else if gone && !away.trimmed {
            away.trimmed = true;
            dt_core::memory::trim_working_set();
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
            simple::apply(ctx, state);
        }
        // `DEADTUNE_FAKE_TRIAL=verified` marks whatever Apply just installed as started with;
        // `testing:hud,blur_disabler` puts those paks on trial as if the game had just started.
        if job.frames == 6
            && let Ok(spec) = std::env::var("DEADTUNE_FAKE_TRIAL")
            && let Screen::Main(state) = &mut self.screen
        {
            if spec == "verified" {
                state.inject_trial_verified();
            } else if let Some(list) = spec.strip_prefix("testing") {
                state.inject_trial_started(fake_paks(list));
            }
        }
        // `DEADTUNE_FAKE_CLOSE=1` closes the faked game after the Apply, so pak changes left
        // for when it closes go in (with `DEADTUNE_FAKE_RUNNING=1`).
        if job.frames == 7
            && std::env::var_os("DEADTUNE_FAKE_CLOSE").is_some_and(|v| v == "1")
            && let Screen::Main(state) = &mut self.screen
        {
            state.observe_game(false, None);
        }
        // Hold the capture while the UI images page or a HUD preview is still decoding.
        let decoding = matches!(&self.screen, Screen::Main(state)
            if state.images.thumbs.as_ref().is_some_and(|t| t.busy()) || state.hud_art.busy());
        if job.frames == 19 && decoding && job.waited < 600 {
            job.frames -= 1;
            job.waited += 1;
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

/// `:hud,blur_disabler` from a `DEADTUNE_FAKE_TRIAL` value.
fn fake_paks(list: &str) -> Vec<Pak> {
    list.trim_start_matches(':')
        .split(',')
        .filter_map(|s| Pak::parse(s.trim()))
        .collect()
}

/// A faked trial needs the game to look running, or the next poll ends it as an early exit.
/// A section by the start of its label or of any word in it (`images` finds "UI images").
fn find_section(name: &str) -> Option<Section> {
    let name = name.to_lowercase();
    let starts = |s: &Section| s.label().to_lowercase().starts_with(&name);
    let word = |s: &Section| {
        s.label()
            .to_lowercase()
            .split_whitespace()
            .any(|w| w.starts_with(&name))
    };
    Section::ALL
        .into_iter()
        .find(starts)
        .or_else(|| Section::ALL.into_iter().find(word))
}

/// Screenshot lever: `DEADTUNE_HUD_EDIT=minimap:-300:-200:150,top:0:40:100` moves (x, y in
/// 1080p px) and sizes (percent) the HUD elements whose labels start with each name.
fn hud_edit_lever(state: &mut AppState) {
    let Ok(list) = std::env::var("DEADTUNE_HUD_EDIT") else {
        return;
    };
    for entry in list.split(',') {
        let mut parts = entry.split(':');
        let name = parts.next().unwrap_or_default().to_lowercase();
        let mut numbers = parts.map(|n| n.trim().parse::<i32>().unwrap_or_default());
        let Some(spec) = dt_core::hud::elements::ELEMENTS
            .iter()
            .find(|s| !name.is_empty() && s.label.to_lowercase().starts_with(&name))
        else {
            continue;
        };
        let mut edit = state.hud_edit(spec.id);
        edit.offset_x = numbers.next().unwrap_or(edit.offset_x);
        edit.offset_y = numbers.next().unwrap_or(edit.offset_y);
        if let Some(pct) = numbers.next() {
            edit.scale_pct = pct.clamp(25, 300) as u16;
        }
        state.set_hud_element(spec.id, edit);
    }
}

/// Screenshot levers for the UI images page: `DEADTUNE_IMAGES_FROM=<snapshot folder>`
/// previews a snapshot's images instead of the game's, `DEADTUNE_IMAGES_FOLDER=hud/top_bar`
/// opens a folder, `DEADTUNE_IMAGES_SEARCH=ping` searches, `DEADTUNE_IMAGES_SELECT=<part of
/// a path>` selects the first match, `DEADTUNE_IMAGES_SET=<part of a path>=<file>,...`
/// replaces images with files, `DEADTUNE_IMAGES_ZOOM=1` shows the preview at 1:1,
/// `DEADTUNE_IMAGES_EXPORT=running` shows "Save all images" mid-way and `=done` (or `=zip`)
/// runs it for real first. Colour edits: `DEADTUNE_IMAGES_EDIT=<part>=<spec>[;<spec>],...`,
/// `DEADTUNE_IMAGES_BULK=<spec>[;<spec>]` on every visible image, `DEADTUNE_IMAGES_MARK=<part>,...`
/// and `DEADTUNE_IMAGES_UNDO=<n>`. Files in: `DEADTUNE_IMAGES_FIT=fill|stretch|own` sets the
/// selected replacement's fit, `DEADTUNE_IMAGES_IMPORT=<file or folder>,...` drops them (a
/// collection waits in its review card), `DEADTUNE_IMAGES_HOVER=<path>,...` draws the drop
/// overlay as if those were dragged over the window.
fn images_levers(state: &mut AppState) {
    let var = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    let mut used = false;
    if let Some(dir) = var("DEADTUNE_IMAGES_FROM") {
        state.images.from = Some(PathBuf::from(dir));
        used = true;
    }
    match var("DEADTUNE_IMAGES_EXPORT").as_deref() {
        Some("running") => state.inject_images_export_running(),
        Some(how @ ("done" | "zip")) => {
            state.images.export_all.zip = how == "zip";
            state.start_images_export(None);
            state.wait_images_export();
        }
        _ => {}
    }
    state.images.folder = var("DEADTUNE_IMAGES_FOLDER");
    if let Some(query) = var("DEADTUNE_IMAGES_SEARCH") {
        state.images.search = query;
    }
    if var("DEADTUNE_IMAGES_ZOOM").is_some() {
        state.images.zoom = crate::images::Zoom::Actual;
    }
    let find = |state: &AppState, part: &str| {
        state
            .image_library()?
            .entries
            .iter()
            .find(|e| e.path.contains(part))
            .map(|e| e.path.clone())
    };
    let sets = var("DEADTUNE_IMAGES_SET");
    let select = var("DEADTUNE_IMAGES_SELECT");
    let edits = var("DEADTUNE_IMAGES_EDIT");
    let bulk = var("DEADTUNE_IMAGES_BULK");
    let marks = var("DEADTUNE_IMAGES_MARK");
    let undo = var("DEADTUNE_IMAGES_UNDO").and_then(|n| n.parse::<usize>().ok());
    let fit = var("DEADTUNE_IMAGES_FIT").and_then(|f| dt_core::texture::encode::Fit::parse(&f));
    let import = var("DEADTUNE_IMAGES_IMPORT");
    if let Some(hover) = var("DEADTUNE_IMAGES_HOVER") {
        state.images.hover_lever = Some(hover.split(',').map(PathBuf::from).collect());
    }
    if !(used
        || import.is_some()
        || sets.is_some()
        || select.is_some()
        || edits.is_some()
        || bulk.is_some()
        || marks.is_some())
    {
        return;
    }
    state.load_images();
    for (part, file) in sets
        .iter()
        .flat_map(|v| v.split(','))
        .filter_map(|kv| kv.split_once('='))
    {
        match (find(state, part.trim()), std::fs::read(file.trim())) {
            (Some(path), Ok(bytes)) => state.replace_image(&path, &bytes),
            _ => eprintln!("DEADTUNE_IMAGES_SET: no image matching {part} or no file {file}"),
        }
    }
    if let Some(Err(e)) = edits.map(|v| state.edit_lever(&v)) {
        eprintln!("DEADTUNE_IMAGES_EDIT: {e}");
    }
    if let Some(Err(e)) = bulk.map(|v| state.bulk_lever(&v)) {
        eprintln!("DEADTUNE_IMAGES_BULK: {e}");
    }
    if let Some(part) = select {
        let path = find(state, &part);
        state.select_image(path);
    }
    if let Some(fit) = fit {
        state.set_image_fit(fit);
    }
    if let Some(value) = marks {
        state.mark_lever(&value);
    }
    if let Some(files) = import {
        let items = files
            .split(',')
            .map(|f| crate::images_collect::Incoming::from_path(std::path::Path::new(f.trim())))
            .collect();
        state.receive_files(items);
    }
    for _ in 0..undo.unwrap_or(0) {
        state.undo_images();
    }
}

fn fake_running() -> bool {
    std::env::var_os("DEADTUNE_FAKE_RUNNING").is_some_and(|v| v == "1")
        || std::env::var("DEADTUNE_FAKE_TRIAL").is_ok_and(|v| v.starts_with("testing"))
}

fn fake_live_check(state: &mut AppState, kind: &str) {
    use crate::live_check::LiveCheck;
    let now = Instant::now();
    match kind {
        "showing" | "shown" => {
            let applied = (kind == "shown").then(|| now - Duration::from_secs(1));
            state.live_hud.inject_flash(applied);
            state.live_check = LiveCheck::Showing {
                since: now - Duration::from_secs(3),
            };
        }
        "asking" => state.live_check = LiveCheck::Asking { seq: 1 },
        sample => {
            if let Some(facts) = dt_core::hud::live_check::sample(sample) {
                state.show_live_check(facts);
            }
        }
    }
}

fn fake_live_hud(state: &mut AppState, kind: &str) {
    use crate::live_hud::LiveHud;
    let now = Instant::now();
    let live = LiveHud::Live {
        base: "1a2b3c4d".into(),
        seq: Some(7),
        acked: Some(now - Duration::from_secs(2)),
        undone: false,
    };
    let long = LiveHud::Waiting {
        since: now - Duration::from_secs(30),
    };
    let (fake, heard) = match kind {
        "off" => (LiveHud::Off, false),
        "not_installed" => (LiveHud::NotInstalled, false),
        "closed" => (LiveHud::GameClosed, false),
        "waiting" => (LiveHud::Waiting { since: now }, false),
        "waiting_long" => (long, false),
        "waiting_page" => (long, true),
        "stale" => (LiveHud::Stale { base: None }, false),
        "stale_base" => (
            LiveHud::Stale {
                base: Some("0badf00d".into()),
            },
            false,
        ),
        "live" => (live, true),
        "error" => (
            LiveHud::Error("The live preview can't read this layout: bad CSS".into()),
            false,
        ),
        _ => return,
    };
    state.set_live_preview(fake != LiveHud::Off);
    state.live_hud.inject(fake, heard.then_some("1a2b3c4d"));
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
            after: dt_core::bridge::ack::TIMEOUT,
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
        let frame_start = Instant::now();
        self.poll(&ctx);
        let (minimized, focused) = ctx.input(|i| {
            let viewport = i.viewport();
            (
                viewport.minimized == Some(true),
                viewport.focused != Some(false),
            )
        });
        let mut reopen = None;
        match &mut self.screen {
            Screen::Main(_) if minimized => {}
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
        if let Screen::Main(state) = &mut self.screen
            && state.ui.launch_options_open
        {
            crate::launch_view::window(&ctx, state);
        }
        if let Some(settings) = reopen {
            self.open_game(settings);
        }
        self.rest(&ctx, frame_start, minimized, focused);
        self.screenshot(&ctx);
    }
}
