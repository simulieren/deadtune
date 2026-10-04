//! Everything the window shows, and every transition on it, without a window.

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Instant, SystemTime};

use dt_core::addons::install::{Action, AddonsPlan, InstalledState};
use dt_core::addons::textures::{BuildStats, Progress, TextureDownscale};
use dt_core::addons::{self, AddonId, AddonsConfig};
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

/// Simple-view navigation: one rail entry each, Overview first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Section {
    #[default]
    Overview,
    Display,
    Shadows,
    Effects,
    World,
    Performance,
    Hud,
    Addons,
    Safety,
}

impl Section {
    pub const ALL: [Section; 9] = [
        Section::Overview,
        Section::Display,
        Section::Shadows,
        Section::Effects,
        Section::World,
        Section::Performance,
        Section::Hud,
        Section::Addons,
        Section::Safety,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Section::Overview => "Overview",
            Section::Display => "Display",
            Section::Shadows => "Shadows",
            Section::Effects => "Lighting & effects",
            Section::World => "World detail",
            Section::Performance => "Performance",
            Section::Hud => "HUD",
            Section::Addons => "Addons",
            Section::Safety => "Safety & setup",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            Section::Overview => "Pick a preset, nudge the big ones, press Apply.",
            Section::Display => "Resolution, upscaling and texture detail.",
            Section::Shadows => "Shadows are the most expensive thing the game draws.",
            Section::Effects => "Ambient occlusion, glow, fog and ability effects.",
            Section::World => "How far and how detailed the world is drawn.",
            Section::Performance => "Frame rate caps, menus and CPU.",
            Section::Hud => "Move and resize parts of the in-game HUD.",
            Section::Addons => {
                "Community performance mods, rebuilt by DeadTune so they survive game updates."
            }
            Section::Safety => "Undo, restore, ranked-safe mode and instant changes.",
        }
    }

    /// Pages made of setting rows (not Overview, HUD or Safety).
    pub fn has_rows(self) -> bool {
        matches!(
            self,
            Section::Display
                | Section::Shadows
                | Section::Effects
                | Section::World
                | Section::Performance
        )
    }
}

/// What Apply would write, as the action bar headlines it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pending {
    Nothing,
    /// The preset changed since the last Apply, so the whole profile lands with its tweaks.
    Preset {
        label: String,
        tweaks: usize,
    },
    /// Settings edited since the last Apply on the same preset.
    Tweaks(usize),
    /// HUD, addon or video edits only.
    Other,
}

/// When pending changes reach the game, for the action bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timing {
    Nothing,
    NextLaunch,
    /// Live changes the running game picks up from the console (key bind or netcon).
    Instant,
    Mixed {
        now: usize,
        later: usize,
    },
}

impl Timing {
    pub fn of(plan: &ApplyPlan, game_running: bool) -> Timing {
        let s = PlanSummary::of(plan, game_running);
        let (now, later) = (s.live_now, s.queued + s.next_launch);
        match (now, later) {
            (0, 0) if plan.is_empty() => Timing::Nothing,
            (0, _) => Timing::NextLaunch,
            (_, 0) => Timing::Instant,
            (now, later) => Timing::Mixed { now, later },
        }
    }
}

/// Pending changes by when they reach the game, for the advanced view's pending panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PlanSummary {
    /// Live-class changes the running game picks up from the console.
    pub live_now: usize,
    /// Cheat-flagged changes held back until the player is in hideout or sandbox.
    pub queued: usize,
    /// Written to the files and read when Deadlock starts.
    pub next_launch: usize,
    /// Denylisted edits that are never written.
    pub refused: usize,
}

impl PlanSummary {
    pub fn of(plan: &ApplyPlan, game_running: bool) -> PlanSummary {
        let live_now = if game_running { plan.live.len() } else { 0 };
        PlanSummary {
            live_now,
            queued: plan.queued_cheat.len(),
            next_launch: plan.restart.len()
                + plan.video_changes.len()
                + plan.addon_changes()
                + (plan.live.len() - live_now),
            refused: plan.denied.len(),
        }
    }
}

/// Rows in one advanced-view list scope, and how many the profile changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ScopeCount {
    pub rows: usize,
    pub changed: usize,
}

