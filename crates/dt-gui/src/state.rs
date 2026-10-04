//! Everything the window shows, and every transition on it, without a window.

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Instant, SystemTime};

use dt_core::apply::{
    self, ApplyContext, ApplyError, ApplyPlan, ApplyReport, BaseTexts, FileWrite,
};
use dt_core::backup::{BackupEntry, BackupStore, FileKind, sha256_hex};
use dt_core::bridge::ConsoleCmd;
use dt_core::catalog::{ApplyClass, Catalog};
use dt_core::gi::{self, Override};
use dt_core::hud::elements::ElementId;
use dt_core::hud::install::HudPlan;
use dt_core::hud::layout::{ElementEdit, HudLayout};
use dt_core::locate::GamePaths;
use dt_core::preset::PresetId;
use dt_core::profile::{self, BaseRef, ConVarEdits, Profile};
use dt_core::watch::{self, Change, Watcher};

use crate::bench::{self, BenchState};
use crate::live::{BridgeTarget, LivePush, PushOutcome};
use crate::profiles;
use crate::relaunch::Relaunch;
use crate::settings::{Settings, TargetSource};
use dt_core::doctor::Check;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveFiles {
    pub gameinfo: String,
    pub video: Option<String>,
}

impl LiveFiles {
    pub fn read(paths: &GamePaths) -> io::Result<LiveFiles> {
        Ok(LiveFiles {
            gameinfo: std::fs::read_to_string(&paths.gameinfo)?,
            video: std::fs::read_to_string(&paths.video).ok(),
        })
    }
}

/// The resolved base preset and the values it gives each convar.
#[derive(Clone, Debug)]
pub struct Base {
    pub texts: BaseTexts,
    pub values: BTreeMap<String, String>,
}

impl Base {
    fn resolve(profile: &Profile) -> Result<Base, String> {
        let texts = apply::resolve_base(profile).map_err(|e| e.to_string())?;
        let values = gi::effective_values(&texts.gameinfo).map_err(|e| e.to_string())?;
        Ok(Base { texts, values })
    }
}

