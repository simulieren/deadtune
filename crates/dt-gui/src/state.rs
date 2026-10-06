//! Everything the window shows, and every transition on it, without a window.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Instant, SystemTime};

use dt_core::addons::guard::{self, Event as GuardEvent, Guard, Pak, PakState, Verdict};
use dt_core::addons::install::{Action, AddonsPlan, InstalledState};
use dt_core::addons::textures::Progress;
use dt_core::addons::{self, AddonId, AddonsConfig};
use dt_core::addons::{TextureDownscale, TextureStats};
use dt_core::apply::{
    self, ApplyContext, ApplyError, ApplyPlan, ApplyReport, BaseTexts, FileWrite,
};
use dt_core::backup::{BackupEntry, BackupStore, FileKind, sha256_hex};
use dt_core::bridge::ConsoleCmd;
use dt_core::bridge::ack::{PushStatus, Tracker};
use dt_core::bridge::boot::BootCfg;
use dt_core::bridge::conlog::LogTail;
use dt_core::catalog::{ApplyClass, Catalog};
use dt_core::gi::{self, Override};
use dt_core::hud::apples_tunnels::{
    APPLE_RADIUS_RANGE, ApplesTunnels, DOT_SIZE_RANGE, RADIUS_RANGE,
};
use dt_core::hud::elements::ElementId;
use dt_core::hud::health_style::HealthStyle;
use dt_core::hud::ingame::{self, IngameSettings};
use dt_core::hud::install::{HudAction, HudPlan, Refreshed};
use dt_core::hud::layout::{ElementEdit, HudLayout};
use dt_core::hud::minimap_colors::{self, Color, IconId};
use dt_core::hud::minimap_style::{
    MAP_OPACITY_RANGE, MARKER_SCALE_RANGE, MarkerGroup, MinimapStyle,
};
use dt_core::hud::player_stats::PlayerStatsStyle;
use dt_core::hud::topbar::{
    MISSING_OPACITY_RANGE, PORTRAIT_GAP_RANGE, PORTRAIT_SCALE_RANGE, TopBarPreset, TopBarStyle,
};
use dt_core::launch::{self, LaunchArgs};
use dt_core::locate::GamePaths;
use dt_core::practice::{self, PracticeMode};
use dt_core::preset::{self, PresetId, remote};
use dt_core::profile::{self, BaseRef, ConVarEdits, Profile};
use dt_core::watch::{self, Change, Watcher};

use crate::bench::{self, BenchState};
use crate::live::{BridgeTarget, LivePush, PushOutcome};
use crate::profiles;
use crate::relaunch::Relaunch;
use crate::settings::{Settings, TargetSource};
use crate::update::{self, Release, Updater};
use dt_core::doctor::Check;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveFiles {
    pub gameinfo: String,
    pub video: Option<String>,
    /// What gameinfo.gi holds right now, for the "practice mode on" chips.
    pub practice: PracticeMode,
}