/// What the advanced view's category rail shows next to each entry, under the current search.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct RailCounts {
    pub all: ScopeCount,
    pub changed: usize,
    pub favourites: usize,
    /// Every catalog category, including those the search empties.
    pub categories: BTreeMap<String, ScopeCount>,
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
    pub section: Section,
    /// The simple-view row whose help the side panel explains: hovered or last edited.
    pub focus: Option<&'static str>,
    /// Simple-view search; non-empty shows the Results page instead of the section.
    pub query: String,
    /// Narrows the mini window's list.
    pub mini_filter: String,
    /// The addon card whose options are unfolded.
    pub addon_expanded: Option<AddonId>,
    /// Path typed into the addons page's Import box.
    pub addon_import_path: String,
}

/// Starting layouts on the HUD tab. Each is plain `HudLayout` values, so a user
/// can pick one and keep editing from there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HudPreset {
    Vanilla,
    Clean,
    Competitive,
}

impl HudPreset {
    pub const ALL: [HudPreset; 3] = [HudPreset::Vanilla, HudPreset::Clean, HudPreset::Competitive];

    pub fn label(self) -> &'static str {
        match self {
            HudPreset::Vanilla => "Vanilla",
            HudPreset::Clean => "Clean",
            HudPreset::Competitive => "Competitive",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            HudPreset::Vanilla => "The game's own layout.",
            HudPreset::Clean => "Hides the kill feed, chat and the souls panel.",
            HudPreset::Competitive => "Bigger minimap, smaller kill feed, quieter chat.",
        }
    }

    pub fn layout(self) -> HudLayout {
        use dt_core::hud::layout::Visibility;
        let hidden = ElementEdit {
            visibility: Visibility::Hidden,
            ..ElementEdit::default()
        };
        let scaled = |scale_pct| ElementEdit {
            scale_pct,
            ..ElementEdit::default()
        };
        let elements: Vec<(ElementId, ElementEdit)> = match self {
            HudPreset::Vanilla => vec![],
            HudPreset::Clean => vec![
                (ElementId::KillFeed, hidden.clone()),
                (ElementId::Chat, hidden.clone()),
                (ElementId::PlayerStats, hidden),
            ],
            HudPreset::Competitive => vec![
                (ElementId::Minimap, scaled(125)),
                (ElementId::KillFeed, scaled(80)),
                (
                    ElementId::Chat,
                    ElementEdit {
                        opacity_pct: 70,
                        ..ElementEdit::default()
                    },
                ),
            ],
        };
        HudLayout {
            elements: elements.into_iter().collect(),
            extra_css: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Info(String),
    Error(String),
}

enum BuildMsg {
    Progress(Progress),
    Done(Result<BuildStats, String>),
}

pub struct TextureBuild {
    rx: Receiver<BuildMsg>,
    pub progress: Progress,
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
    /// Same for the addons plan, keyed by the config it was built from.
    addons_cache: Option<(AddonsConfig, Result<Option<AddonsPlan>, String>)>,
    /// A texture downscaler build running on its own thread.
    pub texture_build: Option<TextureBuild>,
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
        addons: AddonsConfig::default(),
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
            addons_cache: None,
            texture_build: None,
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
        let config = match self.settings.source {
            TargetSource::Profile => &self.profile.addons,
            TargetSource::RankedSafe => &AddonsConfig::default(),
        };
        let addons = match &self.addons_cache {
            Some((cached, plan)) if cached == config => plan.clone(),
            _ => {
                let plan =
                    apply::addons_plan(&self.paths, config, &self.store).map_err(|e| e.to_string());
                self.addons_cache = Some((config.clone(), plan.clone()));
                plan
            }
        };
        let addons = addons.unwrap_or(None);
        let target = match self.settings.source {
            TargetSource::Profile => match &self.base {
                Ok(base) => apply::target(
                    &self.live.gameinfo,
                    self.live.video.as_deref(),
                    &base.texts,
                    &self.profile,
                    self.catalog,
                    hud,
                    addons,
                ),
                Err(e) => {
                    self.preview = Err(e.clone());
                    self.preview_diff.clear();
                    return;
                }
            },
            TargetSource::RankedSafe => {
                apply::ranked_safe_target(&self.live.gameinfo, &self.store, hud, addons)
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
        self.reset_convars([name]);
    }

    pub fn revert_all(&mut self) {
        self.profile = self.saved.clone().unwrap_or_else(default_profile);
        self.base = Base::resolve(&self.profile);
        self.refresh_preview();
    }

    /// Whether the profile moves `name` away from the preset.
    pub fn is_changed(&self, name: &str) -> bool {
        matches!(
            self.setting(name),
            Setting::Override(_) | Setting::CommentedOut
        )
    }

    pub fn changed_count<'a>(&self, names: impl IntoIterator<Item = &'a str>) -> usize {
        names.into_iter().filter(|n| self.is_changed(n)).count()
    }

    /// What the preset gives `name`, falling back to the game default.
    pub fn preset_value(&self, name: &str) -> Option<String> {
        self.base_value(name)
            .map(str::to_string)
            .or_else(|| self.catalog.get(name).and_then(|e| e.default.clone()))
    }

    pub fn reset_convars<'a>(&mut self, names: impl IntoIterator<Item = &'a str>) {
        let edits = &mut self.profile.convars;
        for name in names {
            edits.set.remove(name);
            edits.comment.retain(|c| c != name);
        }
        self.refresh_preview();
    }

    /// Drops every convar edit so the profile is the preset again; HUD and video edits stay.
    pub fn reset_to_preset(&mut self) {
        self.profile.convars = ConVarEdits::default();
        self.refresh_preview();
    }

    pub fn timing(&self) -> Timing {
        match &self.preview {
            Ok(plan) => Timing::of(plan, self.ctx.game_running),
            Err(_) => Timing::Nothing,
        }
    }

    /// Every setting the profile moves away from the preset, whatever view edited it.
    pub fn tweak_count(&self) -> usize {
        self.profile.convars.set.len() + self.profile.convars.comment.len()
    }

    pub fn preset_label(&self) -> String {
        match &self.profile.base {
            BaseRef::Preset(id) => dt_core::preset::info(*id).label.to_string(),
            BaseRef::File(_) => "My original settings".into(),
        }
    }

    pub fn pending(&self) -> Pending {
        if !self.preview.as_ref().is_ok_and(|p| !p.is_empty()) {
            return Pending::Nothing;
        }
        let was = match &self.saved {
            Some(saved) if saved.base == self.profile.base => &saved.convars,
            _ => {
                return Pending::Preset {
                    label: self.preset_label(),
                    tweaks: self.tweak_count(),
                };
            }
        };
        let now = &self.profile.convars;
        let edited = now
            .set
            .keys()
            .chain(was.set.keys())
            .chain(&now.comment)
            .chain(&was.comment)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .filter(|n| {
                now.set.get(*n) != was.set.get(*n)
                    || now.comment.contains(n) != was.comment.contains(n)
            })
            .count();
        if edited > 0 {
            Pending::Tweaks(edited)
        } else {
            Pending::Other
        }
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
        self.addons_cache = None;
        self.refresh_preview();
    }

    /// Why the addons cannot be planned right now, if they cannot.
    pub fn addons_error(&self) -> Option<&str> {
        match &self.addons_cache {
            Some((_, Err(e))) => Some(e),
            _ => None,
        }
    }

    /// What Apply would do for an addon, from the previewed plan.
    pub fn addon_action(&self, id: AddonId) -> Option<&Action> {
        self.preview
            .as_ref()
            .ok()?
            .addons
            .as_ref()?
            .get(id)
            .map(|a| &a.action)
    }

    pub fn addon_conflicts(&self, id: AddonId) -> Vec<&addons::Conflict> {
        self.preview
            .as_ref()
            .ok()
            .and_then(|p| p.addons.as_ref())
            .map(|a| a.conflicts.iter().filter(|c| c.id == id).collect())
            .unwrap_or_default()
    }

    /// Install state per addon the record knows, read fresh (one small file plus stats).
    pub fn addon_states(&self) -> BTreeMap<AddonId, InstalledState> {
        addons::install::installed_state(&self.paths, &self.store.root).unwrap_or_default()
    }

    pub fn addons_enabled_count(&self) -> usize {
        self.profile.addons.enabled.len()
    }

    pub fn set_addon_enabled(&mut self, id: AddonId, on: bool) {
        self.profile.addons.set_enabled(id, on);
        self.refresh_preview();
    }

    /// `visible` keeps the particle group on screen while the disabler is on.
    pub fn set_particle_group(&mut self, group: &str, visible: bool) {
        if visible {
            self.profile.addons.keep_particles.insert(group.to_string());
        } else {
            self.profile.addons.keep_particles.remove(group);
        }
        self.refresh_preview();
    }

    pub fn set_blur(&mut self, hud: bool, menu: bool) {
        self.profile.addons.blur = addons::BlurOptions { hud, menu };
        self.refresh_preview();
    }

    pub fn set_textures(&mut self, cfg: TextureDownscale) {
        self.profile.addons.textures = cfg;
        self.refresh_preview();
    }

    /// Takes a downloaded upstream file (or a folder of them) into the cache.
    pub fn import_addon(&mut self, path: &std::path::Path) -> Result<Vec<AddonId>, String> {
        let cache = addons::sources::cache_dir(&self.store.root);
        let ids = addons::sources::import(&cache, path).map_err(|e| e.to_string())?;
        self.addons_cache = None;
        self.refresh_preview();
        Ok(ids)
    }

    #[cfg(feature = "fetch")]
    pub fn fetch_addon(&mut self, id: AddonId) -> Result<(), String> {
        let cache = addons::sources::cache_dir(&self.store.root);
        addons::sources::fetch(&cache, id).map_err(|e| e.to_string())?;
        self.addons_cache = None;
        self.refresh_preview();
        Ok(())
    }

    /// Starts the texture downscaler build on a thread; `poll_build` picks up the result.
    pub fn start_texture_build(&mut self) {
        if self.texture_build.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let paths = self.paths.clone();
        let config = self.profile.addons.clone();
        let state_dir = self.store.root.clone();
        std::thread::spawn(move || {
            let progress_tx = tx.clone();
            let result = addons::install::build_textures(
                &paths,
                &config,
                &state_dir,
                addons::textures::builder(),
                &mut |p| {
                    let _ = progress_tx.send(BuildMsg::Progress(p));
                },
            )
            .map_err(|e| e.to_string());
            let _ = tx.send(BuildMsg::Done(result));
        });
        self.texture_build = Some(TextureBuild {
            rx,
            progress: Progress::default(),
        });
    }

    /// Drains build messages; on completion the plan is refreshed and the status set.
    pub fn poll_build(&mut self) {
        let Some(build) = &mut self.texture_build else {
            return;
        };
        let mut done = None;
        for msg in build.rx.try_iter() {
            match msg {
                BuildMsg::Progress(p) => build.progress = p,
                BuildMsg::Done(result) => done = Some(result),
            }
        }
        let Some(result) = done else {
            return;
        };
        self.texture_build = None;
        self.status = Some(match result {
            Ok(stats) => Status::Info(format!(
                "Texture pak built: {} textures, {} MB down to {} MB. Takes effect next time you start Deadlock.",
                stats.textures,
                stats.bytes_before >> 20,
                stats.bytes_after >> 20
            )),
            Err(e) => Status::Error(format!("texture build: {e}")),
        });
        self.addons_cache = None;
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

    pub fn hud_edit(&self, id: ElementId) -> ElementEdit {
        self.profile
            .hud
            .elements
            .get(&id)
            .cloned()
            .unwrap_or_default()
    }

    /// Elements that differ from vanilla (identity edits are never stored).
    pub fn hud_changed_count(&self) -> usize {
        self.profile.hud.elements.len()
    }

    pub fn apply_hud_preset(&mut self, preset: HudPreset) {
        self.profile.hud = preset.layout();
        self.refresh_preview();
    }

    /// The toolbar preset the current layout equals, if any.
    pub fn hud_preset(&self) -> Option<HudPreset> {
        HudPreset::ALL
            .into_iter()
            .find(|p| p.layout() == self.profile.hud)
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
        self.addons_cache = None;
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
        self.addons_cache = None;
        self.known_gameinfo_sha = sha256_hex(self.live.gameinfo.as_bytes());
        self.refresh_preview();
        Ok(())
    }

    pub fn toggle_favourite(&mut self, name: &str) {
        if !self.settings.favourites.remove(name) {
            self.settings.favourites.insert(name.to_string());
        }
    }

    pub fn toggle_pin(&mut self, name: &str) {
        if !self.settings.pinned.remove(name) {
            self.settings.pinned.insert(name.to_string());
        }
    }

    pub fn is_pinned(&self, name: &str) -> bool {
        self.settings.pinned.contains(name)
    }

    /// Rows for the advanced view's center list.
    pub fn visible_rows(&self) -> Vec<String> {
        self.rows_in(&self.ui.scope)
    }

    pub fn rail_counts(&self) -> RailCounts {
        let mut counts = RailCounts {
            changed: self.rows_in(&Scope::Changed).len(),
            favourites: self.rows_in(&Scope::Favourites).len(),
            categories: self
                .catalog
                .categories()
                .into_iter()
                .map(|c| (c.to_string(), ScopeCount::default()))
                .collect(),
            ..RailCounts::default()
        };
        for (name, entry) in self.catalog.search(&self.ui.search) {
            let changed = usize::from(self.is_changed(name));
            for count in [
                &mut counts.all,
                counts.categories.entry(entry.category.clone()).or_default(),
            ] {
                count.rows += 1;
                count.changed += changed;
            }
        }
        counts
    }

    fn rows_in(&self, scope: &Scope) -> Vec<String> {
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
    fn rows_follow_search_and_scope() {
        let (_dir, mut state) = state();
        state.ui.search = "farz".into();
        assert!(state.visible_rows().contains(&CHEAT.to_string()));
        state.ui.scope = Scope::Category("Audio".into());
        assert!(state.visible_rows().is_empty());
        state.ui.search.clear();
        state.ui.scope = Scope::Changed;
        state.set_convar(LIVE, "77".into()).unwrap();
        assert_eq!(state.visible_rows(), vec![LIVE.to_string()]);
        state.ui.scope = Scope::Favourites;
        state.toggle_favourite(CHEAT);
        assert_eq!(state.visible_rows(), vec![CHEAT.to_string()]);
        state.toggle_favourite(CHEAT);
        assert!(state.visible_rows().is_empty());
    }

    #[test]
    fn rail_counts_follow_search_edits_and_favourites() {
        let (_dir, mut state) = state();
        let counts = state.rail_counts();
        assert_eq!(counts.all.rows, state.catalog.entries.len());
        assert_eq!(
            (counts.all.changed, counts.changed, counts.favourites),
            (0, 0, 0)
        );
        assert_eq!(
            counts.categories.values().map(|c| c.rows).sum::<usize>(),
            counts.all.rows
        );
        state.set_convar(LIVE, "144".into()).unwrap();
        state.set_convar(RESTART, "true".into()).unwrap();
        state.toggle_favourite(CHEAT);
        let counts = state.rail_counts();
        assert_eq!(
            (counts.all.changed, counts.changed, counts.favourites),
            (2, 2, 1)
        );
        let live_cat = &state.catalog.get(LIVE).unwrap().category;
        assert!(counts.categories[live_cat].changed >= 1, "{live_cat}");
        state.ui.search = "farz".into();
        let counts = state.rail_counts();
        assert!(counts.all.rows > 0 && counts.all.rows < 20, "{counts:?}");
        assert_eq!(counts.all.changed, 0, "neither edit matches farz");
        assert_eq!(
            counts.categories.len(),
            state.catalog.categories().len(),
            "emptied categories stay listed"
        );
        assert_eq!(
            counts.categories.values().map(|c| c.rows).sum::<usize>(),
            counts.all.rows
        );
    }

    #[test]
    fn plan_summary_sorts_changes_by_when_they_land() {
        let (_dir, mut state) = state();
        assert_eq!(PlanSummary::of(plan(&state), false), PlanSummary::default());
        state.set_convar(LIVE, "144".into()).unwrap();
        state.set_convar(CHEAT, "6000".into()).unwrap();
        state.set_convar(RESTART, "true".into()).unwrap();
        let closed = PlanSummary::of(plan(&state), false);
        assert_eq!(
            closed,
            PlanSummary {
                live_now: 0,
                queued: 1,
                next_launch: 2,
                refused: 0
            },
            "game closed: live edits wait for launch too"
        );
        let running = PlanSummary::of(plan(&state), true);
        assert_eq!((running.live_now, running.next_launch), (1, 1));
        state.set_in_sandbox(true);
        let sandbox = PlanSummary::of(plan(&state), true);
        assert_eq!((sandbox.live_now, sandbox.queued), (2, 0));
        let refused = ApplyPlan {
            denied: vec![DENIED.into()],
            ..ApplyPlan::default()
        };
        assert_eq!(PlanSummary::of(&refused, false).refused, 1);
    }

    #[test]
    fn pins_toggle_and_stay_apart_from_favourites() {
        let (_dir, mut state) = state();
        assert!(!state.is_pinned(LIVE));
        state.toggle_pin(LIVE);
        assert!(state.is_pinned(LIVE));
        assert!(!state.settings.favourites.contains(LIVE));
        state.toggle_pin(LIVE);
        assert!(!state.is_pinned(LIVE));
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
    fn hud_presets_are_valid_layouts_the_state_can_tell_apart() {
        let (_dir, mut state) = state();
        assert_eq!(state.hud_preset(), Some(HudPreset::Vanilla));
        assert_eq!(state.hud_changed_count(), 0);
        for preset in HudPreset::ALL {
            assert!(
                dt_core::hud::layout::compile(&preset.layout()).is_ok(),
                "{preset:?}"
            );
            state.apply_hud_preset(preset);
            assert_eq!(state.hud_preset(), Some(preset));
            assert_eq!(
                state.hud_changed_count(),
                preset.layout().elements.len(),
                "{preset:?}"
            );
        }
        assert_eq!(state.hud_edit(ElementId::Minimap).scale_pct, 125);
        assert!(state.is_dirty());
        state.set_hud_element(
            ElementId::TopBar,
            ElementEdit {
                offset_y: 12,
                ..ElementEdit::default()
            },
        );
        assert_eq!(state.hud_preset(), None, "an edit leaves the preset");
        assert_eq!(state.hud_changed_count(), 4);
        state.set_hud_element(ElementId::TopBar, ElementEdit::default());
        assert_eq!(state.hud_preset(), Some(HudPreset::Competitive));
        state.reset_hud();
        assert_eq!(state.hud_preset(), Some(HudPreset::Vanilla));
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

    #[test]
    fn changed_detection_follows_the_preset() {
        let (_dir, mut state) = state();
        assert_eq!(state.changed_count(["fps_max", RESTART]), 0);
        state.set_convar(LIVE, "144".into()).unwrap();
        assert!(state.is_changed(LIVE));
        assert_eq!(state.preset_value(LIVE).as_deref(), Some("400"));
        state.set_base(BaseRef::Preset(PresetId::KaizMinspec));
        assert_eq!(state.preset_value(LIVE).as_deref(), Some("0"));
        assert!(state.is_changed(LIVE), "edits survive a preset switch");
        state.set_convar(LIVE, "0".into()).unwrap();
        assert!(
            !state.is_changed(LIVE),
            "picking the preset value is not a change"
        );
        state.comment_convar("r_ssao").unwrap();
        assert!(
            state.is_changed("r_ssao"),
            "commented out counts as changed"
        );
    }

    #[test]
    fn reset_one_section_or_everything_to_the_preset() {
        let (_dir, mut state) = state();
        state.set_convar(LIVE, "144".into()).unwrap();
        state.set_convar(RESTART, "true".into()).unwrap();
        state.set_convar("r_ssao", "false".into()).unwrap();
        state.reset_convars(["r_ssao"]);
        assert!(!state.is_changed("r_ssao"));
        assert_eq!(state.changed_count([LIVE, RESTART]), 2);
        state.reset_to_preset();
        assert_eq!(state.changed_count([LIVE, RESTART, "r_ssao"]), 0);
        assert!(
            plan(&state).is_empty(),
            "back to the preset, nothing to write"
        );
    }

    #[test]
    fn timing_says_when_changes_land() {
        let (_dir, mut state) = state();
        assert_eq!(state.timing(), Timing::Nothing);
        state.set_convar(LIVE, "144".into()).unwrap();
        assert_eq!(state.timing(), Timing::NextLaunch, "game closed");
        state.observe_game(true, None);
        assert_eq!(state.timing(), Timing::Instant);
        state.set_convar(RESTART, "true".into()).unwrap();
        assert_eq!(state.timing(), Timing::Mixed { now: 1, later: 1 });
    }

    #[test]
    fn pending_names_the_preset_switch_or_counts_edits_since_the_last_apply() {
        let (_dir, mut state) = state();
        assert_eq!(state.pending(), Pending::Nothing);
        state.set_convar(LIVE, "144".into()).unwrap();
        state.set_convar(RESTART, "true".into()).unwrap();
        assert_eq!(state.pending(), Pending::Tweaks(2));
        state.apply().unwrap();
        state.save_profile().unwrap();
        assert_eq!(state.pending(), Pending::Nothing, "applied and saved");
        state.set_convar(LIVE, "90".into()).unwrap();
        assert_eq!(
            state.pending(),
            Pending::Tweaks(1),
            "only the edit since the last Apply counts"
        );
        state.set_base(BaseRef::Preset(PresetId::KaizMinspec));
        assert_eq!(
            state.pending(),
            Pending::Preset {
                label: "Kaizuchaneru minimum spec".into(),
                tweaks: 2
            },
            "a preset switch carries every tweak"
        );
        state.reset_to_preset();
        assert_eq!(
            state.pending(),
            Pending::Preset {
                label: "Kaizuchaneru minimum spec".into(),
                tweaks: 0
            }
        );
    }
}