/// Where a convar's target value comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Setting {
    Override(String),
    /// The profile comments the line out, so the engine default applies.
    CommentedOut,
    Base(String),
    EngineDefault,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EditError {
    Denied(String),
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditError::Denied(name) => write!(f, "{name} is on the denylist and cannot be changed"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Banner {
    /// `(from, to)` build ids when Steam updated the game.
    pub build: Option<(Option<String>, Option<String>)>,
    /// What changed in gameinfo.gi, old live file to new.
    pub diff: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingRestart {
    pub names: Vec<String>,
    pub since: SystemTime,
}

/// What the player starts from in the welcome flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartChoice {
    Preset(PresetId),
    /// The files as they are now become the base; nothing is written.
    KeepCurrent,
}

/// First-run flow once the game is found (finding it is `app::Screen::FindGame`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Welcome {
    PickStart { choice: Option<StartChoice> },
    Done { needs_restart: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Full,
    Compact,
}

/// A new tab (e.g. HUD) is one variant here, an entry in `ALL` and `label`, and one arm in
/// `app::full_ui`; the compiler flags `label` and `full_ui`, not `ALL`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    ConVars,
    Video,
    Hud,
    Profiles,
    Backups,
    Bench,
    Launch,
    Settings,
}

impl Tab {
    pub const ALL: [Tab; 8] = [
        Tab::ConVars,
        Tab::Video,
        Tab::Hud,
        Tab::Profiles,
        Tab::Backups,
        Tab::Bench,
        Tab::Launch,
        Tab::Settings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tab::ConVars => "ConVars",
            Tab::Video => "Video",
            Tab::Hud => "HUD",
            Tab::Profiles => "Profiles",
            Tab::Backups => "Backups",
            Tab::Bench => "Bench",
            Tab::Launch => "Launch",
            Tab::Settings => "Settings",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Scope {
    #[default]
    All,
    Changed,
    Favourites,
    Category(String),
}

#[derive(Clone, Debug, Default)]
pub struct UiState {
    pub mode: Mode,
    pub tab: Tab,
    pub search: String,
    pub scope: Scope,
    /// Path field for overrides.gi import/export on the Profiles tab.
    pub overrides_path: String,
    pub new_profile_name: String,
    pub hud_selected: Option<ElementId>,
    /// The key-bind instructions card in the simple view, opened from the setup menu.
    pub bind_help_open: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Info(String),
    Error(String),
}

/// Result of an Apply click, for the caller to finish what needs a window.
#[derive(Debug, Default)]
pub struct Applied {
    pub report: ApplyReport,
    pub copy: Option<String>,
    pub warning: Option<String>,
}

pub struct AppState {
    pub paths: GamePaths,
    pub store: BackupStore,
    pub data_dir: PathBuf,
    pub catalog: &'static Catalog,
    pub settings: Settings,
    pub profile: Profile,
    /// Last saved or loaded version; `None` for a profile never written to disk.
    pub saved: Option<Profile>,
    pub base: Result<Base, String>,
    pub live: LiveFiles,
    pub ctx: ApplyContext,
    pub preview: Result<ApplyPlan, String>,
    /// Unified diff of the previewed file writes, cached because diffing every frame is slow.
    pub preview_diff: String,
    pub banner: Option<Banner>,
    pub pending_restart: Option<PendingRestart>,
    /// Hash of the gameinfo.gi we last saw or wrote; a change to anything else is someone else's write.
    pub known_gameinfo_sha: String,
    pub live_push: LivePush,
    pub relaunch: Relaunch,
    pub bench: BenchState,
    pub ui: UiState,
    pub status: Option<Status>,
    pub welcome: Option<Welcome>,
    /// HUD plan for the layout it was built from; building reads the game archive, so it is
    /// rebuilt only when the layout changes or files on disk do.
    hud_cache: Option<(HudLayout, Result<Option<HudPlan>, String>)>,
    /// Creation time of the backup the last Undo restored; cleared by Apply.
    pub undo_cursor: Option<chrono::DateTime<chrono::Utc>>,
    /// Last "Check setup" run; `None` until the panel is opened.
    pub checks: Option<Vec<Check>>,
    #[cfg(feature = "remote")]
    pub remote: Option<crate::remote::Remote>,
    watch: Option<(Watcher, Receiver<Vec<Change>>)>,
}

/// Shown when the files changed between preview and Apply; the preview is already refreshed.
pub const STALE: &str =
    "The game files changed while you were editing. The changes were refreshed; press Apply again.";

pub fn default_profile() -> Profile {
    Profile {
        name: "My profile".into(),
        base: BaseRef::Preset(PresetId::Vanilla),
        base_rev: None,
        convars: ConVarEdits::default(),
        video: BTreeMap::new(),
        hud: HudLayout::default(),
    }
}

/// `true`/`false` or `1`/`0`, following how the current value is written.
pub fn bool_text(on: bool, like: Option<&str>) -> String {
    let digits = like.is_some_and(|v| v == "0" || v == "1");
    match (on, digits) {
        (true, true) => "1",
        (false, true) => "0",
        (true, false) => "true",
        (false, false) => "false",
    }
    .into()
}

/// Slider value back to config text: integers without a fraction, floats without trailing zeros.
pub fn fmt_num(value: f64, integer: bool) -> String {
    if integer {
        return format!("{}", value.round() as i64);
    }
    let text = format!("{value:.4}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".into()
    } else {
        text.into()
    }
}

pub fn parse_bool(value: &str) -> bool {
    matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true")
}

impl AppState {
    pub fn open(paths: GamePaths, data_dir: PathBuf, settings: Settings) -> io::Result<AppState> {
        let live = LiveFiles::read(&paths)?;
        let store = BackupStore::open(data_dir.join("backups"))?;
        let dir = profiles::dir(&data_dir);
        let (profile, saved) = match settings
            .last_profile
            .as_deref()
            .and_then(|n| profiles::load(&dir, n))
        {
            Some(p) => (p.clone(), Some(p)),
            None => (default_profile(), Some(default_profile())),
        };
        let mut state = AppState {
            known_gameinfo_sha: sha256_hex(live.gameinfo.as_bytes()),
            base: Base::resolve(&profile),
            paths,
            store,
            data_dir,
            catalog: Catalog::embedded(),
            settings,
            profile,
            saved,
            live,
            ctx: ApplyContext::default(),
            preview: Ok(ApplyPlan::default()),
            preview_diff: String::new(),
            banner: None,
            pending_restart: None,
            live_push: LivePush::default(),
            relaunch: Relaunch::Idle,
            bench: BenchState::default(),
            ui: UiState::default(),
            status: None,
            welcome: None,
            hud_cache: None,
            undo_cursor: None,
            checks: None,
            #[cfg(feature = "remote")]
            remote: None,
            watch: None,
        };
        if !state.settings.onboarded {
            state.welcome = Some(Welcome::PickStart { choice: None });
        }
        state.refresh_preview();
        state.reload_bench();
        Ok(state)
    }

    pub fn choose_start(&mut self, choice: StartChoice) -> io::Result<()> {
        let base = match &choice {
            StartChoice::Preset(id) => BaseRef::Preset(*id),
            StartChoice::KeepCurrent => {
                let path = self.data_dir.join("bases").join("kept-gameinfo.gi");
                std::fs::create_dir_all(path.parent().expect("has parent"))?;
                dt_core::backup::atomic_write(&path, self.live.gameinfo.as_bytes())?;
                BaseRef::File(path)
            }
        };
        self.switch_profile(
            Profile {
                name: "My settings".into(),
                base,
                ..default_profile()
            },
            false,
        );
        self.welcome = Some(Welcome::PickStart {
            choice: Some(choice),
        });
        Ok(())
    }

    /// The welcome flow's Apply: write the starting point, save it as the profile, finish onboarding.
    pub fn welcome_apply(&mut self) -> Result<(), String> {
        let needs_restart = match &self.preview {
            Ok(plan) if !plan.is_empty() => self.apply()?.report.needs_restart,
            Ok(_) => false,
            Err(e) => return Err(e.clone()),
        };
        self.save_profile().map_err(|e| e.to_string())?;
        self.settings.onboarded = true;
        self.welcome = Some(Welcome::Done { needs_restart });
        Ok(())
    }

    pub fn finish_welcome(&mut self) {
        self.welcome = None;
    }

    pub fn run_checks(&mut self) {
        self.checks = Some(dt_core::doctor::run(Some(&self.paths), &self.data_dir));
    }

    /// Restores the newest gameinfo.gi backup older than the last one undone (and different
    /// from the live file), so repeated clicks walk further back until the next Apply.
    /// A video.txt backup from the same apply is restored with it.
    pub fn undo_last(&mut self) -> Result<(), String> {
        let live_sha = sha256_hex(self.live.gameinfo.as_bytes());
        let cursor = self.undo_cursor;
        let gameinfo = self
            .backups(FileKind::GameInfo)
            .into_iter()
            .filter(|b| cursor.is_none_or(|c| b.created < c))
            .find(|b| b.sha256 != live_sha)
            .ok_or("There is nothing to undo yet.")?;
        let video_sha = self.live.video.as_deref().map(|v| sha256_hex(v.as_bytes()));
        let video = self.backups(FileKind::Video).into_iter().find(|b| {
            (b.created - gameinfo.created).num_seconds().abs() <= 10
                && Some(&b.sha256) != video_sha.as_ref()
        });
        self.restore(&gameinfo).map_err(|e| e.to_string())?;
        if let Some(video) = video {
            self.restore(&video).map_err(|e| e.to_string())?;
        }
        self.undo_cursor = Some(gameinfo.created);
        Ok(())
    }

    /// Puts back the files exactly as they were before DeadTune first wrote them.
    pub fn restore_original_files(&mut self) -> Result<(), String> {
        let originals: Vec<BackupEntry> = [FileKind::GameInfo, FileKind::Video]
            .into_iter()
            .filter_map(|k| self.store.original(k))
            .collect();
        if originals.is_empty() {
            return Err("DeadTune has not changed any game files yet.".into());
        }
        for entry in &originals {
            self.restore(entry).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn is_dirty(&self) -> bool {
        self.saved.as_ref() != Some(&self.profile)
    }

    pub fn refresh_preview(&mut self) {
        // Ranked-safe also takes our HUD addon out: stock means stock.
        let layout = match self.settings.source {
            TargetSource::Profile => &self.profile.hud,
            TargetSource::RankedSafe => &HudLayout::default(),
        };
        let hud = match &self.hud_cache {
            Some((cached, plan)) if cached == layout => plan.clone(),
            _ => {
                let plan =
                    apply::hud_plan(&self.paths, layout, &self.store).map_err(|e| e.to_string());
                self.hud_cache = Some((layout.clone(), plan.clone()));
                plan
            }
        };
        // A HUD that cannot be built (missing game archive, foreign addon) must not block
        // convar changes; the HUD tab shows the reason instead.
        let hud = hud.unwrap_or(None);
        let target = match self.settings.source {
            TargetSource::Profile => match &self.base {
                Ok(base) => apply::target(
                    &self.live.gameinfo,
                    self.live.video.as_deref(),
                    &base.texts,
                    &self.profile,
                    self.catalog,
                    hud,
                ),
                Err(e) => {
                    self.preview = Err(e.clone());
                    self.preview_diff.clear();
                    return;
                }
            },
            TargetSource::RankedSafe => {
                apply::ranked_safe_target(&self.live.gameinfo, &self.store, hud)
            }
        };
        self.preview = target
            .and_then(|t| {
                apply::plan(
                    &self.paths,
                    &self.live.gameinfo,
                    self.live.video.as_deref(),
                    &t,
                    self.catalog,
                    self.ctx,
                )
            })
            .map_err(|e| e.to_string());
        self.preview_diff = match &self.preview {
            Ok(plan) => [&plan.gameinfo, &plan.video]
                .into_iter()
                .flatten()
                .map(FileWrite::unified_diff)
                .collect::<Vec<_>>()
                .join("\n"),
            Err(_) => String::new(),
        };
    }

    pub fn setting(&self, name: &str) -> Setting {
        if let Some(v) = self.profile.convars.set.get(name) {
            return Setting::Override(v.clone());
        }
        if self.profile.convars.comment.iter().any(|c| c == name) {
            return Setting::CommentedOut;
        }
        match self.base.as_ref().ok().and_then(|b| b.values.get(name)) {
            Some(v) => Setting::Base(v.clone()),
            None => Setting::EngineDefault,
        }
    }

    /// The value the control shows: the configured one, else the catalog default.
    pub fn current_value(&self, name: &str) -> Option<String> {
        match self.setting(name) {
            Setting::Override(v) | Setting::Base(v) => Some(v),
            Setting::CommentedOut | Setting::EngineDefault => {
                self.catalog.get(name).and_then(|e| e.default.clone())
            }
        }
    }

    fn base_value(&self, name: &str) -> Option<&str> {
        self.base
            .as_ref()
            .ok()?
            .values
            .get(name)
            .map(String::as_str)
    }

    /// Whether a change to `name` can reach the running game through the console now.
    pub fn is_live_now(&self, name: &str) -> bool {
        match self.catalog.apply_class(name) {
            ApplyClass::Live => true,
            ApplyClass::LiveCheat => self.ctx.in_sandbox,
            ApplyClass::Restart => false,
        }
    }

    pub fn set_convar(&mut self, name: &str, value: String) -> Result<(), EditError> {
        if self.catalog.is_denied(name) {
            return Err(EditError::Denied(name.to_string()));
        }
        let is_base = self.base_value(name) == Some(value.as_str());
        let edits = &mut self.profile.convars;
        edits.comment.retain(|c| c != name);
        if is_base {
            edits.set.remove(name);
        } else {
            edits.set.insert(name.to_string(), value.clone());
        }
        if self.ctx.game_running
            && self.settings.bridge.pushes_while_dragging()
            && self.is_live_now(name)
        {
            self.live_push.schedule(name, &value, Instant::now());
        }
        self.refresh_preview();
        Ok(())
    }

    pub fn comment_convar(&mut self, name: &str) -> Result<(), EditError> {
        if self.catalog.is_denied(name) {
            return Err(EditError::Denied(name.to_string()));
        }
        let in_base = self.base_value(name).is_some();
        let edits = &mut self.profile.convars;
        edits.set.remove(name);
        if in_base && !edits.comment.iter().any(|c| c == name) {
            edits.comment.push(name.to_string());
        }
        self.refresh_preview();
        Ok(())
    }

    pub fn revert_convar(&mut self, name: &str) {
        self.profile.convars.set.remove(name);
        self.profile.convars.comment.retain(|c| c != name);
        self.refresh_preview();
    }

    pub fn revert_all(&mut self) {
        self.profile = self.saved.clone().unwrap_or_else(default_profile);
        self.base = Base::resolve(&self.profile);
        self.refresh_preview();
    }

    /// Drops every per-setting edit so the profile is the preset again. Video and HUD edits stay.
    pub fn reset_to_preset(&mut self) {
        self.profile.convars = ConVarEdits::default();
        self.refresh_preview();
    }

    /// Settings that differ from the preset: overridden or commented out.
    pub fn changed_from_preset(&self) -> usize {
        self.profile.convars.set.len() + self.profile.convars.comment.len()
    }

    pub fn is_changed(&self, name: &str) -> bool {
        matches!(
            self.setting(name),
            Setting::Override(_) | Setting::CommentedOut
        )
    }

    pub fn set_video(&mut self, key: &str, value: String) {
        self.profile.video.insert(key.to_string(), value);
        self.refresh_preview();
    }

    pub fn revert_video(&mut self, key: &str) {
        self.profile.video.remove(key);
        self.refresh_preview();
    }

    pub fn set_base(&mut self, base: BaseRef) {
        self.profile.base = base;
        self.base = Base::resolve(&self.profile);
        self.refresh_preview();
    }

    /// One click to the stock ConVars block and one click back, written immediately.
    /// On failure the previous source is kept so the toggle never disagrees with the files.
    pub fn toggle_ranked_safe(&mut self) -> Result<Applied, String> {
        let previous = self.settings.source;
        self.settings.source = match previous {
            TargetSource::Profile => TargetSource::RankedSafe,
            TargetSource::RankedSafe => TargetSource::Profile,
        };
        self.refresh_preview();
        let result = self.apply();
        if result.is_err() {
            self.settings.source = previous;
            self.refresh_preview();
        }
        result
    }

    pub fn set_in_sandbox(&mut self, in_sandbox: bool) {
        self.ctx.in_sandbox = in_sandbox;
        self.refresh_preview();
    }

    /// Polled game state; a game started after the last restart-class apply has loaded it.
    pub fn observe_game(&mut self, running: bool, started_at: Option<SystemTime>) {
        if self
            .pending_restart
            .as_ref()
            .is_some_and(|p| started_at.is_some_and(|t| t >= p.since))
        {
            self.pending_restart = None;
        }
        if self.ctx.game_running != running {
            self.ctx.game_running = running;
            self.refresh_preview();
        }
    }

    /// Re-reads the live files after the watcher reports changes.
    pub fn on_changes(&mut self, changes: &[Change]) {
        let Ok(new) = LiveFiles::read(&self.paths) else {
            self.status = Some(Status::Error("gameinfo.gi disappeared".into()));
            return;
        };
        let build = changes.iter().find_map(|c| match c {
            Change::GameUpdated { from, to } => Some((from.clone(), to.clone())),
            _ => None,
        });
        let new_sha = sha256_hex(new.gameinfo.as_bytes());
        let overwritten = new_sha != self.known_gameinfo_sha;
        if overwritten {
            let diff = FileWrite {
                path: self.paths.gameinfo.clone(),
                before: self.live.gameinfo.clone(),
                after: new.gameinfo.clone(),
            }
            .unified_diff();
            self.banner = Some(Banner { build, diff });
        } else if let Some((_, to)) = build {
            self.status = Some(Status::Info(format!(
                "Game updated to build {}; gameinfo.gi was not touched.",
                to.as_deref().unwrap_or("?")
            )));
        }
        self.known_gameinfo_sha = new_sha;
        self.live = new;
        self.hud_cache = None;
        self.refresh_preview();
    }

    /// Why the HUD layout cannot be applied right now, if it cannot.
    pub fn hud_error(&self) -> Option<&str> {
        match &self.hud_cache {
            Some((_, Err(e))) => Some(e),
            _ => None,
        }
    }

    pub fn set_hud_element(&mut self, id: ElementId, edit: ElementEdit) {
        if edit == ElementEdit::default() {
            self.profile.hud.elements.remove(&id);
        } else {
            self.profile.hud.elements.insert(id, edit);
        }
        self.refresh_preview();
    }

    pub fn reset_hud(&mut self) {
        self.profile.hud = HudLayout::default();
        self.refresh_preview();
    }

    pub fn bridge_target(&self) -> BridgeTarget<'_> {
        BridgeTarget {
            kind: self.settings.bridge,
            cfg_dir: &self.paths.cfg_dir,
            netcon_port: self.settings.netcon_port,
        }
    }

    /// Writes the previewed plan; live commands go through the active bridge when the game runs.
    pub fn apply(&mut self) -> Result<Applied, String> {
        let plan = self.preview.clone()?;
        let mut applied = Applied::default();
        let mut bridge = None;
        if self.ctx.game_running && !plan.live.is_empty() {
            match self.bridge_target().open() {
                Ok(Some(b)) => bridge = Some(b),
                Ok(None) => {
                    applied.copy = Some(
                        dt_core::bridge::clipboard::batch_string(&plan.live)
                            .map_err(|e| e.to_string())?,
                    )
                }
                Err(e) => applied.warning = Some(format!("live push skipped: {e}")),
            }
        }
        let bridge_ref: Option<&mut dyn dt_core::bridge::Bridge> = match bridge.as_mut() {
            Some(b) => Some(b.as_mut()),
            None => None,
        };
        applied.report = match apply::execute(&self.paths, &plan, &self.store, bridge_ref) {
            Ok(report) => report,
            Err(ApplyError::Stale(_)) => {
                self.live = LiveFiles::read(&self.paths).map_err(|e| e.to_string())?;
                self.known_gameinfo_sha = sha256_hex(self.live.gameinfo.as_bytes());
                self.refresh_preview();
                return Err(STALE.into());
            }
            Err(e) => return Err(e.to_string()),
        };
        if let Some(e) = &applied.report.bridge_error {
            applied.warning = Some(format!("files saved, but the live push failed: {e}"));
        }
        if applied.report.needs_restart {
            let mut names: Vec<String> = plan
                .restart
                .iter()
                .chain(&plan.queued_cheat)
                .chain(plan.video_changes.keys())
                .cloned()
                .collect();
            if let Some(prev) = self.pending_restart.take() {
                names.extend(prev.names);
            }
            names.sort();
            names.dedup();
            self.pending_restart = Some(PendingRestart {
                names,
                since: SystemTime::now(),
            });
        }
        self.live = LiveFiles::read(&self.paths).map_err(|e| e.to_string())?;
        self.known_gameinfo_sha = sha256_hex(self.live.gameinfo.as_bytes());
        self.banner = None;
        self.undo_cursor = None;
        self.hud_cache = None;
        self.refresh_preview();
        Ok(applied)
    }

    /// Sends what is due from slider drags.
    pub fn tick_live(&mut self, now: Instant) -> Option<Result<PushOutcome, String>> {
        let cmds = self.live_push.take_due(now)?;
        Some(self.bridge_target().push(&cmds).map_err(|e| e.to_string()))
    }

    /// The Push button: every live-class difference between the files and the target.
    pub fn push_now(&mut self) -> Result<PushOutcome, String> {
        let mut cmds: BTreeMap<String, String> = self
            .preview
            .as_ref()
            .map(|p| {
                p.live
                    .iter()
                    .map(|c| (c.name.clone(), c.value.clone()))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(due) = self
            .live_push
            .take_due(Instant::now() + crate::live::DEBOUNCE)
        {
            cmds.extend(due.into_iter().map(|c| (c.name, c.value)));
        }
        if cmds.is_empty() {
            return Err("nothing live to push".into());
        }
        let cmds: Vec<ConsoleCmd> = cmds
            .into_iter()
            .map(|(name, value)| ConsoleCmd { name, value })
            .collect();
        self.bridge_target().push(&cmds).map_err(|e| e.to_string())
    }

    pub fn profiles_dir(&self) -> PathBuf {
        profiles::dir(&self.data_dir)
    }

    pub fn save_profile(&mut self) -> io::Result<PathBuf> {
        let path = profiles::save(&self.profiles_dir(), &self.profile)?;
        self.saved = Some(self.profile.clone());
        self.settings.last_profile = Some(self.profile.name.clone());
        Ok(path)
    }

    pub fn switch_profile(&mut self, profile: Profile, on_disk: bool) {
        self.saved = on_disk.then(|| profile.clone());
        if on_disk {
            self.settings.last_profile = Some(profile.name.clone());
        }
        self.profile = profile;
        self.base = Base::resolve(&self.profile);
        self.settings.source = TargetSource::Profile;
        self.reload_bench();
        self.refresh_preview();
    }

    /// Merges an overrides.gi into the profile. Returns `(applied, denied)` names.
    pub fn import_overrides(&mut self, text: &str) -> Result<(usize, Vec<String>), String> {
        let overrides = profile::parse_overrides_gi(text).map_err(|e| e.to_string())?;
        let mut denied = Vec::new();
        let mut applied = 0;
        for (name, o) in overrides {
            if self.catalog.is_denied(&name) {
                denied.push(name);
                continue;
            }
            applied += 1;
            match o {
                Override::Set(v) => {
                    self.profile.convars.comment.retain(|c| *c != name);
                    self.profile.convars.set.insert(name, v);
                }
                Override::Comment => {
                    self.profile.convars.set.remove(&name);
                    if !self.profile.convars.comment.contains(&name) {
                        self.profile.convars.comment.push(name);
                    }
                }
            }
        }
        self.refresh_preview();
        Ok((applied, denied))
    }

    pub fn export_overrides(&self) -> String {
        profile::write_overrides_gi(&self.profile.overrides())
    }

    pub fn backups(&self, kind: FileKind) -> Vec<BackupEntry> {
        self.store.list(kind).unwrap_or_default()
    }

    pub fn restore(&mut self, entry: &BackupEntry) -> io::Result<()> {
        let live = match entry.kind {
            FileKind::GameInfo => &self.paths.gameinfo,
            FileKind::Video => &self.paths.video,
        };
        self.store.restore(entry, live)?;
        self.live = LiveFiles::read(&self.paths)?;
        self.hud_cache = None;
        self.known_gameinfo_sha = sha256_hex(self.live.gameinfo.as_bytes());
        self.refresh_preview();
        Ok(())
    }

    pub fn toggle_favourite(&mut self, name: &str) {
        if !self.settings.favourites.remove(name) {
            self.settings.favourites.insert(name.to_string());
        }
    }

    /// Rows for the center list (full mode) or the compact list.
    pub fn visible_rows(&self) -> Vec<String> {
        let scope = match self.ui.mode {
            Mode::Compact => &Scope::Favourites,
            Mode::Full => &self.ui.scope,
        };
        let query = self.ui.search.to_lowercase();
        let matches = |name: &str| query.is_empty() || name.to_lowercase().contains(&query);
        match scope {
            Scope::All => self
                .catalog
                .search(&self.ui.search)
                .map(|(n, _)| n.to_string())
                .collect(),
            Scope::Category(cat) => self
                .catalog
                .search(&self.ui.search)
                .filter(|(_, e)| &e.category == cat)
                .map(|(n, _)| n.to_string())
                .collect(),
            Scope::Favourites => self
                .settings
                .favourites
                .iter()
                .filter(|n| matches(n))
                .cloned()
                .collect(),
            Scope::Changed => {
                let mut names: Vec<String> = self
                    .profile
                    .convars
                    .set
                    .keys()
                    .chain(&self.profile.convars.comment)
                    .filter(|n| matches(n))
                    .cloned()
                    .collect();
                names.sort();
                names.dedup();
                names
            }
        }
    }

    pub fn reload_bench(&mut self) {
        let dir = bench::history_dir(&self.data_dir);
        self.bench.reload_history(&dir, &self.profile.name);
        let dirs = self
            .paths
            .steam_root
            .as_deref()
            .map(dt_core::locate::screenshot_dirs)
            .unwrap_or_default();
        self.bench.screenshots = bench::recent_screenshots(&dirs, 24);
    }

    pub fn start_watch(&mut self) -> Result<(), String> {
        let (tx, rx) = channel();
        let watcher = watch::spawn(self.paths.clone(), watch::fingerprint(&self.paths), tx)
            .map_err(|e| e.to_string())?;
        self.watch = Some((watcher, rx));
        Ok(())
    }

    pub fn poll_watch(&mut self) {
        let Some((_, rx)) = &self.watch else { return };
        let changes: Vec<Change> = rx.try_iter().flatten().collect();
        if !changes.is_empty() {
            self.on_changes(&changes);
        }
    }
}

#[cfg(test)]
pub mod testutil {
    use super::*;

    /// Same layout as `scripts/fake-install.sh`.
    pub fn fake_install() -> (tempfile::TempDir, GamePaths) {
        let dir = tempfile::tempdir().unwrap();
        let research = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../research/configs/OptimizationLock"
        );
        let root = dir.path().join("steamapps/common/Deadlock");
        let citadel = root.join("game/citadel");
        std::fs::create_dir_all(citadel.join("cfg")).unwrap();
        std::fs::copy(
            format!("{research}/clean gameinfo.gi/gameinfo.gi"),
            citadel.join("gameinfo.gi"),
        )
        .unwrap();
        std::fs::copy(
            format!("{research}/test_cfg/video.txt"),
            citadel.join("cfg/video.txt"),
        )
        .unwrap();
        let paths = dt_core::locate::from_game_root(&root).unwrap();
        (dir, paths)
    }

    /// An onboarded user, so tests start on the main screen.
    pub fn state() -> (tempfile::TempDir, AppState) {
        let (dir, paths) = fake_install();
        let data = dir.path().join("data");
        let settings = Settings {
            onboarded: true,
            ..Settings::default()
        };
        let state = AppState::open(paths, data, settings).unwrap();
        (dir, state)
    }

    pub fn first_run() -> (tempfile::TempDir, AppState) {
        let (dir, paths) = fake_install();
        let data = dir.path().join("data");
        let state = AppState::open(paths, data, Settings::default()).unwrap();
        (dir, state)
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::state;
    use super::*;

    const LIVE: &str = "fps_max";
    const CHEAT: &str = "r_farz";
    const RESTART: &str = "ai_foot_sweep_enable";
    const DENIED: &str = "citadel_player_outline_enemies";

    fn plan(state: &AppState) -> &ApplyPlan {
        state.preview.as_ref().expect("plan preview")
    }

    #[test]
    fn fresh_state_is_clean_with_an_empty_plan() {
        let (_dir, state) = state();
        assert!(!state.is_dirty());
        assert!(state.base.is_ok(), "{:?}", state.base.as_ref().err());
        assert!(
            plan(&state).is_empty(),
            "vanilla base over clean gameinfo: {:?}",
            plan(&state)
        );
    }

    #[test]
    fn editing_a_convar_marks_dirty_and_updates_the_plan() {
        let (_dir, mut state) = state();
        state.set_convar(RESTART, "true".into()).unwrap();
        assert!(state.is_dirty());
        assert_eq!(state.setting(RESTART), Setting::Override("true".into()));
        let plan = plan(&state);
        assert!(plan.restart.contains(&RESTART.to_string()), "{plan:?}");
        let write = plan.gameinfo.as_ref().expect("gameinfo write");
        assert!(write.unified_diff().contains(RESTART));
    }

    #[test]
    fn live_class_edit_is_planned_as_a_console_command() {
        let (_dir, mut state) = state();
        state.set_convar(LIVE, "144".into()).unwrap();
        let plan = plan(&state);
        assert!(
            plan.live.iter().any(|c| c.name == LIVE && c.value == "144"),
            "{plan:?}"
        );
    }

    #[test]
    fn cheat_convar_is_queued_unless_in_sandbox() {
        let (_dir, mut state) = state();
        state.set_convar(CHEAT, "6000".into()).unwrap();
        assert!(plan(&state).queued_cheat.contains(&CHEAT.to_string()));
        state.set_in_sandbox(true);
        assert!(plan(&state).live.iter().any(|c| c.name == CHEAT));
    }

    #[test]
    fn setting_the_base_value_drops_the_override() {
        let (_dir, mut state) = state();
        let base = state
            .base
            .as_ref()
            .unwrap()
            .values
            .iter()
            .next()
            .map(|(k, v)| (k.clone(), v.clone()));
        let (name, value) = base.expect("vanilla base has convars");
        if state.catalog.is_denied(&name) {
            return;
        }
        state.set_convar(&name, "12345".into()).unwrap();
        state.set_convar(&name, value.clone()).unwrap();
        assert_eq!(state.setting(&name), Setting::Base(value));
        assert!(!state.is_dirty());
    }

    #[test]
    fn revert_one_and_revert_all() {
        let (_dir, mut state) = state();
        state.set_convar(RESTART, "true".into()).unwrap();
        state.set_convar(LIVE, "90".into()).unwrap();
        state.revert_convar(RESTART);
        assert_ne!(state.setting(RESTART), Setting::Override("true".into()));
        assert!(state.is_dirty(), "fps_max still edited");
        state.revert_all();
        assert!(!state.is_dirty());
        assert!(plan(&state).is_empty());
    }

    #[test]
    fn reset_to_preset_clears_every_edit_but_keeps_the_preset() {
        let (_dir, mut state) = state();
        state.set_base(BaseRef::Preset(PresetId::Sqooky));
        state.set_convar(LIVE, "144".into()).unwrap();
        state.set_convar(RESTART, "true".into()).unwrap();
        state.comment_convar(CHEAT).unwrap();
        assert_eq!(state.changed_from_preset(), 3);
        assert!(state.is_changed(LIVE) && state.is_changed(CHEAT));
        assert!(
            !state.is_changed("r_shadows"),
            "preset value is not a change"
        );
        state.revert_convar(CHEAT);
        assert_eq!(state.changed_from_preset(), 2);
        state.reset_to_preset();
        assert_eq!(state.changed_from_preset(), 0);
        assert!(!state.is_changed(LIVE));
        assert_eq!(state.profile.base, BaseRef::Preset(PresetId::Sqooky));
        assert_eq!(state.setting(LIVE), Setting::Base("400".into()));
    }

    #[test]
    fn denylisted_convars_are_read_only() {
        let (_dir, mut state) = state();
        assert_eq!(
            state.set_convar(DENIED, "1".into()),
            Err(EditError::Denied(DENIED.into()))
        );
        assert_eq!(
            state.comment_convar(DENIED),
            Err(EditError::Denied(DENIED.into()))
        );
        assert!(!state.is_dirty());
        let (applied, denied) = state
            .import_overrides(&format!("{DENIED} \"1\"\nfps_max \"90\"\n"))
            .unwrap();
        assert_eq!((applied, denied), (1, vec![DENIED.to_string()]));
    }

    #[test]
    fn switching_preset_keeps_overrides_and_changes_the_plan() {
        let (_dir, mut state) = state();
        state.set_convar(LIVE, "90".into()).unwrap();
        let before = plan(&state).gameinfo.clone();
        state.set_base(BaseRef::Preset(PresetId::KaizMinspec));
        assert_eq!(state.profile.base, BaseRef::Preset(PresetId::KaizMinspec));
        assert_eq!(state.setting(LIVE), Setting::Override("90".into()));
        let after = plan(&state).gameinfo.clone();
        assert_ne!(before, after);
        assert!(
            plan(&state).restart.len() > 10,
            "kaiz touches many devonly convars"
        );
    }

    #[test]
    fn ranked_safe_toggles_between_stock_and_profile() {
        let (_dir, mut state) = state();
        state.set_base(BaseRef::Preset(PresetId::KaizMinspec));
        state.apply().unwrap();
        assert!(plan(&state).is_empty(), "profile applied");
        let video_before = state.live.video.clone();
        let applied = state.toggle_ranked_safe().unwrap();
        assert!(
            applied.report.wrote_gameinfo,
            "stock block differs from kaiz"
        );
        assert_eq!(state.settings.source, TargetSource::RankedSafe);
        let stock = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../research/configs/OptimizationLock/clean gameinfo.gi/gameinfo.gi"
        ))
        .unwrap();
        assert_eq!(state.live.gameinfo, stock, "original snapshot restored");
        assert_eq!(
            state.live.video, video_before,
            "ranked-safe keeps video.txt"
        );
        state.toggle_ranked_safe().unwrap();
        assert_eq!(state.settings.source, TargetSource::Profile);
        assert!(plan(&state).is_empty(), "profile written back");
        assert_ne!(state.live.gameinfo, stock);
    }

    #[test]
    fn apply_writes_gameinfo_backs_up_and_marks_pending_restart() {
        let (_dir, mut state) = state();
        state.set_convar(RESTART, "true".into()).unwrap();
        let applied = state.apply().unwrap();
        assert!(applied.report.wrote_gameinfo);
        let on_disk = std::fs::read_to_string(&state.paths.gameinfo).unwrap();
        let values = gi::effective_values(&on_disk).unwrap();
        assert_eq!(values.get(RESTART).map(String::as_str), Some("true"));
        assert!(state.store.original(FileKind::GameInfo).is_some());
        let pending = state.pending_restart.clone().expect("restart pending");
        assert!(pending.names.contains(&RESTART.to_string()));
        assert!(plan(&state).is_empty(), "nothing left to apply");

        state.observe_game(
            true,
            Some(pending.since - std::time::Duration::from_secs(60)),
        );
        assert!(
            state.pending_restart.is_some(),
            "game started before the apply"
        );
        state.observe_game(
            true,
            Some(pending.since + std::time::Duration::from_secs(1)),
        );
        assert!(state.pending_restart.is_none());
    }

    #[test]
    fn external_overwrite_raises_the_banner_but_our_own_write_does_not() {
        let (_dir, mut state) = state();
        state.set_convar(RESTART, "true".into()).unwrap();
        state.apply().unwrap();
        state.on_changes(&[Change::GameInfoChanged]);
        assert_eq!(state.banner, None, "our own write");

        let stock = state.store.original(FileKind::GameInfo).unwrap();
        std::fs::copy(&stock.path, &state.paths.gameinfo).unwrap();
        state.on_changes(&[
            Change::GameUpdated {
                from: Some("1".into()),
                to: Some("2".into()),
            },
            Change::GameInfoChanged,
        ]);
        let banner = state.banner.clone().expect("banner");
        assert_eq!(banner.build, Some((Some("1".into()), Some("2".into()))));
        assert!(banner.diff.contains(RESTART), "{}", banner.diff);
        assert!(plan(&state).gameinfo.is_some(), "re-apply is ready");
        state.apply().unwrap();
        assert_eq!(state.banner, None);
    }

    #[test]
    fn build_change_without_overwrite_is_only_a_status() {
        let (_dir, mut state) = state();
        state.on_changes(&[Change::GameUpdated {
            from: None,
            to: Some("9".into()),
        }]);
        assert_eq!(state.banner, None);
        assert!(matches!(state.status, Some(Status::Info(_))));
    }

    #[test]
    fn live_edit_while_running_schedules_a_debounced_push() {
        let (_dir, mut state) = state();
        state.set_convar(LIVE, "100".into()).unwrap();
        assert!(!state.live_push.is_pending(), "game not running");
        state.observe_game(true, None);
        state.set_convar(RESTART, "true".into()).unwrap();
        assert!(!state.live_push.is_pending(), "restart class never pushes");
        state.set_convar(LIVE, "120".into()).unwrap();
        assert!(state.live_push.is_pending());
        let outcome = state
            .tick_live(Instant::now() + crate::live::DEBOUNCE)
            .unwrap()
            .unwrap();
        assert!(matches!(outcome, PushOutcome::Sent { count: 1, .. }));
        let cfg = std::fs::read_to_string(state.paths.cfg_dir.join("deadtune_live.cfg")).unwrap();
        assert!(cfg.contains(r#"fps_max "120""#));
    }

    #[test]
    fn rows_follow_search_scope_and_mode() {
        let (_dir, mut state) = state();
        state.ui.search = "farz".into();
        assert!(state.visible_rows().contains(&CHEAT.to_string()));
        state.ui.scope = Scope::Category("Audio".into());
        assert!(state.visible_rows().is_empty());
        state.ui.search.clear();
        state.ui.scope = Scope::Changed;
        state.set_convar(LIVE, "77".into()).unwrap();
        assert_eq!(state.visible_rows(), vec![LIVE.to_string()]);
        state.toggle_favourite(CHEAT);
        state.ui.mode = Mode::Compact;
        assert_eq!(
            state.visible_rows(),
            vec![CHEAT.to_string()],
            "compact shows favourites"
        );
        state.toggle_favourite(CHEAT);
        assert!(state.visible_rows().is_empty());
    }

    #[test]
    fn profiles_save_and_switch() {
        let (_dir, mut state) = state();
        state.set_convar(LIVE, "60".into()).unwrap();
        state.profile.name = "Laptop".into();
        state.save_profile().unwrap();
        assert!(!state.is_dirty());
        assert_eq!(state.settings.last_profile.as_deref(), Some("Laptop"));
        let battery = profile::builtin_suggestions().pop().unwrap();
        state.switch_profile(battery, false);
        assert!(state.is_dirty(), "suggestion not saved yet");
        let laptop = profiles::load(&state.profiles_dir(), "Laptop").unwrap();
        state.switch_profile(laptop, true);
        assert_eq!(state.setting(LIVE), Setting::Override("60".into()));
        assert!(!state.is_dirty());
    }

    #[test]
    fn overrides_round_trip() {
        let (_dir, mut state) = state();
        state.set_convar(LIVE, "60".into()).unwrap();
        state.comment_convar(CHEAT).ok();
        let text = state.export_overrides();
        let (_d2, mut other) = super::testutil::state();
        other.import_overrides(&text).unwrap();
        assert_eq!(other.profile.overrides(), state.profile.overrides());
    }

    #[test]
    fn first_run_picks_a_preset_applies_and_finishes() {
        let (_dir, mut state) = super::testutil::first_run();
        assert_eq!(state.welcome, Some(Welcome::PickStart { choice: None }));
        state
            .choose_start(StartChoice::Preset(PresetId::KaizMinspec))
            .unwrap();
        assert_eq!(state.profile.base, BaseRef::Preset(PresetId::KaizMinspec));
        assert!(!plan(&state).is_empty());
        state.welcome_apply().unwrap();
        assert_eq!(
            state.welcome,
            Some(Welcome::Done {
                needs_restart: true
            })
        );
        assert!(state.settings.onboarded);
        assert!(!state.is_dirty(), "starting point saved as the profile");
        assert_eq!(state.settings.last_profile.as_deref(), Some("My settings"));
        let on_disk = std::fs::read_to_string(&state.paths.gameinfo).unwrap();
        assert_eq!(on_disk, state.live.gameinfo);
        assert!(plan(&state).is_empty(), "files match the preset");
        state.finish_welcome();
        assert_eq!(state.welcome, None);
    }

    #[test]
    fn first_run_keep_current_writes_nothing() {
        let (_dir, mut state) = super::testutil::first_run();
        let before = std::fs::read(&state.paths.gameinfo).unwrap();
        state.choose_start(StartChoice::KeepCurrent).unwrap();
        assert!(plan(&state).is_empty(), "{:?}", plan(&state));
        state.welcome_apply().unwrap();
        assert_eq!(
            state.welcome,
            Some(Welcome::Done {
                needs_restart: false
            })
        );
        assert_eq!(std::fs::read(&state.paths.gameinfo).unwrap(), before);
        assert!(state.settings.onboarded);
    }

    #[test]
    fn onboarded_users_skip_the_welcome() {
        let (_dir, state) = state();
        assert_eq!(state.welcome, None);
    }

    #[test]
    fn undo_walks_back_through_backups_and_restore_original_resets() {
        let (_dir, mut state) = state();
        let original = state.live.gameinfo.clone();
        state.set_convar(RESTART, "true".into()).unwrap();
        state.apply().unwrap();
        let first = state.live.gameinfo.clone();
        state.set_convar(LIVE, "90".into()).unwrap();
        state.apply().unwrap();
        state.undo_last().unwrap();
        assert_eq!(state.live.gameinfo, first, "one step back");
        state.undo_last().unwrap();
        assert_eq!(state.live.gameinfo, original, "two steps back");
        assert!(state.undo_last().is_err(), "nothing older");
        state.apply().unwrap();
        state.restore_original_files().unwrap();
        assert_eq!(state.live.gameinfo, original);
        assert_eq!(
            std::fs::read_to_string(&state.paths.gameinfo).unwrap(),
            original
        );
    }

    #[test]
    fn restore_original_before_any_write_explains() {
        let (_dir, mut state) = state();
        assert!(
            state
                .restore_original_files()
                .unwrap_err()
                .contains("not changed")
        );
    }

    #[test]
    fn hud_edit_marks_dirty_and_a_missing_archive_does_not_block_convars() {
        let (_dir, mut state) = state();
        let edit = ElementEdit {
            visibility: dt_core::hud::layout::Visibility::Hidden,
            ..ElementEdit::default()
        };
        state.set_hud_element(ElementId::Minimap, edit);
        assert!(state.is_dirty());
        assert!(
            state
                .hud_error()
                .is_some_and(|e| e.contains("pak01_dir.vpk")),
            "fake install has no game archive: {:?}",
            state.hud_error()
        );
        state.set_convar(LIVE, "90".into()).unwrap();
        assert!(
            plan(&state).gameinfo.is_some(),
            "convar changes still planned"
        );
        state.set_hud_element(ElementId::Minimap, ElementEdit::default());
        assert!(
            state.profile.hud.elements.is_empty(),
            "identity edit removed"
        );
        assert_eq!(state.hud_error(), None);
        state.revert_convar(LIVE);
        assert!(!state.is_dirty());
    }

    #[test]
    fn bool_text_follows_existing_style() {
        assert_eq!(bool_text(true, Some("0")), "1");
        assert_eq!(bool_text(false, Some("true")), "false");
        assert_eq!(bool_text(true, None), "true");
        assert!(parse_bool("TRUE") && parse_bool("1") && !parse_bool("0"));
    }

    #[test]
    fn numbers_format_like_config_values() {
        assert_eq!(fmt_num(6000.4, true), "6000");
        assert_eq!(fmt_num(0.55, false), "0.55");
        assert_eq!(fmt_num(2.0, false), "2");
        assert_eq!(fmt_num(-0.00001, false), "0");
    }
}