impl LiveFiles {
    pub fn read(paths: &GamePaths) -> io::Result<LiveFiles> {
        let gameinfo = std::fs::read_to_string(&paths.gameinfo)?;
        Ok(LiveFiles {
            practice: practice::detect(&gameinfo).unwrap_or_default(),
            gameinfo,
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
    fn resolve(profile: &Profile, presets_dir: &Path) -> Result<Base, String> {
        let texts = apply::resolve_base(profile, presets_dir).map_err(|e| e.to_string())?;
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
    Minimap,
    TopBar,
    Images,
    Health,
    Stats,
    Ingame,
    Addons,
    System,
    GameFiles,
    Safety,
}

impl Section {
    pub const ALL: [Section; 17] = [
        Section::Overview,
        Section::Display,
        Section::Shadows,
        Section::Effects,
        Section::World,
        Section::Performance,
        Section::Hud,
        Section::Minimap,
        Section::TopBar,
        Section::Images,
        Section::Health,
        Section::Stats,
        Section::Ingame,
        Section::Addons,
        Section::System,
        Section::GameFiles,
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
            Section::Minimap => "Minimap",
            Section::TopBar => "Top bar",
            Section::Images => "UI images",
            Section::Health => "Health bar",
            Section::Stats => "Player stats",
            Section::Ingame => "In-game settings",
            Section::Addons => "Addons",
            Section::System => "System check",
            Section::GameFiles => "Game files",
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
            Section::Minimap => "Colours, marker sizes and the look of the minimap.",
            Section::TopBar => "Hero portraits, clock and soul lead, plus spawn timers and more.",
            Section::Images => {
                "Every picture and icon in the game's interface. Look at any of them, save a copy, or drop in your own."
            }
            Section::Health => "A bigger health number, colours by health, less shaking.",
            Section::Stats => {
                "Your stat numbers, level, souls and items in the lower left: move, resize, recolour or hide every part."
            }
            Section::Ingame => {
                "DeadTune rows inside Deadlock's own settings menu: a Wide FOV slider and live performance sliders."
            }
            Section::Addons => {
                "Community performance mods, rebuilt by DeadTune so they survive game updates."
            }
            Section::System => {
                "Is everything set up for good FPS? Checks the game files, DeadTune and your Windows settings."
            }
            Section::GameFiles => {
                "Saves copies of the game's interface files so we can see what an update changed."
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
    /// Written, but the engine does not take them from gameinfo.gi.
    pub ignored: usize,
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
                + plan.sections.len()
                + plan.video_changes.len()
                + plan.addon_changes()
                + (plan.live.len() - live_now),
            ignored: plan.ignored.len(),
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
    /// The part the Player stats page is editing.
    pub stats_selected: Option<dt_core::hud::player_stats::StatsPart>,
    pub hud_backdrop: Backdrop,
    pub hud_page: HudPage,
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
    /// Path typed into a remote preset's Import box.
    pub preset_import_path: String,
    /// Screenshot lever: hold the "All presets" dropdown open.
    pub open_presets: bool,
    /// The launch-guard banner's "Show details" fold.
    pub guard_details: bool,
    /// The Launch options window, opened from the Launch button's menu.
    pub launch_options_open: bool,
    /// The Top bar page's mock shows an enemy out of vision and a dead hero.
    pub top_bar_preview: TopBarPreview,
    /// The Game files page's compare pickers: indexes into `snapshots` (older, newer).
    pub snapshot_compare: (usize, usize),
    /// The snapshot whose Delete button was pressed once and waits for a second press.
    pub snapshot_delete_armed: Option<PathBuf>,
}

/// The HUD layout preview's game screenshot behind the element boxes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Backdrop {
    pub shown: bool,
    /// 0 to 1.
    pub opacity: f32,
}

impl Default for Backdrop {
    fn default() -> Backdrop {
        Backdrop {
            shown: true,
            opacity: 0.85,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TopBarPreview {
    pub missing_enemy: bool,
    pub dead_hero: bool,
}

impl Default for TopBarPreview {
    fn default() -> Self {
        TopBarPreview {
            missing_enemy: true,
            dead_hero: true,
        }
    }
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
            ..HudLayout::default()
        }
    }
}

/// The HUD tab's pages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HudPage {
    #[default]
    Layout,
    Colors,
    TopBar,
    Health,
    Ingame,
    Images,
}

/// The game's own enemy colour setting (2026-09-29 accessibility update).
pub const CUSTOM_UI_COLORS: &str = "citadel_custom_ui_colors";
pub const ENEMY_UI_COLOR: [&str; 3] = [
    "citadel_enemy_ui_color_r",
    "citadel_enemy_ui_color_g",
    "citadel_enemy_ui_color_b",
];

/// Minimap colour schemes, as plain `IconId` values the user can keep editing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MinimapPreset {
    ColorBlind,
    HighContrast,
}

impl MinimapPreset {
    pub const ALL: [MinimapPreset; 2] = [MinimapPreset::ColorBlind, MinimapPreset::HighContrast];

    pub fn label(self) -> &'static str {
        match self {
            MinimapPreset::ColorBlind => "Colour-blind friendly",
            MinimapPreset::HighContrast => "High contrast",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            MinimapPreset::ColorBlind => {
                "Allies blue, enemies orange. Allied objectives keep their lane colours."
            }
            MinimapPreset::HighContrast => {
                "Allies cyan, enemies magenta, fully saturated. Allied objectives keep their lane colours."
            }
        }
    }

    pub fn colors(self) -> BTreeMap<IconId, Color> {
        let (ally, enemy) = match self {
            MinimapPreset::ColorBlind => ([0x56, 0xB4, 0xE9, 255], [0xE6, 0x9F, 0x00, 255]),
            MinimapPreset::HighContrast => ([0x00, 0xF0, 0xFF, 255], [0xFF, 0x00, 0xD4, 255]),
        };
        [
            (IconId::AllyHero, ally),
            (IconId::AllyHeroArrow, ally),
            (IconId::AllyUrnReturn, ally),
            (IconId::EnemyHero, enemy),
            (IconId::EnemyHeroArrow, enemy),
            (IconId::EnemyObjective, enemy),
            (IconId::EnemyUrnReturn, enemy),
        ]
        .into_iter()
        .map(|(id, rgba)| (id, Color(rgba)))
        .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Info(String),
    /// It worked, with a catch the player should know about.
    Warn(String),
    Error(String),
}

enum BuildMsg {
    Progress(Progress),
    Done(Result<TextureStats, String>),
}

pub struct TextureBuild {
    rx: Receiver<BuildMsg>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub progress: Progress,
}

impl TextureBuild {
    /// The build thread stops after the texture it is on and removes its output.
    pub fn cancel(&self) {
        self.cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
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
    /// Follows the last push through the console log.
    pub ack: Tracker,
    conlog: LogTail,
    conlog_polled: Option<Instant>,
    /// Decides whether the addons the game started with are safe; see `dt_core::addons::guard`.
    pub guard: Guard,
    /// Console log lines since the guard last looked.
    guard_lines: Vec<String>,
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
    /// What the last finished build did, for the card.
    pub last_texture_build: Option<Result<TextureStats, String>>,
    /// A game file snapshot or comparison running on its own thread (`crate::snapshots`).
    pub snapshot_job: Option<crate::snapshots::SnapshotJob>,
    /// What the last snapshot job did, for the Game files page.
    pub last_snapshot: Option<Result<crate::snapshots::SnapshotDone, String>>,
    /// Snapshot folders, newest first; refreshed after every job and delete.
    pub snapshots: Vec<dt_core::snapshot::SnapshotInfo>,
    /// The newest report among the snapshots.
    pub latest_diff: Option<dt_core::snapshot::SnapshotDiff>,
    /// The pak tree classified, scanned when the page first needs it; dropped on a game update.
    pub snapshot_inventory: Option<Result<dt_core::snapshot::Inventory, String>>,
    /// The UI images page (`crate::images`).
    pub images: crate::images::ImagesState,
    /// Game images the HUD previews draw (`crate::hud_art`).
    pub hud_art: crate::hud_art::HudArtState,
    /// The game build the appmanifest reports, read with the snapshot listing.
    pub game_build: Option<String>,
    /// Look for a HUD pak built for an older game build the next time the game is not
    /// running: at start, after a game update and when the game closes.
    hud_update_check: bool,
    /// Creation time of the backup the last Undo restored; cleared by Apply.
    pub undo_cursor: Option<chrono::DateTime<chrono::Utc>>,
    /// Last "Check setup" run; `None` until the panel is opened.
    pub checks: Option<Vec<Check>>,
    pub checks_at: Option<std::time::SystemTime>,
    /// Doctor runs off the UI thread: on Windows it shells out to PowerShell for RAM info.
    checks_rx: Option<Receiver<Vec<Check>>>,
    pub update: Updater,
    #[cfg(feature = "remote")]
    pub remote: Option<crate::remote::Remote>,
    watch: Option<(Watcher, Receiver<Vec<Change>>)>,
    /// Cache state of every remote preset (SideLock), refreshed after each fetch, import,
    /// accept or discard rather than hashed every frame.
    pub remote_presets: BTreeMap<PresetId, RemoteView>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteView {
    pub status: remote::Status,
    /// What a file waiting for review would change; empty when none is.
    pub changes: Vec<remote::Change>,
}

fn remote_views(presets_dir: &Path) -> BTreeMap<PresetId, RemoteView> {
    preset::all()
        .iter()
        .filter_map(|p| p.remote())
        .map(|r| {
            let dir = remote::dir(r, presets_dir);
            let status = remote::status(r, &dir).unwrap_or(remote::Status {
                active: None,
                pending: None,
            });
            let changes = remote::pending_changes(&dir).unwrap_or_default();
            (r.id, RemoteView { status, changes })
        })
        .collect()
}

/// Shown when the files changed between preview and Apply; the preview is already refreshed.
const CONLOG_POLL: std::time::Duration = std::time::Duration::from_millis(250);

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
        practice: PracticeMode::default(),
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
        let store = BackupStore::open(dt_core::backup::records_dir(&data_dir))?;
        let dir = profiles::dir(&data_dir);
        let presets_dir = preset::cache_dir(&data_dir);
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
            base: Base::resolve(&profile, &presets_dir),
            conlog: LogTail::for_game(&paths),
            conlog_polled: None,
            guard: Guard::load(&store.root).unwrap_or_default(),
            guard_lines: Vec::new(),
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
            ack: Tracker::default(),
            relaunch: Relaunch::Idle,
            bench: BenchState::default(),
            ui: UiState::default(),
            status: None,
            welcome: None,
            hud_cache: None,
            addons_cache: None,
            texture_build: None,
            last_texture_build: None,
            snapshot_job: None,
            last_snapshot: None,
            snapshots: Vec::new(),
            latest_diff: None,
            snapshot_inventory: None,
            images: Default::default(),
            hud_art: Default::default(),
            game_build: None,
            hud_update_check: true,
            undo_cursor: None,
            checks: None,
            checks_at: None,
            checks_rx: None,
            update: Updater::default(),
            #[cfg(feature = "remote")]
            remote: None,
            watch: None,
            remote_presets: remote_views(&presets_dir),
        };
        if !state.settings.onboarded {
            state.welcome = Some(Welcome::PickStart { choice: None });
        }
        state.refresh_preview();
        state.reload_bench();
        state.refresh_snapshots();
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
        let (tx, rx) = channel();
        // The addon and HUD records live in the store root, not the data dir.
        let (paths, data_dir) = (self.paths.clone(), self.store.root.clone());
        std::thread::spawn(move || {
            let mut checks = dt_core::doctor::run(Some(&paths), &data_dir);
            // Screenshot lever: Windows checks on sample facts, for working on a Mac.
            if std::env::var_os("DEADTUNE_FAKE_WINDOWS").is_some_and(|v| v == "1") {
                checks.extend(crate::checks_view::sample_windows_checks());
            }
            let _ = tx.send(checks);
        });
        self.checks_rx = Some(rx);
    }

    /// Warnings and failures from the last system check, for the sidebar badge.
    pub fn check_problems(&self) -> usize {
        self.checks.as_ref().map_or(0, |checks| {
            checks
                .iter()
                .filter(|c| c.status != dt_core::doctor::CheckStatus::Pass)
                .count()
        })
    }

    pub fn checks_running(&self) -> bool {
        self.checks_rx.is_some()
    }

    pub fn poll_checks(&mut self) {
        let Some(rx) = &self.checks_rx else { return };
        match rx.try_recv() {
            Ok(checks) => {
                self.checks = Some(checks);
                self.checks_at = Some(std::time::SystemTime::now());
                self.checks_rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => self.checks_rx = None,
        }
    }

    /// `manual` ("Check now") also reports a version the user skipped.
    pub fn check_update(&mut self, manual: bool) {
        let settings = &self.settings.update;
        let skipped = if manual { None } else { settings.skipped };
        self.update.check(settings.channel, skipped);
    }

    pub fn install_update(&mut self, release: Release) {
        self.update.install(release);
    }

    pub fn skip_update(&mut self, release: &Release) {
        self.settings.update.skipped = Some(release.version);
        self.update.state = update::UpdateState::Idle;
    }

    /// Returns `true` when the new version is installed and the window should restart.
    pub fn poll_update(&mut self) -> bool {
        let mut restart = false;
        for action in self.update.poll() {
            match action {
                update::Action::Checked(at) => self.settings.update.last_check = Some(at),
                update::Action::Restart => restart = true,
            }
        }
        restart
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

    /// Ranked-safe and safe mode both take every DeadTune pak out of the game folder.
    fn without_addons(&self) -> bool {
        self.settings.source == TargetSource::RankedSafe || self.settings.safe_mode
    }

    pub fn refresh_preview(&mut self) {
        // Ranked-safe also takes our HUD addon out: stock means stock.
        let layout = if self.without_addons() {
            &HudLayout::default()
        } else {
            &self.profile.hud
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
        // convar changes; the HUD tab shows the reason instead. The exact build that broke
        // the last launch stays out until the player asks to try it again.
        let hud = hud.unwrap_or(None).map(|mut plan| {
            if self.guard.holds_back(&plan) {
                plan.action = HudAction::Nothing;
            }
            plan
        });
        let config = if self.without_addons() {
            &AddonsConfig::default()
        } else {
            &self.profile.addons
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
                    apply::Extras {
                        hud,
                        addons,
                        practice: practice::Record::load(&self.store.root).unwrap_or_default(),
                    },
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

    /// Whether the preset or the profile gives `name` a value, so a `+name` launch option
    /// competes with it.
    pub fn sets_convar(&self, name: &str) -> bool {
        self.profile.convars.set.contains_key(name) || self.base_value(name).is_some()
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
        if let Some((key, text)) = self.video_twin(name, &value) {
            if is_base {
                self.profile.video.remove(&key);
            } else {
                self.profile.video.insert(key, text);
            }
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
        self.images.edit.forget();
        self.images.crop_draft = None;
        self.profile = self.saved.clone().unwrap_or_else(default_profile);
        self.base = Base::resolve(&self.profile, &self.presets_dir());
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
        for name in names {
            let edits = &mut self.profile.convars;
            edits.set.remove(name);
            edits.comment.retain(|c| c != name);
            self.profile.video.remove(&format!("setting.{name}"));
        }
        self.refresh_preview();
    }

    /// The `video.txt` key holding the same setting as ConVar `name`, with `value` written
    /// in that file's style. The game's menu writes `video.txt`, so a ConVar edit alone
    /// would leave the two files disagreeing.
    fn video_twin(&self, name: &str, value: &str) -> Option<(String, String)> {
        let key = format!("setting.{name}");
        let current = dt_core::video::read_settings(self.live.video.as_deref()?)
            .ok()?
            .into_iter()
            .find(|(k, _)| *k == key)?
            .1;
        let boolean = matches!(current.as_str(), "true" | "false");
        let text = if boolean && matches!(value.trim(), "0" | "1" | "true" | "false") {
            bool_text(parse_bool(value), Some(&current))
        } else {
            value.to_string()
        };
        Some((key, text))
    }

    /// Drops every setting edit, and the `video.txt` twins of those settings, so the
    /// profile is the preset again; HUD and other video edits stay.
    pub fn reset_to_preset(&mut self) {
        let names: Vec<String> = self.profile.convars.set.keys().cloned().collect();
        for name in names {
            self.profile.video.remove(&format!("setting.{name}"));
        }
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

    /// Downloaded and imported remote presets.
    pub fn presets_dir(&self) -> PathBuf {
        preset::cache_dir(&self.data_dir)
    }

    pub fn set_practice(&mut self, mode: PracticeMode) {
        self.profile.practice = mode;
        self.refresh_preview();
    }

    pub fn set_base(&mut self, base: BaseRef) {
        self.profile.base = base;
        self.base = Base::resolve(&self.profile, &self.presets_dir());
        self.refresh_preview();
    }

    fn remote_dir(&self, id: PresetId) -> PathBuf {
        remote::dir(preset::remote(id), &self.presets_dir())
    }

    /// Re-reads the remote preset caches, and the base if it is one of them.
    fn remote_changed(&mut self) {
        self.remote_presets = remote_views(&self.presets_dir());
        self.set_base(self.profile.base.clone());
    }

    /// Takes a downloaded cfg.zip or gameinfo.gi; one that differs from the pinned file waits
    /// in `remote_presets` for [`AppState::accept_remote`].
    pub fn import_remote(&mut self, id: PresetId, path: &Path) -> Result<(), String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let result = remote::stage(preset::remote(id), &self.remote_dir(id), &bytes);
        self.remote_changed();
        result.map(drop).map_err(|e| e.to_string())
    }

    #[cfg(feature = "fetch")]
    pub fn fetch_remote(&mut self, id: PresetId) -> Result<(), String> {
        let result = remote::fetch(preset::remote(id), &self.remote_dir(id));
        self.remote_changed();
        result.map(drop).map_err(|e| e.to_string())
    }

    pub fn accept_remote(&mut self, id: PresetId) -> Result<(), String> {
        let result = remote::accept(&self.remote_dir(id));
        self.remote_changed();
        result.map_err(|e| e.to_string())
    }

    pub fn discard_remote(&mut self, id: PresetId) -> Result<(), String> {
        let result = remote::discard(&self.remote_dir(id));
        self.remote_changed();
        result.map_err(|e| e.to_string())
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

    /// Safe mode removes every DeadTune pak (performance addons and the HUD) right away and
    /// keeps them out until it is turned off; the profile remembers what was on.
    pub fn toggle_safe_mode(&mut self) -> Result<Applied, String> {
        let previous = self.settings.safe_mode;
        self.settings.safe_mode = !previous;
        self.refresh_preview();
        let result = self.apply();
        if result.is_err() {
            self.settings.safe_mode = previous;
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
            self.hud_update_check |= !running;
            self.refresh_preview();
        }
        if !running && std::mem::take(&mut self.hud_update_check) {
            self.refresh_hud_after_update();
        }
        self.guard_tick(running, started_at, SystemTime::now());
    }

    /// A HUD pak from before a game update carries the old game files and can stop the
    /// game from starting, so it is rebuilt from the new ones, or taken out.
    fn refresh_hud_after_update(&mut self) {
        let message = match dt_core::hud::install::refresh_after_update(
            &self.paths,
            &self.store.root,
        ) {
            Ok(Refreshed::Current) => return,
            Ok(Refreshed::Rebuilt) => Status::Info(
                "The game updated, so DeadTune rebuilt your HUD changes from the new game files."
                    .into(),
            ),
            Ok(Refreshed::Removed(why)) => Status::Error(format!(
                "The game updated and your HUD changes could not be rebuilt ({why}), so DeadTune \
                 took them out to keep the game starting. Press Apply to put them back."
            )),
            Err(e) => Status::Error(format!("checking the HUD after a game update: {e}")),
        };
        self.status = Some(message);
        self.hud_cache = None;
        self.refresh_preview();
    }

    /// One guard step per game poll; a verdict rolls back failed paks or marks good ones.
    pub fn guard_tick(&mut self, running: bool, started_at: Option<SystemTime>, now: SystemTime) {
        let installed = guard::installed_paks(&self.paths, &self.store.root);
        let lines = std::mem::take(&mut self.guard_lines);
        let build = dt_core::locate::buildid(&self.paths);
        let event = self.guard.step(&guard::Observation {
            running,
            started_at,
            now,
            installed: &installed,
            lines: &lines,
            build: build.as_deref(),
        });
        let Some(event) = event else {
            return;
        };
        match event {
            GuardEvent::Started { changed } => {
                self.status = Some(Status::Info(trial_started_message(&changed)));
            }
            GuardEvent::Passed { ids } => {
                self.keep_verified_hud();
                self.status = Some(Status::Info(format!(
                    "Deadlock started fine with {}.",
                    guard::names(&ids)
                )));
                self.install_candidate();
            }
            GuardEvent::Failed { ids, .. } => {
                self.rollback_paks(&ids);
                if let Some(failure) = &self.guard.failure {
                    self.status = Some(Status::Error(failure.message().headline));
                }
                self.install_candidate();
            }
        }
        self.save_guard();
    }

    fn save_guard(&mut self) {
        if let Err(e) = self.guard.save(&self.store.root) {
            self.status = Some(Status::Error(format!("saving the launch guard: {e}")));
        }
    }

    fn keep_verified_hud(&mut self) {
        if let Err(e) = guard::keep_verified_hud(&self.guard, &self.paths, &self.store.root) {
            self.status = Some(Status::Error(format!(
                "keeping a copy of the HUD that worked: {e}"
            )));
        }
    }

    /// Rolls back the paks of `ids` (only files DeadTune's records name). Failed addons are
    /// switched off in the profile, on disk too, so no later Apply puts them back unasked;
    /// the HUD settings stay, and Apply holds back the exact build that failed.
    fn rollback_paks(&mut self, ids: &[Pak]) {
        guard::rollback(&mut self.guard, ids, &self.paths, &self.store.root);
        let addons: Vec<AddonId> = ids
            .iter()
            .filter_map(|p| match p {
                Pak::Addon(id) => Some(*id),
                Pak::Hud => None,
            })
            .collect();
        if !addons.is_empty() {
            for id in &addons {
                self.profile.addons.set_enabled(*id, false);
            }
            let dir = self.profiles_dir();
            if let Some(saved) = &mut self.saved {
                for id in &addons {
                    saved.addons.set_enabled(*id, false);
                }
                if let Err(e) = profiles::save(&dir, saved) {
                    self.status = Some(Status::Error(format!("saving the profile: {e}")));
                }
            }
        }
        self.addons_cache = None;
        self.hud_cache = None;
        self.refresh_preview();
    }

    /// Starts the one-at-a-time flow for the last failure's suspects.
    pub fn start_one_at_a_time(&mut self) {
        let Some(failure) = &self.guard.failure else {
            return;
        };
        self.guard.start_sequence(failure.ids.clone());
        self.install_candidate();
        self.save_guard();
    }

    pub fn stop_one_at_a_time(&mut self) {
        self.guard.stop_sequence();
        self.save_guard();
    }

    /// Switches the sequence's next suspect on and installs it for the next launch.
    fn install_candidate(&mut self) {
        match self.guard.candidate() {
            Some(Pak::Addon(id)) => {
                self.profile.addons.set_enabled(id, true);
                self.addons_cache = None;
                self.refresh_preview();
                let name = addons::info(id).name;
                self.status = Some(match self.apply() {
                    Ok(_) => Status::Info(format!(
                        "{name} is on and installed. Start Deadlock to test it; DeadTune reports the result."
                    )),
                    Err(e) => Status::Error(format!("Couldn't install {name} for its test: {e}")),
                });
            }
            Some(Pak::Hud) => self.reinstall_hud(),
            None => {}
        }
    }

    /// "Try again" for the HUD: lets the build that failed back in and installs it.
    pub fn retry_hud(&mut self) {
        self.reinstall_hud();
        self.save_guard();
    }

    fn reinstall_hud(&mut self) {
        self.guard.retry(Pak::Hud);
        self.hud_cache = None;
        self.refresh_preview();
        self.status = Some(match self.apply() {
            Ok(_) => Status::Info(
                "DeadTune's HUD changes are back on. Start Deadlock to test them; DeadTune reports the result."
                    .into(),
            ),
            Err(e) => Status::Error(format!("Couldn't put the HUD changes back: {e}")),
        });
    }

    /// True when Apply is holding back the HUD build that stopped the last launch.
    pub fn hud_held(&self) -> bool {
        matches!(&self.hud_cache, Some((_, Ok(Some(plan)))) if self.guard.holds_back(plan))
    }

    pub fn dismiss_guard_failure(&mut self) {
        self.guard.dismiss();
        self.save_guard();
    }

    /// What the launch guard knows about an addon's pak.
    pub fn addon_badge(&self, id: AddonId, installed: Option<&InstalledState>) -> Option<PakState> {
        let sha = match installed {
            Some(InstalledState::Current(r) | InstalledState::Stale(r)) => Some(r.sha256.as_str()),
            _ => None,
        };
        self.guard.state_of(id, sha)
    }

    /// What the launch guard knows about the installed HUD pak.
    pub fn hud_trial_state(&self) -> Option<PakState> {
        let installed = guard::installed_paks(&self.paths, &self.store.root);
        let sha = installed
            .iter()
            .find(|p| p.id == Pak::Hud)
            .map(|p| p.sha256.as_str());
        self.guard.state_of(Pak::Hud, sha)
    }

    /// Screenshot lever and test hook: a failed trial for `ids` with `fatal` captured, as if
    /// the game had just died with them installed.
    pub fn inject_trial_failure(&mut self, ids: Vec<Pak>, fatal: Option<String>) {
        let now = guard::unix(SystemTime::now());
        let installed = guard::installed_paks(&self.paths, &self.store.root);
        self.guard.failure = Some(guard::Failure {
            ids: ids.clone(),
            fatal: fatal.clone(),
            at: now,
            removed: Vec::new(),
            kept: Vec::new(),
            hud: None,
        });
        for id in &ids {
            self.guard.verdicts.insert(
                *id,
                Verdict::Failed {
                    at: now,
                    fatal: fatal.clone(),
                    sha256: installed
                        .iter()
                        .find(|p| p.id == *id)
                        .map(|p| p.sha256.clone()),
                },
            );
        }
        self.rollback_paks(&ids);
        self.save_guard();
    }

    /// Screenshot lever: a trial running for `ids`, as if the game had just started with them.
    pub fn inject_trial_started(&mut self, ids: Vec<Pak>) {
        let installed = guard::installed_paks(&self.paths, &self.store.root);
        let changed: guard::PakSet = ids
            .iter()
            .map(|id| {
                let sha = installed
                    .iter()
                    .find(|p| p.id == *id)
                    .map(|p| p.sha256.clone());
                (*id, sha.unwrap_or_default())
            })
            .collect();
        self.guard.trial = Some(guard::Trial {
            loaded: changed.clone(),
            changed,
            launched_at: guard::unix(SystemTime::now()),
            build: None,
        });
        self.status = Some(Status::Info(trial_started_message(&ids)));
    }

    /// Screenshot lever: every installed pak counts as started with.
    pub fn inject_trial_verified(&mut self) {
        let now = guard::unix(SystemTime::now());
        for pak in guard::installed_paks(&self.paths, &self.store.root) {
            self.guard.last_good.insert(pak.id, pak.sha256);
            self.guard
                .verdicts
                .insert(pak.id, Verdict::Verified { at: now });
        }
        self.keep_verified_hud();
        self.save_guard();
    }

    pub fn diagnostic_report(&self) -> String {
        dt_core::doctor::report(
            Some(&self.paths),
            &self.store.root,
            env!("CARGO_PKG_VERSION"),
            &self.launch_args().args,
        )
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
        let updated = build.is_some();
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
        if updated {
            self.hud_update_check = true;
            if !self.ctx.game_running {
                self.hud_update_check = false;
                self.refresh_hud_after_update();
            }
            self.snapshot_inventory = None;
            self.game_build = dt_core::locate::buildid(&self.paths);
            if self.settings.snapshots.auto {
                self.start_snapshot(true);
            }
        }
    }

    /// Why the addons cannot be planned right now, if they cannot.
    /// Forgets a failed addon plan so the next preview tries again (e.g. after Steam finished
    /// updating the game files).
    pub fn retry_addons(&mut self) {
        self.addons_cache = None;
        self.refresh_preview();
    }

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

    /// Deletes our installed pak for `id` immediately and switches the addon off in the
    /// profile, so the next Apply doesn't put it back.
    pub fn remove_addon_now(&mut self, id: AddonId) -> Result<bool, String> {
        let removed = addons::install::remove_now(id, &self.paths, &self.store.root)
            .map_err(|e| e.to_string())?;
        self.profile.addons.set_enabled(id, false);
        self.addons_cache = None;
        self.refresh_preview();
        Ok(removed)
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

    pub fn set_clutter_group(&mut self, group: &str, hide: bool) {
        if hide {
            self.profile.addons.hide_clutter.insert(group.to_string());
        } else {
            self.profile.addons.hide_clutter.remove(group);
        }
        self.refresh_preview();
    }

    pub fn set_blur(&mut self, opts: addons::BlurOptions) {
        self.profile.addons.blur = opts;
        self.refresh_preview();
    }

    pub fn set_scope(&mut self, opts: addons::ScopeOptions) {
        self.profile.addons.scope = opts;
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
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop = cancel.clone();
        std::thread::spawn(move || {
            let progress_tx = tx.clone();
            let result = addons::install::build_textures(&paths, &config, &state_dir, &mut |p| {
                let _ = progress_tx.send(BuildMsg::Progress(p));
                if stop.load(std::sync::atomic::Ordering::Relaxed) {
                    std::ops::ControlFlow::Break(())
                } else {
                    std::ops::ControlFlow::Continue(())
                }
            })
            .map_err(|e| e.to_string());
            let _ = tx.send(BuildMsg::Done(result));
        });
        self.last_texture_build = None;
        self.texture_build = Some(TextureBuild {
            rx,
            cancel,
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
        self.status = Some(match &result {
            Ok(stats) => Status::Info(format!(
                "Texture pak built: {}. Takes effect next time you start Deadlock.",
                addons::textures::summary(stats)
            )),
            Err(e) => Status::Error(format!("texture build: {e}")),
        });
        self.last_texture_build = Some(result);
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

    /// Recoloured icons, minimap style edits and the game's own enemy colour settings that are set.
    pub fn minimap_changed_count(&self) -> usize {
        let set = &self.profile.convars.set;
        self.profile.hud.minimap_colors.len()
            + self.profile.hud.minimap.changed_count()
            + self.profile.hud.apples_tunnels.changed_count()
            + ENEMY_UI_COLOR
                .iter()
                .chain([&CUSTOM_UI_COLORS])
                .filter(|name| set.contains_key(**name))
                .count()
    }

    /// Layout presets leave the Minimap and Top bar pages' settings alone.
    pub fn apply_hud_preset(&mut self, preset: HudPreset) {
        let minimap_colors = std::mem::take(&mut self.profile.hud.minimap_colors);
        let minimap = std::mem::take(&mut self.profile.hud.minimap);
        let top_bar = std::mem::take(&mut self.profile.hud.top_bar);
        let health = std::mem::take(&mut self.profile.hud.health);
        let apples_tunnels = self.profile.hud.apples_tunnels;
        let ingame = std::mem::take(&mut self.profile.hud.ingame);
        let icons = std::mem::take(&mut self.profile.hud.icons);
        self.profile.hud = HudLayout {
            minimap_colors,
            minimap,
            top_bar,
            health,
            apples_tunnels,
            ingame,
            icons,
            ..preset.layout()
        };
        self.refresh_preview();
    }

    /// The toolbar preset the current layout equals, if any, ignoring the Minimap and
    /// Top bar pages.
    pub fn hud_preset(&self) -> Option<HudPreset> {
        HudPreset::ALL.into_iter().find(|p| {
            HudLayout {
                minimap_colors: self.profile.hud.minimap_colors.clone(),
                minimap: self.profile.hud.minimap.clone(),
                top_bar: self.profile.hud.top_bar.clone(),
                health: self.profile.hud.health.clone(),
                apples_tunnels: self.profile.hud.apples_tunnels,
                ingame: self.profile.hud.ingame.clone(),
                icons: self.profile.hud.icons.clone(),
                ..p.layout()
            } == self.profile.hud
        })
    }

    /// Replaces the top bar style; values are clamped to their ranges.
    pub fn set_top_bar(&mut self, style: TopBarStyle) {
        self.profile.hud.top_bar = TopBarStyle {
            missing_opacity_pct: style
                .missing_opacity_pct
                .clamp(*MISSING_OPACITY_RANGE.start(), *MISSING_OPACITY_RANGE.end()),
            portrait_scale_pct: style
                .portrait_scale_pct
                .clamp(*PORTRAIT_SCALE_RANGE.start(), *PORTRAIT_SCALE_RANGE.end()),
            portrait_gap_px: style
                .portrait_gap_px
                .clamp(*PORTRAIT_GAP_RANGE.start(), *PORTRAIT_GAP_RANGE.end()),
            ..style
        };
        self.refresh_preview();
    }

    pub fn apply_top_bar_preset(&mut self, preset: TopBarPreset) {
        self.set_top_bar(preset.style());
    }

    pub fn top_bar_preset(&self) -> Option<TopBarPreset> {
        TopBarPreset::ALL
            .into_iter()
            .find(|p| p.style() == self.profile.hud.top_bar)
    }

    pub fn top_bar_changed_count(&self) -> usize {
        self.profile.hud.top_bar.changed_count()
    }

    /// Clamped to the allowed range; 100 % is not stored.
    pub fn set_marker_scale(&mut self, group: MarkerGroup, pct: u16) {
        let pct = pct.clamp(*MARKER_SCALE_RANGE.start(), *MARKER_SCALE_RANGE.end());
        let sizes = &mut self.profile.hud.minimap.marker_scale_pct;
        if pct == 100 {
            sizes.remove(&group);
        } else {
            sizes.insert(group, pct);
        }
        self.refresh_preview();
    }

    pub fn set_map_opacity(&mut self, pct: u8) {
        self.profile.hud.minimap.map_opacity_pct =
            pct.clamp(*MAP_OPACITY_RANGE.start(), *MAP_OPACITY_RANGE.end());
        self.refresh_preview();
    }

    /// Replaces the apples and tunnels options; sizes and radius are clamped to their ranges.
    pub fn set_apples_tunnels(&mut self, mut style: ApplesTunnels) {
        for dots in [&mut style.apples, &mut style.tunnels] {
            dots.size_px = dots
                .size_px
                .clamp(*DOT_SIZE_RANGE.start(), *DOT_SIZE_RANGE.end());
        }
        style.tunnel_radius_pct = style
            .tunnel_radius_pct
            .clamp(*RADIUS_RANGE.start(), *RADIUS_RANGE.end());
        style.apple_radius_pct = style
            .apple_radius_pct
            .clamp(*APPLE_RADIUS_RANGE.start(), *APPLE_RADIUS_RANGE.end());
        self.profile.hud.apples_tunnels = style;
        self.refresh_preview();
    }

    pub fn set_minimal_minimap(&mut self, on: bool) {
        self.profile.hud.minimap.minimal = on;
        self.refresh_preview();
    }

    /// Health bar edits are made on a copy and stored whole.
    pub fn set_health_style(&mut self, mut style: HealthStyle) {
        style.number_scale_pct = style.number_scale_pct.clamp(
            *dt_core::hud::health_style::NUMBER_SCALE_RANGE.start(),
            *dt_core::hud::health_style::NUMBER_SCALE_RANGE.end(),
        );
        self.profile.hud.health = style;
        self.refresh_preview();
    }

    pub fn health_changed_count(&self) -> usize {
        self.profile.hud.health.changed_count()
    }

    /// Player stats edits are made on a copy and stored whole; out-of-range values are
    /// pulled back in.
    pub fn set_player_stats_style(&mut self, mut style: PlayerStatsStyle) {
        use dt_core::hud::player_stats as ps;
        let parts: Vec<_> = style.parts.iter().map(|(&k, &v)| (k, v)).collect();
        for (part, mut edit) in parts {
            edit.offset_x = edit
                .offset_x
                .clamp(*ps::OFFSET_RANGE.start(), *ps::OFFSET_RANGE.end());
            edit.offset_y = edit
                .offset_y
                .clamp(*ps::OFFSET_RANGE.start(), *ps::OFFSET_RANGE.end());
            edit.scale_pct = edit
                .scale()
                .clamp(*ps::SCALE_RANGE.start(), *ps::SCALE_RANGE.end());
            edit.opacity_pct = edit.opacity_pct.map(|o| o.min(100));
            style.set_part(part, edit);
        }
        let clamp = |v: u8, r: &std::ops::RangeInclusive<u8>| v.clamp(*r.start(), *r.end());
        style.number_px = clamp(style.number_px, &ps::NUMBER_PX_RANGE);
        style.level_px = clamp(style.level_px, &ps::NUMBER_PX_RANGE);
        style.souls_px = clamp(style.souls_px, &ps::SOULS_PX_RANGE);
        style.tile_gap_px = clamp(style.tile_gap_px, &ps::TILE_GAP_RANGE);
        for pct in [
            &mut style.icon_opacity_pct,
            &mut style.empty_opacity_pct,
            &mut style.cooldown_pct,
        ] {
            *pct = (*pct).min(100);
        }
        self.profile.hud.player_stats = style;
        self.refresh_preview();
    }

    pub fn player_stats_changed_count(&self) -> usize {
        self.profile.hud.player_stats.changed_count()
    }

    /// Replaces the in-game settings rows; names that are not rows of the DeadTune group
    /// are dropped.
    pub fn set_ingame(&mut self, mut settings: IngameSettings) {
        settings
            .performance
            .retain(|name| ingame::perf_row(name).is_some());
        self.profile.hud.ingame = settings;
        self.refresh_preview();
    }

    pub fn ingame_changed_count(&self) -> usize {
        self.profile.hud.ingame.changed_count()
    }

    /// Carries a Wide FOV set on the in-game slider into gameinfo.gi and the profile, so
    /// the Overview's Wide view shows it. Once per saved value; nothing most of the time.
    pub fn sync_ingame(&mut self) -> Option<Status> {
        let sync = match ingame::sync_wide_fov(&self.paths, &self.store) {
            Ok(Some(sync)) => sync,
            Ok(None) => return None,
            Err(e) => return Some(Status::Error(format!("in-game Wide FOV: {e}"))),
        };
        if sync.gameinfo_changed {
            match LiveFiles::read(&self.paths) {
                Ok(live) => {
                    self.live = live;
                    self.known_gameinfo_sha = sha256_hex(self.live.gameinfo.as_bytes());
                }
                Err(e) => return Some(Status::Error(format!("in-game Wide FOV: {e}"))),
            }
        }
        let value = ingame::wide_fov_text(sync.ratio);
        let was_saved = self.saved.is_some();
        if let Err(e) = self.set_convar(ingame::WIDE_FOV_CONVAR, value.clone()) {
            return Some(Status::Error(format!("in-game Wide FOV: {e}")));
        }
        if was_saved && let Err(e) = self.save_profile() {
            return Some(Status::Error(format!("in-game Wide FOV: {e}")));
        }
        let shown = match crate::friendly::row(ingame::WIDE_FOV_CONVAR) {
            Some(row) => crate::friendly::display(row.control, &value),
            None => value,
        };
        Some(Status::Info(format!(
            "Wide view set to {shown} from the in-game slider"
        )))
    }

    pub fn reset_minimap_style(&mut self) {
        self.profile.hud.minimap = MinimapStyle::default();
        self.refresh_preview();
    }

    /// Stores `color` for `id`; the vanilla colour itself is not stored, like identity layout edits.
    pub fn set_minimap_color(&mut self, id: IconId, color: Color) {
        let vanilla = minimap_colors::spec(id)
            .vanilla
            .and_then(|v| v.parse::<Color>().ok());
        if vanilla == Some(color) {
            self.profile.hud.minimap_colors.remove(&id);
        } else {
            self.profile.hud.minimap_colors.insert(id, color);
        }
        self.refresh_preview();
    }

    pub fn reset_minimap_color(&mut self, id: IconId) {
        self.profile.hud.minimap_colors.remove(&id);
        self.refresh_preview();
    }

    /// Replaces every minimap colour; `None` is Reset all.
    pub fn apply_minimap_preset(&mut self, preset: Option<MinimapPreset>) {
        self.profile.hud.minimap_colors = preset.map(MinimapPreset::colors).unwrap_or_default();
        self.refresh_preview();
    }

    pub fn minimap_preset(&self) -> Option<MinimapPreset> {
        MinimapPreset::ALL
            .into_iter()
            .find(|p| p.colors() == self.profile.hud.minimap_colors)
    }

    /// The enemy colour the game would use, from the profile or the catalog default.
    pub fn enemy_ui_color(&self) -> [u8; 3] {
        ENEMY_UI_COLOR.map(|name| {
            self.current_value(name)
                .and_then(|v| v.trim().parse::<f64>().ok())
                .map_or(0, |v| v.clamp(0.0, 255.0).round() as u8)
        })
    }

    pub fn set_enemy_ui_color(&mut self, rgb: [u8; 3]) {
        let current = self.enemy_ui_color();
        for ((name, value), old) in ENEMY_UI_COLOR.iter().zip(rgb).zip(current) {
            if value != old {
                self.set_convar(name, value.to_string())
                    .expect("enemy colour ConVars are not denylisted");
            }
        }
    }

    pub fn custom_ui_colors(&self) -> bool {
        self.current_value(CUSTOM_UI_COLORS)
            .is_some_and(|v| parse_bool(&v))
    }

    pub fn set_custom_ui_colors(&mut self, on: bool) {
        let like = self.current_value(CUSTOM_UI_COLORS);
        self.set_convar(CUSTOM_UI_COLORS, bool_text(on, like.as_deref()))
            .expect("citadel_custom_ui_colors is not denylisted");
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
        self.commit_edits();
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
        if let Some(receipt) = applied.report.receipt.clone() {
            self.ack.start(receipt, Instant::now());
        }
        if applied.report.needs_restart {
            let mut names: Vec<String> = plan
                .restart
                .iter()
                .chain(&plan.queued_cheat)
                .chain(&plan.sections)
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
        if let Err(e) = self.write_boot_cfg() {
            applied.warning = Some(format!("boot cfg not updated: {e}"));
        }
        Ok(applied)
    }

    /// Sends what is due from slider drags.
    pub fn tick_live(&mut self, now: Instant) -> Option<Result<PushOutcome, String>> {
        let cmds = self.live_push.take_due(now)?;
        Some(self.send(&cmds))
    }

    /// Pushes through the bridge and starts following the reply in the console log.
    fn send(&mut self, cmds: &[ConsoleCmd]) -> Result<PushOutcome, String> {
        let outcome = self.bridge_target().push(cmds).map_err(|e| e.to_string())?;
        if let PushOutcome::Sent { receipt, .. } = &outcome {
            self.ack.start(receipt.clone(), Instant::now());
        }
        Ok(outcome)
    }

    /// A harmless batch (it only asks the game for `fps_max`) to prove the live path works.
    pub fn send_test(&mut self) -> Result<(), String> {
        let receipt = self
            .bridge_target()
            .probe("fps_max")
            .map_err(|e| e.to_string())?
            .ok_or("The clipboard bridge has no reply to read; pick Exec file or Netcon.")?;
        self.ack.start(receipt, Instant::now());
        Ok(())
    }

    /// Reads new console log lines into the ack tracker, at most four times a second.
    pub fn poll_conlog(&mut self, now: Instant) {
        if self
            .conlog_polled
            .is_some_and(|last| now.duration_since(last) < CONLOG_POLL)
        {
            return;
        }
        self.conlog_polled = Some(now);
        for line in self.conlog.poll() {
            self.ack.observe_line(&line);
            self.guard_lines.push(line);
        }
        // Drained every game poll; bounded in case none comes.
        if self.guard_lines.len() > 10_000 {
            let excess = self.guard_lines.len() - 10_000;
            self.guard_lines.drain(..excess);
        }
        self.ack.tick(now);
        if matches!(self.ack.status(), PushStatus::Confirmed { .. }) {
            self.settings.live_verified = true;
        }
    }

    /// Live-class convars the profile sets, for the boot cfg; none in ranked-safe mode.
    fn boot_cfg(&self) -> BootCfg {
        let live = match self.settings.source {
            TargetSource::RankedSafe => Vec::new(),
            TargetSource::Profile => self
                .profile
                .convars
                .set
                .iter()
                .filter(|(name, _)| {
                    self.catalog.apply_class(name) == ApplyClass::Live
                        && !self.catalog.is_denied(name)
                })
                .map(|(name, value)| ConsoleCmd {
                    name: name.clone(),
                    value: value.clone(),
                })
                .collect(),
        };
        BootCfg {
            bind_key: self.settings.bind_key.clone(),
            live,
            version: env!("CARGO_PKG_VERSION").into(),
        }
    }

    pub fn write_boot_cfg(&self) -> Result<(), String> {
        self.boot_cfg()
            .write(&self.paths.cfg_dir)
            .map_err(|e| e.to_string())
    }

    pub fn launch_args(&self) -> LaunchArgs {
        launch::with_boot(&self.settings.launch.args(), self.settings.console_window)
    }

    /// What someone who starts the game from Steam pastes into its Launch Options box.
    pub fn steam_launch_options(&self) -> String {
        launch::command_line(&self.launch_args().args)
    }

    /// Writes the boot cfg and starts the game through Steam with `+exec deadtune_boot`.
    pub fn launch_game(&mut self) -> Result<(), String> {
        self.write_boot_cfg()?;
        launch::launch(&self.launch_args()).map_err(|e| e.to_string())
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
        self.send(&cmds)
    }

    pub fn profiles_dir(&self) -> PathBuf {
        profiles::dir(&self.data_dir)
    }

    /// What every Apply button runs: write, then keep what was written as the saved
    /// profile, so Discard afterwards goes back to it and not to an older save.
    pub fn apply_and_save(&mut self) -> Result<Applied, String> {
        let applied = self.apply()?;
        self.save_profile()
            .map_err(|e| format!("Applied, but saving your profile failed: {e}"))?;
        Ok(applied)
    }

    pub fn save_profile(&mut self) -> io::Result<PathBuf> {
        let path = profiles::save(&self.profiles_dir(), &self.profile)?;
        self.saved = Some(self.profile.clone());
        self.settings.last_profile = Some(self.profile.name.clone());
        Ok(path)
    }

    /// The power-source profile switch would turn Ranked-safe off and bring practice mode
    /// back, so it waits while Ranked-safe is on.
    pub fn ranked_safe_blocks_auto_profile(&self) -> bool {
        self.settings.source == TargetSource::RankedSafe
    }

    pub fn switch_profile(&mut self, profile: Profile, on_disk: bool) {
        self.saved = on_disk.then(|| profile.clone());
        if on_disk {
            self.settings.last_profile = Some(profile.name.clone());
        }
        self.profile = profile;
        self.base = Base::resolve(&self.profile, &self.presets_dir());
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

fn trial_started_message(changed: &[Pak]) -> String {
    format!(
        "Testing on this launch: {}. If Deadlock fails to start, DeadTune turns them off.",
        guard::names(changed)
    )
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
    use std::time::Duration;

    use super::testutil::state;
    use super::*;
    use dt_core::bridge::ack::Outcome;

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
    fn field_of_view_sets_live_and_resets_to_the_preset() {
        let fov = "citadel_camera_hero_fov";
        let (_dir, mut state) = state();
        let row = crate::friendly::row(fov).unwrap();
        assert_eq!(state.preset_value(fov).as_deref(), Some("90"));
        let typed = crate::friendly::parse(row.control, "80").unwrap();
        state.set_convar(fov, fmt_num(typed, true)).unwrap();
        assert!(state.is_changed(fov));
        assert_eq!(state.current_value(fov).as_deref(), Some("80"));
        assert!(
            plan(&state)
                .live
                .iter()
                .any(|c| c.name == fov && c.value == "80"),
            "{:?}",
            plan(&state)
        );
        state.reset_convars([fov]);
        assert!(!state.is_changed(fov));
        assert_eq!(state.current_value(fov).as_deref(), Some("90"));
        assert!(plan(&state).is_empty());
    }

    #[test]
    fn wide_view_is_automatic_unless_set_and_applies_on_restart() {
        let wide = "r_aspectratio";
        let (_dir, mut state) = state();
        let row = crate::friendly::row(wide).unwrap();
        let shown =
            |s: &AppState| crate::friendly::display(row.control, &s.current_value(wide).unwrap());
        assert_eq!(shown(&state), "Automatic");
        let typed = crate::friendly::parse(row.control, "100").unwrap();
        state.set_convar(wide, fmt_num(typed, false)).unwrap();
        assert_eq!(state.current_value(wide).as_deref(), Some("2.49"));
        assert!(plan(&state).restart.contains(&wide.to_string()));
        state.reset_convars([wide]);
        assert_eq!(shown(&state), "Automatic");
    }

    #[test]
    fn preset_camera_values_are_shown_as_is_not_tweaked() {
        let (_dir, mut state) = state();
        state.set_base(BaseRef::Preset(PresetId::OptilockRecommended));
        assert_eq!(
            state.current_value("citadel_camera_hero_fov").as_deref(),
            Some("100"),
            "the game clamps it to 90; DeadTune keeps the preset's value"
        );
        assert_eq!(
            state.current_value("r_aspectratio").as_deref(),
            Some("2.15")
        );
        assert_eq!(
            state.changed_count(crate::friendly::CAMERA.iter().copied()),
            0
        );
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
    fn practice_mode_lands_in_the_files_and_ranked_safe_takes_it_out() {
        let (_dir, mut state) = state();
        assert!(state.live.practice.is_off());
        let fog = PracticeMode {
            fog: true,
            ..PracticeMode::default()
        };
        state.set_practice(fog);
        assert_eq!(state.profile.practice, fog);
        assert_eq!(state.pending(), Pending::Other);
        assert_eq!(plan(&state).sections.len(), 3);
        assert_eq!(state.timing(), Timing::NextLaunch);
        assert!(state.live.practice.is_off(), "not written before Apply");

        let applied = state.apply().unwrap();
        assert!(applied.report.wrote_gameinfo && applied.report.needs_restart);
        assert_eq!(state.live.practice, fog);
        assert_eq!(
            practice::detect(&std::fs::read_to_string(&state.paths.gameinfo).unwrap()).unwrap(),
            fog
        );
        assert!(plan(&state).is_empty());
        let names = &state.pending_restart.as_ref().unwrap().names;
        assert!(names.contains(&"SceneSystem/VolumetricFog".to_string()));

        let record = practice::Record::load(&state.store.root).unwrap();
        assert_eq!(record.prior("VolumetricFog"), Some(Some("1")));

        state.toggle_ranked_safe().unwrap();
        assert!(state.live.practice.is_off(), "ranked-safe restored stock");
        assert_eq!(state.profile.practice, fog, "the profile remembers it");
        assert_eq!(
            practice::Record::load(&state.store.root).unwrap(),
            record,
            "ranked-safe leaves the record"
        );
        state.toggle_ranked_safe().unwrap();
        assert_eq!(state.live.practice, fog);

        let stock = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../research/configs/OptimizationLock/clean gameinfo.gi/gameinfo.gi"
        ))
        .unwrap();
        state.set_practice(PracticeMode::default());
        state.apply().unwrap();
        assert_eq!(state.live.gameinfo, stock, "off restored the prior values");
        assert!(!state.store.root.join(practice::RECORD_FILE).exists());

        let theirs = stock.replacen(
            "VolumetricFog                     \"1\"",
            "VolumetricFog                     \"0\"",
            1,
        );
        std::fs::write(&state.paths.gameinfo, &theirs).unwrap();
        state.live = LiveFiles::read(&state.paths).unwrap();
        state.known_gameinfo_sha = sha256_hex(state.live.gameinfo.as_bytes());
        state.set_convar(LIVE, "90".into()).unwrap();
        state.apply().unwrap();
        assert!(
            state
                .live
                .gameinfo
                .contains("VolumetricFog                     \"0\""),
            "a value DeadTune never wrote stays"
        );
        assert!(state.live.practice.is_off(), "one key is not the fog group");

        state.set_practice(PracticeMode::default());
        state.apply().unwrap();
        assert!(state.live.practice.is_off());
        assert!(!state.profile.to_toml().unwrap().contains("practice"));
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
        let PushOutcome::Sent { receipt, .. } = outcome else {
            panic!("{outcome:?}");
        };
        assert_eq!(receipt.queries, [LIVE]);
        let cfg = std::fs::read_to_string(state.paths.cfg_dir.join("deadtune_live.cfg")).unwrap();
        assert!(cfg.contains(r#"fps_max "120""#));
        assert!(state.ack.is_waiting(), "the push is followed in the log");
    }

    /// Plays the game's part: appends console output to the log DeadTune tails.
    fn game_prints(state: &AppState, text: &str) {
        use std::io::Write;
        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(state.paths.citadel_dir.join("console.log"))
            .unwrap();
        log.write_all(text.as_bytes()).unwrap();
    }

    #[test]
    fn push_is_confirmed_from_the_console_log_end_to_end() {
        let (_dir, mut state) = state();
        state.observe_game(true, None);
        game_prints(&state, "DEADTUNE_BOOT 0.0.1\r\n");
        state.set_convar(LIVE, "120".into()).unwrap();
        state.set_convar(CHEAT, "6000".into()).unwrap();
        let t0 = Instant::now() + crate::live::DEBOUNCE;
        state.tick_live(t0).unwrap().unwrap();
        state.poll_conlog(t0);
        assert!(state.ack.is_waiting());
        assert_eq!(
            state.ack.boot.as_ref().map(|b| b.version.as_str()),
            Some("0.0.1")
        );
        assert!(!state.settings.live_verified);
        let nonce = match state.ack.status() {
            PushStatus::Waiting { .. } => {
                let cfg =
                    std::fs::read_to_string(state.paths.cfg_dir.join("deadtune_live.cfg")).unwrap();
                cfg.lines()
                    .find_map(|l| l.strip_prefix("echo DEADTUNE_ACK "))
                    .map(|rest| rest.split(' ').next().unwrap().to_string())
                    .unwrap()
            }
            other => panic!("{other:?}"),
        };
        game_prints(
            &state,
            &format!(
                "DEADTUNE_ACK {nonce} 1\r\n\"fps_max\" = \"120\" ( def. \"400\" )\r\n - Frame rate limiter\r\nDEADTUNE_END {nonce}\r\n"
            ),
        );
        state.poll_conlog(t0 + Duration::from_secs(1));
        let PushStatus::Confirmed { results, .. } = state.ack.status() else {
            panic!("{:?}", state.ack.status());
        };
        assert_eq!(
            results,
            &[(LIVE.to_string(), Outcome::Applied("120".into()))],
            "cheat convar is queued, so only the live one was sent"
        );
        assert!(state.settings.live_verified);
    }

    #[test]
    fn push_times_out_when_the_game_never_answers() {
        let (_dir, mut state) = state();
        state.observe_game(true, None);
        state.set_convar(LIVE, "120".into()).unwrap();
        let t0 = Instant::now() + crate::live::DEBOUNCE;
        state.tick_live(t0).unwrap().unwrap();
        let timeout = dt_core::bridge::ack::TIMEOUT;
        state.poll_conlog(t0 + timeout - Duration::from_secs(1));
        assert!(state.ack.is_waiting());
        state.poll_conlog(t0 + timeout + Duration::from_secs(1));
        assert!(
            matches!(state.ack.status(), PushStatus::TimedOut { count: 1, .. }),
            "{:?}",
            state.ack.status()
        );
        assert!(!state.settings.live_verified);
    }

    #[test]
    fn send_test_probes_without_changing_anything() {
        let (_dir, mut state) = state();
        state.send_test().unwrap();
        assert!(state.ack.is_waiting());
        let cfg = std::fs::read_to_string(state.paths.cfg_dir.join("deadtune_live.cfg")).unwrap();
        assert!(
            cfg.contains("\nfps_max\n") && !cfg.contains("fps_max \""),
            "{cfg}"
        );
        state.settings.bridge = crate::live::BridgeKind::Clipboard;
        assert!(state.send_test().unwrap_err().contains("clipboard"));
    }

    #[test]
    fn boot_cfg_carries_the_bind_and_the_live_convars_unless_ranked_safe() {
        let (_dir, mut state) = state();
        state.settings.bind_key = "F9".into();
        state.set_convar(LIVE, "144".into()).unwrap();
        state.set_convar(RESTART, "true".into()).unwrap();
        state.write_boot_cfg().unwrap();
        let cfg = std::fs::read_to_string(state.paths.cfg_dir.join("deadtune_boot.cfg")).unwrap();
        assert!(cfg.contains(r#"bind F9 "exec deadtune_live""#), "{cfg}");
        assert!(cfg.contains(r#"fps_max "144""#), "{cfg}");
        assert!(!cfg.contains(RESTART), "restart class stays out: {cfg}");
        assert!(cfg.contains("echo DEADTUNE_BOOT "), "{cfg}");
        state.settings.source = TargetSource::RankedSafe;
        state.write_boot_cfg().unwrap();
        let cfg = std::fs::read_to_string(state.paths.cfg_dir.join("deadtune_boot.cfg")).unwrap();
        assert!(!cfg.contains("fps_max"), "{cfg}");
        assert_eq!(
            state.launch_args().args,
            ["-novid", "+exec", "deadtune_boot", "-condebug"]
        );
        state.settings.launch.renderer = dt_core::launch_options::Renderer::Vulkan;
        state.settings.launch.extra = vec!["-high".into()];
        state.settings.console_window = true;
        assert_eq!(
            state.steam_launch_options(),
            "-vulkan -novid -high +exec deadtune_boot -condebug -console"
        );
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
                ignored: 0,
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
        let ignored = ApplyPlan {
            ignored: vec!["r_shadows".into()],
            ..ApplyPlan::default()
        };
        let ignored = PlanSummary::of(&ignored, false);
        assert_eq!((ignored.ignored, ignored.next_launch), (1, 0));
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
    fn remove_addon_now_switches_the_addon_off_even_when_nothing_is_installed() {
        let (_dir, mut state) = state();
        state.set_addon_enabled(AddonId::TextureDownscaler, true);
        assert_eq!(
            state.remove_addon_now(AddonId::TextureDownscaler),
            Ok(false)
        );
        assert!(!state.profile.addons.is_enabled(AddonId::TextureDownscaler));
    }

    #[test]
    fn checks_run_in_the_background_and_land_on_poll() {
        let (_dir, mut state) = state();
        state.run_checks();
        assert!(state.checks_running());
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        while state.checks_running() && Instant::now() < deadline {
            state.poll_checks();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let checks = state.checks.as_ref().expect("checks arrived");
        assert!(!checks.is_empty());
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
        state.apply_hud_preset(HudPreset::Vanilla);
        assert_eq!(state.hud_preset(), Some(HudPreset::Vanilla));
        assert!(!state.is_dirty());
    }

    #[test]
    fn ingame_rows_drop_unknown_names_count_and_survive_layout_presets() {
        let (_dir, mut state) = state();
        state.set_ingame(IngameSettings {
            wide_fov: true,
            performance: ["r_citadel_shadow_quality", "fps_max"]
                .map(String::from)
                .into(),
        });
        let stored = state.profile.hud.ingame.clone();
        assert_eq!(
            stored.performance.iter().collect::<Vec<_>>(),
            ["r_citadel_shadow_quality"]
        );
        assert_eq!(state.ingame_changed_count(), 2);
        assert!(state.is_dirty());
        state.apply_hud_preset(HudPreset::Competitive);
        assert_eq!(state.profile.hud.ingame, stored);
        assert_eq!(state.hud_preset(), Some(HudPreset::Competitive));
        state.set_ingame(IngameSettings::default());
        state.apply_hud_preset(HudPreset::Vanilla);
        assert!(!state.is_dirty());
    }

    #[test]
    fn ingame_wide_fov_sync_lands_in_gameinfo_and_the_profile_once() {
        let (_dir, mut state) = state();
        assert_eq!(state.sync_ingame(), None, "nothing saved in game yet");
        std::fs::write(
            state.paths.cfg_dir.join("user_convars_0_slot0.vcfg"),
            format!(
                "\"config\" {{ \"convars\" {{ \"{}\" \"{}\" }} }}",
                ingame::STASH_CONVAR,
                ingame::stash_encode(2.49)
            ),
        )
        .unwrap();
        let status = state.sync_ingame().expect("synced");
        assert!(
            matches!(&status, Status::Info(s) if s.starts_with("Wide view set to ") && s.contains("100")),
            "{status:?}"
        );
        assert_eq!(
            state.current_value("r_aspectratio").as_deref(),
            Some("2.49")
        );
        assert!(
            state.live.gameinfo.contains("r_aspectratio"),
            "written to gameinfo.gi"
        );
        assert_eq!(
            state.known_gameinfo_sha,
            sha256_hex(state.live.gameinfo.as_bytes())
        );
        assert!(
            plan(&state).restart.is_empty() && !plan(&state).is_empty() || plan(&state).is_empty(),
            "the file already holds the value: {:?}",
            plan(&state)
        );
        assert!(
            !state.is_dirty(),
            "the profile was on disk, so the synced value is saved with it"
        );
        assert_eq!(state.sync_ingame(), None, "same value again: nothing to do");
    }

    #[test]
    fn apples_and_tunnels_clamp_count_and_survive_layout_presets() {
        let (_dir, mut state) = state();
        let mut style = ApplesTunnels::default();
        style.apples.on = true;
        style.apples.size_px = 40;
        style.tunnel_radius_pct = 1;
        style.apple_radius_pct = 90;
        state.set_apples_tunnels(style);
        let stored = state.profile.hud.apples_tunnels;
        assert_eq!(stored.apples.size_px, 16);
        assert_eq!(stored.tunnel_radius_pct, 5);
        assert_eq!(stored.apple_radius_pct, 40);
        assert_eq!(state.minimap_changed_count(), 1);
        assert!(state.is_dirty());
        state.apply_hud_preset(HudPreset::Competitive);
        assert_eq!(state.profile.hud.apples_tunnels, stored);
        assert_eq!(state.hud_preset(), Some(HudPreset::Competitive));
        state.set_apples_tunnels(ApplesTunnels::default());
        state.apply_hud_preset(HudPreset::Vanilla);
        assert!(!state.is_dirty());
    }

    #[test]
    fn minimap_colors_edit_the_profile_hud_and_presets_round_trip() {
        let (_dir, mut state) = state();
        let blue = Color([0, 0x72, 0xB2, 255]);
        state.set_minimap_color(IconId::EnemyHero, blue);
        assert_eq!(state.profile.hud.minimap_colors[&IconId::EnemyHero], blue);
        assert!(state.is_dirty());
        state.set_minimap_color(IconId::EnemyHero, "#FF410D".parse().unwrap());
        assert!(
            state.profile.hud.minimap_colors.is_empty(),
            "the vanilla colour is not stored"
        );
        assert!(!state.is_dirty());

        state.set_minimap_color(IconId::Shop, blue);
        state.reset_minimap_color(IconId::Shop);
        assert!(state.profile.hud.minimap_colors.is_empty());

        assert_eq!(state.minimap_preset(), None);
        for preset in MinimapPreset::ALL {
            state.apply_minimap_preset(Some(preset));
            assert_eq!(state.minimap_preset(), Some(preset));
            let patch = dt_core::hud::layout::compile(&state.profile.hud).unwrap();
            let css = &patch.styles[minimap_colors::MINIMAP_STYLE];
            assert!(css.contains(".player.enemy #BackgroundImage"), "{css}");
            assert!(
                !state
                    .profile
                    .hud
                    .minimap_colors
                    .contains_key(&IconId::AllyObjective),
                "{preset:?} keeps the lane colours"
            );
        }
        state.apply_hud_preset(HudPreset::Clean);
        assert_eq!(state.hud_preset(), Some(HudPreset::Clean));
        assert_eq!(
            state.minimap_preset(),
            Some(MinimapPreset::HighContrast),
            "a layout preset keeps the colours"
        );
        state.apply_hud_preset(HudPreset::Vanilla);
        let cb = MinimapPreset::ColorBlind.colors();
        assert_ne!(cb[&IconId::AllyHero], cb[&IconId::EnemyHero]);
        state.apply_minimap_preset(None);
        assert!(state.profile.hud.minimap_colors.is_empty());
        assert!(!state.is_dirty());
    }

    #[test]
    fn minimap_changes_count_icons_and_enemy_colour_settings() {
        let (_dir, mut state) = state();
        assert_eq!(state.minimap_changed_count(), 0);
        state.set_minimap_color(IconId::Shop, Color([0, 0x72, 0xB2, 255]));
        state.set_enemy_ui_color([0, 50, 50]);
        state.set_custom_ui_colors(true);
        assert_eq!(state.minimap_changed_count(), 3);
        state.set_hud_element(
            ElementId::Minimap,
            ElementEdit {
                offset_x: 10,
                ..Default::default()
            },
        );
        assert_eq!(
            state.minimap_changed_count(),
            3,
            "layout edits belong to the HUD page"
        );
    }

    #[test]
    fn minimap_style_edits_survive_layout_presets_and_reset() {
        let (_dir, mut state) = state();
        state.set_marker_scale(MarkerGroup::EnemyHeroes, 150);
        state.set_marker_scale(MarkerGroup::Shops, 999);
        state.set_map_opacity(0);
        state.set_minimal_minimap(true);
        let style = &state.profile.hud.minimap;
        assert_eq!(style.scale(MarkerGroup::Shops), 200, "clamped");
        assert_eq!(style.map_opacity_pct, 20, "clamped");
        assert_eq!(state.minimap_changed_count(), 4);
        assert!(state.is_dirty());

        state.apply_hud_preset(HudPreset::Clean);
        assert_eq!(state.hud_preset(), Some(HudPreset::Clean));
        assert_eq!(
            state.minimap_changed_count(),
            4,
            "a layout preset keeps them"
        );
        assert_eq!(
            state.hud_changed_count(),
            HudPreset::Clean.layout().elements.len()
        );
        let patch = dt_core::hud::layout::compile(&state.profile.hud).unwrap();
        assert!(
            patch.styles[minimap_colors::MINIMAP_STYLE]
                .contains("player.enemy{pre-transform-scale2d:1.5;}")
        );

        state.set_marker_scale(MarkerGroup::EnemyHeroes, 100);
        assert!(
            !state
                .profile
                .hud
                .minimap
                .marker_scale_pct
                .contains_key(&MarkerGroup::EnemyHeroes)
        );
        state.reset_minimap_style();
        state.apply_hud_preset(HudPreset::Vanilla);
        assert_eq!(state.minimap_changed_count(), 0);
        assert!(!state.is_dirty());
    }

    #[test]
    fn health_style_survives_layout_presets_and_compiles() {
        use dt_core::hud::health_style::{HEALTH_CONTAINER_STYLE, HealthPreset};
        let (_dir, mut state) = state();
        state.set_health_style(HealthStyle {
            number_scale_pct: 999,
            ..HealthPreset::BigNumber.style()
        });
        assert_eq!(state.profile.hud.health.number_scale_pct, 200, "clamped");
        assert!(state.is_dirty());
        state.apply_hud_preset(HudPreset::Competitive);
        assert_eq!(state.hud_preset(), Some(HudPreset::Competitive));
        assert_eq!(state.health_changed_count(), 5, "a layout preset keeps it");
        let patch = dt_core::hud::layout::compile(&state.profile.hud).unwrap();
        assert!(patch.styles[HEALTH_CONTAINER_STYLE].contains("font-size:64px"));
        state.set_health_style(HealthStyle::default());
        state.apply_hud_preset(HudPreset::Vanilla);
        assert!(!state.is_dirty());
    }

    #[test]
    fn enemy_ui_color_goes_through_live_convar_edits() {
        let (_dir, mut state) = state();
        for name in ENEMY_UI_COLOR.iter().chain([&CUSTOM_UI_COLORS]) {
            assert_eq!(state.catalog.apply_class(name), ApplyClass::Live, "{name}");
        }
        assert_eq!(state.enemy_ui_color(), [215, 50, 50], "catalog default");
        assert!(!state.custom_ui_colors());
        state.set_enemy_ui_color([0, 213, 255]);
        state.set_custom_ui_colors(true);
        assert_eq!(state.enemy_ui_color(), [0, 213, 255]);
        assert!(state.custom_ui_colors());
        let set = &state.profile.convars.set;
        assert_eq!(set["citadel_enemy_ui_color_r"], "0");
        assert_eq!(set["citadel_enemy_ui_color_b"], "255");
        assert_eq!(set[CUSTOM_UI_COLORS], "true");
        assert!(state.is_dirty());
        state.reset_convars(ENEMY_UI_COLOR.iter().copied().chain([CUSTOM_UI_COLORS]));
        assert_eq!(state.enemy_ui_color(), [215, 50, 50]);
        assert!(!state.is_dirty());
    }

    #[test]
    fn apply_and_save_makes_the_applied_profile_the_saved_one() {
        let (_dir, mut state) = state();
        state.set_convar("r_farz", "6000".into()).unwrap();
        state.apply_and_save().unwrap();
        assert_eq!(state.saved.as_ref(), Some(&state.profile));
        state.revert_all();
        assert_eq!(
            state.profile.convars.set.get("r_farz").map(String::as_str),
            Some("6000"),
            "Discard after Apply keeps what was applied"
        );
    }

    #[test]
    fn ranked_safe_holds_back_the_power_profile_switch() {
        let (_dir, mut state) = state();
        assert!(!state.ranked_safe_blocks_auto_profile());
        state.toggle_ranked_safe().unwrap();
        assert!(state.ranked_safe_blocks_auto_profile());
    }

    #[test]
    fn a_convar_edit_keeps_its_video_txt_twin_in_step() {
        let (_dir, mut state) = state();
        let live = state.live.video.clone().unwrap();
        let has = |k: &str| live.contains(&format!("\"setting.{k}\""));
        assert!(
            has("r_citadel_shadow_quality") && has("r_screen_space_shadows"),
            "fixture"
        );
        state
            .set_convar("r_citadel_shadow_quality", "2".into())
            .unwrap();
        state
            .set_convar("r_screen_space_shadows", "1".into())
            .unwrap();
        let video = &state.profile.video;
        assert_eq!(video["setting.r_citadel_shadow_quality"], "2");
        assert_eq!(
            video["setting.r_screen_space_shadows"], "true",
            "the file's own style"
        );
        state.set_convar("r_farz", "6000".into()).unwrap();
        assert!(
            !state.profile.video.contains_key("setting.r_farz"),
            "no twin, nothing written"
        );
        state.reset_convars(["r_citadel_shadow_quality"]);
        assert!(
            !state
                .profile
                .video
                .contains_key("setting.r_citadel_shadow_quality")
        );
        state.reset_to_preset();
        assert!(state.profile.video.is_empty());
    }

    #[test]
    fn sets_convar_covers_profile_and_preset() {
        let (_dir, mut state) = state();
        assert!(!state.sets_convar("definitely_not_a_convar"));
        state.set_convar("r_farz", "6000".into()).unwrap();
        assert!(state.sets_convar("r_farz"));
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
    fn sidelock_needs_an_import_and_a_review_before_it_becomes_the_base() {
        let (dir, mut state) = state();
        let view = |s: &AppState| s.remote_presets[&PresetId::SideLock].clone();
        assert_eq!(view(&state).status.active, None);
        state.set_base(BaseRef::Preset(PresetId::SideLock));
        assert!(
            state
                .base
                .as_ref()
                .unwrap_err()
                .contains("isn't downloaded yet")
        );

        let standin = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../dt-core/tests/fixtures/sidelock_standin/cfg.zip");
        let copy = dir.path().join("cfg.zip");
        std::fs::copy(standin, &copy).unwrap();
        assert!(
            state
                .import_remote(PresetId::SideLock, &dir.path().join("nope.zip"))
                .is_err()
        );
        state.import_remote(PresetId::SideLock, &copy).unwrap();
        let pending = view(&state);
        assert!(
            pending.status.pending.is_some(),
            "the stand-in is not the pinned file"
        );
        assert!(
            pending
                .changes
                .iter()
                .any(|c| c.name == "citadel_damage_indicator_radius")
        );
        assert!(state.base.is_err(), "nothing is used before it is accepted");

        state.discard_remote(PresetId::SideLock).unwrap();
        assert_eq!(view(&state).status.pending, None);
        state.import_remote(PresetId::SideLock, &copy).unwrap();
        state.accept_remote(PresetId::SideLock).unwrap();
        assert_eq!(view(&state).status.active, Some(remote::Active::Accepted));
        assert!(view(&state).changes.is_empty());
        let base = state.base.as_ref().unwrap();
        assert_eq!(base.values["citadel_damage_indicator_radius"], "1");
        assert!(
            plan(&state)
                .denied
                .contains(&"r_citadel_selection_outline2_alpha".to_string()),
            "the denylist still applies to a remote preset"
        );
    }

    #[test]
    fn changed_detection_follows_the_preset() {
        let (_dir, mut state) = state();
        assert_eq!(state.changed_count(["fps_max", RESTART]), 0);
        state.set_convar(LIVE, "144".into()).unwrap();
        assert!(state.is_changed(LIVE));
        assert_eq!(state.preset_value(LIVE).as_deref(), Some("400"));
        // Not Kaiz: its quoted `"fps_max" "400"` near the end overrides its earlier 0.
        state.set_base(BaseRef::Preset(PresetId::BootMaxfps));
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

    fn upstream(dir: &str, file: &str) -> PathBuf {
        PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../research/configs/OptimizationLock/Various Addons Relating to Performance/"
        ))
        .join(dir)
        .join(file)
    }

    /// Imports and installs the soul container, the one addon that needs no game files.
    fn with_soul_container_installed(state: &mut AppState) -> PathBuf {
        state
            .import_addon(&upstream("Optimized Soul Container", "pak01_dir.vpk"))
            .unwrap();
        state.set_addon_enabled(AddonId::SoulContainer, true);
        state.apply().unwrap();
        state.save_profile().unwrap();
        let pak = dt_core::hud::install::addons_dir(&state.paths).join("pak75_dir.vpk");
        assert!(pak.exists());
        pak
    }

    #[test]
    fn a_failed_trial_removes_the_new_addon_switches_it_off_and_raises_the_banner() {
        let (_dir, mut state) = state();
        let pak = with_soul_container_installed(&mut state);
        let launched = SystemTime::now() + Duration::from_secs(5);
        state.observe_game(true, Some(launched));
        assert!(state.guard.trial.is_some(), "a new pak is on trial");
        assert!(matches!(
            state.addon_badge(
                AddonId::SoulContainer,
                state.addon_states().get(&AddonId::SoulContainer)
            ),
            Some(PakState::OnTrial)
        ));
        state.guard_lines.push(
            "FATAL ERROR: Unable to read default keybinding configuration user_keys_default".into(),
        );
        state.observe_game(true, Some(launched));

        assert!(!pak.exists(), "the pak that broke the start is gone");
        assert!(!state.profile.addons.is_enabled(AddonId::SoulContainer));
        assert!(
            !state
                .saved
                .as_ref()
                .unwrap()
                .addons
                .is_enabled(AddonId::SoulContainer)
        );
        let on_disk = profiles::load(&state.profiles_dir(), &state.profile.name).unwrap();
        assert!(
            !on_disk.addons.is_enabled(AddonId::SoulContainer),
            "profile on disk updated"
        );
        let failure = state.guard.failure.clone().expect("banner shows");
        assert_eq!(failure.ids, [Pak::Addon(AddonId::SoulContainer)]);
        assert_eq!(failure.removed, ["pak75_dir.vpk"]);
        assert!(failure.fatal.as_deref().unwrap().starts_with("FATAL ERROR"));
        assert!(matches!(
            state.addon_badge(AddonId::SoulContainer, None),
            Some(PakState::Broke { fatal: Some(_), .. })
        ));
        assert!(matches!(state.status, Some(Status::Error(ref m)) if m.contains("didn't start")));
        assert_eq!(
            Guard::load(&state.store.root).unwrap().failure,
            Some(failure)
        );

        state.dismiss_guard_failure();
        assert!(state.guard.failure.is_none());
        assert!(Guard::load(&state.store.root).unwrap().failure.is_none());
    }

    #[test]
    fn a_passed_trial_marks_the_addon_verified() {
        let (_dir, mut state) = state();
        with_soul_container_installed(&mut state);
        let launched = SystemTime::now() + Duration::from_secs(5);
        state.observe_game(true, Some(launched));
        state.guard_lines.push("DEADTUNE_BOOT 0.3.0".into());
        state.observe_game(true, Some(launched));
        let states = state.addon_states();
        assert!(matches!(
            state.addon_badge(AddonId::SoulContainer, states.get(&AddonId::SoulContainer)),
            Some(PakState::Verified { .. })
        ));
        assert!(state.profile.addons.is_enabled(AddonId::SoulContainer));
        assert!(matches!(state.status, Some(Status::Info(ref m)) if m.contains("started fine")));
    }

    /// A pak01 holding a scope texture shaped like the game's: Tamara's header with the
    /// dims patched, over a radial alpha vignette. 1536 px rather than the game's 2048
    /// keeps the test quick while every offered size still shrinks it.
    fn install_scope_original(state: &AppState) {
        use dt_core::addons::native_scope::TEXTURE;
        use dt_core::hud::resource::Resource;
        use dt_core::hud::vpk::{self, VpkDir};
        use dt_core::texture::Vtex;
        let up = VpkDir::open(&upstream("Vindicta Scope Downscale", "pak89_dir.vpk"))
            .unwrap()
            .read(TEXTURE)
            .unwrap();
        let start = Vtex::parse(&up).unwrap().pixel_start();
        let data_len = Resource::parse(&up)
            .unwrap()
            .block(b"DATA")
            .unwrap()
            .data
            .len();
        let mut original = up[..start].to_vec();
        let dims = start - data_len + 20;
        const SIDE: i32 = 1536;
        let side = (SIDE as u16).to_le_bytes();
        original[dims..dims + 4].copy_from_slice(&[side, side].concat());
        for y in 0..SIDE {
            for x in 0..SIDE {
                let r = f64::from((2 * x - SIDE).pow(2) + (2 * y - SIDE).pow(2)).sqrt()
                    / f64::from(SIDE);
                original.extend_from_slice(&[0, 0, 0, (92.0 + 150.0 * r.min(1.0)) as u8]);
            }
        }
        let files = BTreeMap::from([(TEXTURE.to_string(), original)]);
        std::fs::write(
            state
                .paths
                .citadel_dir
                .join(dt_core::hud::install::GAME_PAK),
            vpk::write(&files),
        )
        .unwrap();
    }

    fn scope_pak_side(state: &AppState) -> Option<u16> {
        use dt_core::hud::vpk::VpkDir;
        let pak = dt_core::hud::install::addons_dir(&state.paths).join("pak74_dir.vpk");
        let bytes = VpkDir::open(&pak)
            .ok()?
            .read(addons::native_scope::TEXTURE)
            .unwrap();
        let v = dt_core::texture::Vtex::parse(&bytes).unwrap();
        assert_eq!(v.width, v.height);
        Some(v.width)
    }

    #[test]
    fn scope_applies_verifies_goes_on_trial_and_removes_at_every_size() {
        let (_dir, mut state) = state();
        install_scope_original(&state);
        state.set_addon_enabled(AddonId::VindictaScope, true);
        let mut launched = SystemTime::now();
        for side in [720u16, 1080, 1440] {
            state.set_scope(addons::ScopeOptions { side });
            assert!(
                matches!(
                    state.addon_action(AddonId::VindictaScope),
                    Some(Action::Write(_))
                ),
                "{side}: {:?}",
                state.addon_action(AddonId::VindictaScope)
            );
            state.apply().unwrap();
            assert_eq!(scope_pak_side(&state), Some(side));
            let reports = addons::verify::verify_installed(&state.paths, &state.store.root);
            assert_eq!(reports.len(), 1);
            assert!(
                reports[0]
                    .result
                    .as_ref()
                    .is_ok_and(|v| v.problems.is_empty()),
                "{side}: {}",
                reports[0]
            );

            launched += Duration::from_secs(60);
            state.observe_game(true, Some(launched));
            let trial = state.guard.trial.clone().expect("the new pak is on trial");
            assert_eq!(
                trial.changed.keys().collect::<Vec<_>>(),
                [&Pak::Addon(AddonId::VindictaScope)],
                "{side}"
            );
            state.guard_lines.push("DEADTUNE_BOOT 0.9.0".into());
            state.observe_game(true, Some(launched));
            assert!(matches!(
                state.guard.verdict(AddonId::VindictaScope),
                Some(Verdict::Verified { .. })
            ));
            state.observe_game(false, None);
        }

        state.set_scope(addons::ScopeOptions { side: 2048 });
        assert_eq!(
            state.addon_action(AddonId::VindictaScope),
            Some(&Action::Remove),
            "a size not below the game's leaves nothing to install"
        );
        state.apply().unwrap();
        assert_eq!(scope_pak_side(&state), None);

        state.set_scope(addons::ScopeOptions::default());
        state.apply().unwrap();
        assert_eq!(scope_pak_side(&state), Some(1080));
        assert_eq!(state.remove_addon_now(AddonId::VindictaScope), Ok(true));
        assert_eq!(scope_pak_side(&state), None);
        assert!(!state.profile.addons.is_enabled(AddonId::VindictaScope));
    }

    #[test]
    fn injected_failure_drives_the_banner_without_a_game() {
        let (_dir, mut state) = state();
        state.set_addon_enabled(AddonId::BlurDisabler, true);
        state.inject_trial_failure(
            vec![
                AddonId::BlurDisabler.into(),
                AddonId::ParticleDisabler.into(),
            ],
            Some("FATAL ERROR: test".into()),
        );
        let failure = state.guard.failure.as_ref().unwrap();
        assert_eq!(failure.ids.len(), 2);
        assert!(!state.profile.addons.is_enabled(AddonId::BlurDisabler));
        assert!(matches!(
            state.addon_badge(AddonId::ParticleDisabler, None),
            Some(PakState::Broke { .. })
        ));
        state.start_one_at_a_time();
        assert_eq!(state.guard.candidate(), Some(AddonId::BlurDisabler.into()));
        assert!(
            state.profile.addons.is_enabled(AddonId::BlurDisabler),
            "first suspect back on"
        );
        assert!(!state.profile.addons.is_enabled(AddonId::ParticleDisabler));
        state.stop_one_at_a_time();
        assert!(state.guard.sequence.is_none());
    }

    #[test]
    fn safe_mode_removes_every_pak_and_restores_on_the_way_back() {
        let (_dir, mut state) = state();
        let pak = with_soul_container_installed(&mut state);
        state.toggle_safe_mode().unwrap();
        assert!(state.settings.safe_mode);
        assert!(!pak.exists());
        assert!(
            state.profile.addons.is_enabled(AddonId::SoulContainer),
            "remembered"
        );
        assert!(plan(&state).is_empty());
        state.toggle_safe_mode().unwrap();
        assert!(!state.settings.safe_mode);
        assert!(pak.exists());
    }

    #[test]
    fn diagnostic_report_names_the_installed_pak() {
        let (_dir, mut state) = state();
        with_soul_container_installed(&mut state);
        let text = state.diagnostic_report();
        assert!(text.contains("pak75_dir.vpk"), "{text}");
        assert!(
            text.contains("DeadTune: Optimized soul container"),
            "{text}"
        );
        assert!(text.contains("-condebug"), "launch args: {text}");
    }

    fn hud_pak(state: &AppState) -> PathBuf {
        dt_core::hud::install::addons_dir(&state.paths).join(dt_core::hud::install::ADDON_FILE)
    }

    /// A game pak holding the real vanilla HUD stylesheet, so layout edits build.
    fn with_game_hud(state: &AppState) {
        use dt_core::hud::vpk;
        let vanilla = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../dt-core/tests/fixtures/hud/hud_vanilla.vcss_c"
        ))
        .unwrap();
        let files = BTreeMap::from([(dt_core::hud::elements::HUD_STYLE.to_string(), vanilla)]);
        std::fs::write(
            state
                .paths
                .citadel_dir
                .join(dt_core::hud::install::GAME_PAK),
            vpk::write(&files),
        )
        .unwrap();
    }

    fn apply_minimap_opacity(state: &mut AppState, pct: u8) -> Vec<u8> {
        state.set_hud_element(
            ElementId::Minimap,
            ElementEdit {
                opacity_pct: pct,
                ..ElementEdit::default()
            },
        );
        state.apply().unwrap();
        std::fs::read(hud_pak(state)).unwrap()
    }

    fn launch_with(state: &mut AppState, after: u64, line: &str) {
        let launched = SystemTime::now() + Duration::from_secs(after);
        state.observe_game(true, Some(launched));
        state.guard_lines.push(line.into());
        state.observe_game(true, Some(launched));
    }

    const FATAL: &str =
        "FATAL ERROR: Unable to read default keybinding configuration user_keys_default";

    #[test]
    fn a_game_update_rebuilds_the_hud_once_the_game_is_closed() {
        use dt_core::hud::install::{InstalledState, installed_state};
        let (_dir, mut state) = state();
        with_game_hud(&state);
        let acf = state.paths.game_root.join("../../appmanifest_1422450.acf");
        std::fs::write(&acf, "\"AppState\"\n{\n\t\"buildid\"\t\t\"1\"\n}\n").unwrap();
        state.paths.appmanifest = Some(acf.clone());
        apply_minimap_opacity(&mut state, 50);
        std::fs::write(&acf, "\"AppState\"\n{\n\t\"buildid\"\t\t\"2\"\n}\n").unwrap();
        let stale = |state: &AppState| {
            matches!(
                installed_state(&state.paths, &state.store.root).unwrap(),
                InstalledState::Stale(_)
            )
        };
        assert!(stale(&state));

        state.observe_game(true, Some(SystemTime::now() - Duration::from_secs(200)));
        assert!(stale(&state), "left alone while the game runs");
        state.observe_game(false, None);
        assert!(
            matches!(&state.status, Some(Status::Info(m)) if m.contains("rebuilt")),
            "{:?}",
            state.status
        );
        assert!(matches!(
            installed_state(&state.paths, &state.store.root).unwrap(),
            InstalledState::Current(r) if r.build_id.as_deref() == Some("2")
        ));
    }

    #[test]
    fn a_failed_hud_trial_restores_the_last_hud_that_worked_and_keeps_the_settings() {
        let (_dir, mut state) = state();
        with_game_hud(&state);
        let good = apply_minimap_opacity(&mut state, 50);
        assert_eq!(state.hud_trial_state(), Some(PakState::Untried));
        launch_with(&mut state, 5, "DEADTUNE_BOOT 0.12.0");
        assert!(matches!(
            state.hud_trial_state(),
            Some(PakState::Verified { .. })
        ));
        state.observe_game(false, None);

        let bad = apply_minimap_opacity(&mut state, 40);
        assert_ne!(good, bad);
        launch_with(&mut state, 10, FATAL);

        assert_eq!(std::fs::read(hud_pak(&state)).unwrap(), good);
        assert_eq!(
            state.hud_edit(ElementId::Minimap).opacity_pct,
            40,
            "settings kept"
        );
        let failure = state.guard.failure.clone().expect("banner shows");
        assert_eq!(failure.ids, [Pak::Hud]);
        assert_eq!(
            failure.hud.as_ref().map(|h| h.features.clone()),
            Some(vec![dt_core::hud::HudFeature::Layout])
        );
        assert!(
            matches!(state.status, Some(Status::Error(ref m)) if m.contains("put back the last HUD that worked")),
            "{:?}",
            state.status
        );
        assert!(state.hud_held());
        state.observe_game(false, None);
        state.apply().unwrap();
        assert_eq!(
            std::fs::read(hud_pak(&state)).unwrap(),
            good,
            "an unrelated Apply does not put the broken HUD back"
        );

        state.retry_hud();
        assert_eq!(std::fs::read(hud_pak(&state)).unwrap(), bad);
        assert!(!state.hud_held());
        assert_eq!(state.hud_trial_state(), Some(PakState::Untried));
    }

    #[test]
    fn a_failed_hud_trial_with_no_hud_that_worked_turns_the_hud_off() {
        let (_dir, mut state) = state();
        with_game_hud(&state);
        apply_minimap_opacity(&mut state, 50);
        launch_with(&mut state, 5, FATAL);
        assert!(!hud_pak(&state).exists());
        assert_eq!(
            state.hud_edit(ElementId::Minimap).opacity_pct,
            50,
            "settings kept"
        );
        assert!(matches!(
            state.status,
            Some(Status::Error(ref m)) if m == "DeadTune's HUD changes stopped the game from starting, so they were turned off."
        ));
        assert!(state.hud_held());
        assert!(matches!(
            state.hud_trial_state(),
            Some(PakState::Broke { .. })
        ));
    }

    #[test]
    fn a_failed_launch_with_an_addon_and_the_hud_rolls_back_both() {
        let (_dir, mut state) = state();
        with_game_hud(&state);
        let addon = with_soul_container_installed(&mut state);
        apply_minimap_opacity(&mut state, 50);
        launch_with(&mut state, 5, FATAL);
        assert!(!addon.exists());
        assert!(!hud_pak(&state).exists());
        assert!(!state.profile.addons.is_enabled(AddonId::SoulContainer));
        let failure = state.guard.failure.clone().unwrap();
        assert_eq!(failure.ids, [Pak::Addon(AddonId::SoulContainer), Pak::Hud]);
        state.start_one_at_a_time();
        assert_eq!(
            state.guard.candidate(),
            Some(Pak::Addon(AddonId::SoulContainer))
        );
        assert!(addon.exists(), "first suspect back for its test");
        assert!(!hud_pak(&state).exists(), "the HUD waits its turn");
    }

    #[test]
    fn safe_mode_takes_the_hud_out_and_a_launch_in_it_is_no_trial() {
        let (_dir, mut state) = state();
        with_game_hud(&state);
        apply_minimap_opacity(&mut state, 50);
        state.toggle_safe_mode().unwrap();
        assert!(!hud_pak(&state).exists());
        let launched = SystemTime::now() + Duration::from_secs(5);
        state.observe_game(true, Some(launched));
        assert!(state.guard.trial.is_none());
        state.observe_game(false, None);
        state.toggle_safe_mode().unwrap();
        assert!(hud_pak(&state).exists());
        assert_eq!(state.hud_trial_state(), Some(PakState::Untried));
    }
}
