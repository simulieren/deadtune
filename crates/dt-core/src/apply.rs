//! Turns a profile into console commands + file writes, split by apply class.
//!
//! Pipeline: live files + profile -> `Target` (pure) -> `ApplyPlan` (pure) -> `execute` (effects).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::addons::install::{self as addons_install, AddonsPlan};
use crate::addons::{AddonError, AddonsConfig};
use crate::backup::{BackupStore, FileKind, atomic_write};
use crate::bridge::{Bridge, BridgeError, ConsoleCmd, Receipt};
use crate::catalog::{ApplyClass, Catalog};
use crate::gi::{self, GiError, Override, Overrides};
use crate::hud::install::{self as hud_install, HudAction, HudError, HudPlan, InstalledState};
use crate::hud::{HudLayout, searchpaths};
use crate::locate::GamePaths;
use crate::practice;
use crate::preset::remote::{self, RemoteError};
use crate::preset::{self, Source};
use crate::profile::{BaseRef, Profile};
use crate::video::{self, VideoError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileWrite {
    pub path: PathBuf,
    pub before: String,
    pub after: String,
}

impl FileWrite {
    /// Unified diff (via `similar`) for the review pane and `dt-cli diff`.
    pub fn unified_diff(&self) -> String {
        let path = self.path.display().to_string();
        similar::TextDiff::from_lines(&self.before, &self.after)
            .unified_diff()
            .header(&path, &path)
            .to_string()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ApplyContext {
    /// Player says they are in hideout/sandbox, where cheat convars are settable.
    pub in_sandbox: bool,
    pub game_running: bool,
}

/// Base preset texts a profile resolves to (pinned, fetched, or `file:`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseTexts {
    pub gameinfo: String,
    pub video: Option<String>,
}

/// What the files should contain after applying.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub gameinfo: String,
    pub video: Option<String>,
    /// Denylisted convars refused: profile edits dropped, and base preset values put back to
    /// stock. Sorted.
    pub denied: Vec<String>,
    /// The HUD addon to write or remove, from [`hud_plan`].
    pub hud: Option<HudPlan>,
    /// The performance addon paks to write or remove, from [`addons_plan`].
    pub addons: Option<AddonsPlan>,
    /// What the practice record should hold once `gameinfo` is on disk; `None` leaves it.
    pub practice_record: Option<practice::Record>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ApplyPlan {
    /// Sent through the bridge now (also persisted in the files).
    pub live: Vec<ConsoleCmd>,
    /// Cheat-flagged changes held back because `in_sandbox` is false; take effect next launch.
    pub queued_cheat: Vec<String>,
    /// devonly/unknown changes, and removals with no known default; take effect next launch.
    pub restart: Vec<String>,
    /// Changes the engine will not take from gameinfo.gi (`gameinfo_cannot_override`). They are
    /// written but do nothing, now or next launch.
    pub ignored: Vec<String>,
    /// Denylisted convars refused, from the profile or the base preset.
    pub denied: Vec<String>,
    /// `Section/Key` edits outside the ConVars block (practice mode); take effect next launch
    /// and matchmaking may refuse to queue while they differ from stock.
    pub sections: Vec<String>,
    pub gameinfo: Option<FileWrite>,
    pub video: Option<FileWrite>,
    pub video_changes: BTreeMap<String, String>,
    pub hud: Option<HudPlan>,
    pub addons: Option<AddonsPlan>,
    pub practice_record: Option<practice::Record>,
}

impl ApplyPlan {
    pub fn is_empty(&self) -> bool {
        self.gameinfo.is_none()
            && self.video.is_none()
            && self.live.is_empty()
            && self
                .hud
                .as_ref()
                .is_none_or(|h| h.action == HudAction::Nothing)
            && self.addons.as_ref().is_none_or(AddonsPlan::is_empty)
    }

    /// Addon paks written or removed on Apply.
    pub fn addon_changes(&self) -> usize {
        self.addons.as_ref().map_or(0, |a| {
            a.addons
                .iter()
                .filter(|p| {
                    matches!(
                        p.action,
                        crate::addons::Action::Write(_) | crate::addons::Action::Remove
                    )
                })
                .count()
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ApplyReport {
    pub wrote_gameinfo: bool,
    pub wrote_video: bool,
    pub pushed_live: usize,
    pub needs_restart: bool,
    /// Set when the files were written but the bridge push failed. The writes stand, so this
    /// is a report field rather than an `Err`; the live changes take effect next launch.
    pub bridge_error: Option<String>,
    /// What the bridge sent, for the ack tracker to confirm against the console log.
    pub receipt: Option<Receipt>,
    /// The HUD addon was written or removed.
    pub hud_changed: bool,
    /// A performance addon pak was written or removed.
    pub addons_changed: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    #[error("gameinfo.gi: {0}")]
    Gi(#[from] GiError),
    #[error("video.txt: {0}")]
    Video(#[from] VideoError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("bridge: {0}")]
    Bridge(#[from] BridgeError),
    #[error("hud: {0}")]
    Hud(#[from] HudError),
    #[error("addons: {0}")]
    Addons(#[from] AddonError),
    #[error("{0} changed on disk since the plan was made; re-plan before applying")]
    Stale(PathBuf),
    #[error("base file {0}: {1}")]
    Base(PathBuf, std::io::Error),
    #[error("{0}")]
    Remote(#[from] RemoteError),
}

/// Embedded preset text, a remote preset's cached gameinfo.gi under `presets_dir` (see
/// [`preset::cache_dir`]), or the file for `BaseRef::File`. Remote presets and files carry no
/// video.txt. Only the ConVars block of the gameinfo is ever used ([`target`]).
pub fn resolve_base(profile: &Profile, presets_dir: &Path) -> Result<BaseTexts, ApplyError> {
    match &profile.base {
        BaseRef::Preset(id) => match &preset::info(*id).source {
            Source::Pinned(p) => Ok(BaseTexts {
                gameinfo: p.gameinfo.to_string(),
                video: p.video.map(str::to_string),
            }),
            Source::Remote(r) => Ok(BaseTexts {
                gameinfo: remote::read(r, &remote::dir(r, presets_dir))?,
                video: None,
            }),
        },
        BaseRef::File(path) => Ok(BaseTexts {
            gameinfo: fs::read_to_string(path).map_err(|e| ApplyError::Base(path.clone(), e))?,
            video: None,
        }),
    }
}

/// The HUD addon plan for `layout`, or `None` when there is nothing of ours to touch: a vanilla
/// layout with no DeadTune addon installed. That keeps a foreign file at our addon path from
/// blocking a convar-only apply. The install record lives in `store.root`.
pub fn hud_plan(
    paths: &GamePaths,
    layout: &HudLayout,
    store: &BackupStore,
) -> Result<Option<HudPlan>, ApplyError> {
    if layout.is_vanilla()
        && matches!(
            hud_install::installed_state(paths, &store.root)?,
            InstalledState::None | InstalledState::Foreign
        )
    {
        return Ok(None);
    }
    Ok(Some(hud_install::plan(paths, layout, &store.root)?))
}

/// The performance addon plan for `config`, or `None` when there is nothing of ours to touch:
/// nothing enabled and nothing recorded as installed. The record and the download cache live
/// in `store.root`.
pub fn addons_plan(
    paths: &GamePaths,
    config: &AddonsConfig,
    store: &BackupStore,
) -> Result<Option<AddonsPlan>, ApplyError> {
    let plan = addons_install::plan(paths, config, &store.root)?;
    Ok((!plan.addons.is_empty()).then_some(plan))
}

/// What the store contributes to a profile target: the HUD and addon plans and the practice
/// record. `Default` is a bare convar-only target.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Extras {
    pub hud: Option<HudPlan>,
    pub addons: Option<AddonsPlan>,
    pub practice: practice::Record,
}

/// Base ConVars block swapped into the live gameinfo (so SearchPaths edits survive), then the
/// profile's convar overrides minus denylisted names; base video settings + profile video edits.
/// Denylisted convars the base sets away from the vanilla preset go back to the vanilla value,
/// or are commented out when vanilla does not set them: presets carry some of them.
/// When the HUD or addons plan needs it, `Game citadel/addons` is added to SearchPaths.
/// The profile's practice mode is written into SceneSystem last: groups that are on get
/// their values, groups that are off get back what `practice` recorded, and keys DeadTune
/// never wrote stay as they are.
pub fn target(
    live_gameinfo: &str,
    live_video: Option<&str>,
    base: &BaseTexts,
    profile: &Profile,
    catalog: &Catalog,
    extras: Extras,
) -> Result<Target, ApplyError> {
    let Extras {
        hud,
        addons,
        practice,
    } = extras;
    let (denied, allowed): (Overrides, Overrides) = profile
        .overrides()
        .into_iter()
        .partition(|(name, _)| catalog.is_denied(name));
    let swapped = gi::replace_convars_block(live_gameinfo, &base.gameinfo)?;
    let stock = gi::effective_values(preset::vanilla_gameinfo())?;
    let base_denied: Overrides = gi::effective_values(&swapped)?
        .into_iter()
        .filter(|(name, value)| catalog.is_denied(name) && stock.get(name) != Some(value))
        .map(|(name, _)| {
            let back = stock
                .get(&name)
                .map_or(Override::Comment, |v| Override::Set(v.clone()));
            (name, back)
        })
        .collect();
    let refused: BTreeSet<String> = denied
        .into_keys()
        .chain(base_denied.keys().cloned())
        .collect();
    let overrides: Overrides = base_denied.into_iter().chain(allowed).collect();
    let mut gameinfo = gi::apply_overrides(&swapped, &overrides)?.text;
    if hud.as_ref().is_some_and(|h| h.needs_search_path)
        || addons.as_ref().is_some_and(|a| a.needs_search_path)
    {
        gameinfo = searchpaths::ensure_addons(&gameinfo).map_err(HudError::from)?;
    }
    let (gameinfo, practice_record) = practice::plan(&gameinfo, profile.practice, &practice)?;
    let video = live_video
        .map(|live| {
            let swapped = match &base.video {
                Some(base_video) => video::replace_settings(live, base_video)?,
                None => live.to_string(),
            };
            video::apply_settings(&swapped, &profile.video)
        })
        .transpose()?;
    Ok(Target {
        gameinfo,
        video,
        denied: refused.into_iter().collect(),
        hud,
        addons,
        practice_record: Some(practice_record),
    })
}

/// Diffs effective convars of live vs target and classifies each change by apply class.
/// A convar that becomes commented out is pushed live as its catalog default when known.
pub fn plan(
    paths: &GamePaths,
    live_gameinfo: &str,
    live_video: Option<&str>,
    target: &Target,
    catalog: &Catalog,
    ctx: ApplyContext,
) -> Result<ApplyPlan, ApplyError> {
    let live = gi::effective_values(live_gameinfo)?;
    let wanted = gi::effective_values(&target.gameinfo)?;
    let mut plan = ApplyPlan {
        denied: target.denied.clone(),
        sections: practice::drift(&target.gameinfo, live_gameinfo)?
            .iter()
            .map(|d| format!("{}/{}", d.section, d.key))
            .collect(),
        hud: target.hud.clone(),
        addons: target.addons.clone(),
        practice_record: target.practice_record.clone(),
        ..ApplyPlan::default()
    };

    let changed = wanted
        .iter()
        .filter(|(name, value)| live.get(*name) != Some(value))
        .map(|(name, value)| (name, Some(value.clone())));
    let removed = live
        .keys()
        .filter(|name| !wanted.contains_key(*name))
        .map(|name| (name, catalog.get(name).and_then(|e| e.default.clone())));
    for (name, value) in changed.chain(removed) {
        match (value, catalog.apply_class(name)) {
            (Some(value), ApplyClass::Live) => plan.live.push(cmd(name, value)),
            (Some(value), ApplyClass::LiveCheat) if ctx.in_sandbox => {
                plan.live.push(cmd(name, value))
            }
            _ if catalog.is_gameinfo_ignored(name) => plan.ignored.push(name.clone()),
            (Some(_), ApplyClass::LiveCheat) => plan.queued_cheat.push(name.clone()),
            (None, _) | (Some(_), ApplyClass::Restart) => plan.restart.push(name.clone()),
        }
    }

    plan.gameinfo = file_write(&paths.gameinfo, live_gameinfo, &target.gameinfo);
    if let (Some(before), Some(after)) = (live_video, target.video.as_deref()) {
        plan.video = file_write(&paths.video, before, after);
        // Maps, not pair lists: real video.txt files repeat keys, and the last one wins.
        let old: BTreeMap<String, String> = video::read_settings(before)?.into_iter().collect();
        let new: BTreeMap<String, String> = video::read_settings(after)?.into_iter().collect();
        plan.video_changes = new
            .into_iter()
            .filter(|(key, value)| old.get(key) != Some(value))
            .collect();
    }
    Ok(plan)
}

fn cmd(name: &str, value: String) -> ConsoleCmd {
    ConsoleCmd {
        name: name.to_string(),
        value,
    }
}

fn file_write(path: &Path, before: &str, after: &str) -> Option<FileWrite> {
    (before != after).then(|| FileWrite {
        path: path.to_path_buf(),
        before: before.to_string(),
        after: after.to_string(),
    })
}

/// Ranked-safe target: the original (or vanilla preset) ConVars block and stock values for the
/// SceneSystem keys practice mode knows, whoever wrote them; video.txt and the practice
/// record untouched, so leaving ranked-safe returns to the profile's state.
/// Pass `hud_plan(paths, &HudLayout::default(), store)` as `hud` and
/// `addons_plan(paths, &AddonsConfig::default(), store)` as `addons` to remove our addons too;
/// SearchPaths is left as is, so a `Game citadel/addons` line stays (harmless, see `execute`).
pub fn ranked_safe_target(
    live_gameinfo: &str,
    store: &BackupStore,
    hud: Option<HudPlan>,
    addons: Option<AddonsPlan>,
) -> Result<Target, ApplyError> {
    let stock = match store.original(FileKind::GameInfo) {
        Some(entry) => fs::read_to_string(&entry.path)?,
        None => preset::vanilla_gameinfo().to_string(),
    };
    Ok(Target {
        gameinfo: practice::restore_stock(&gi::replace_convars_block(live_gameinfo, &stock)?)?,
        video: None,
        denied: Vec::new(),
        hud,
        addons,
        practice_record: None,
    })
}

/// Checks every write against disk, snapshots originals, backs up, writes atomically, installs
/// or removes the HUD addon, then pushes `live` through the bridge.
///
/// Anything failing before or during the file and addon writes is an `Err`. A bridge failure
/// after them is not: the report says what was written and carries the error in `bridge_error`.
pub fn execute(
    paths: &GamePaths,
    plan: &ApplyPlan,
    store: &BackupStore,
    bridge: Option<&mut dyn Bridge>,
) -> Result<ApplyReport, ApplyError> {
    let writes: Vec<(FileKind, &FileWrite)> = [
        (FileKind::GameInfo, plan.gameinfo.as_ref()),
        (FileKind::Video, plan.video.as_ref()),
    ]
    .into_iter()
    .filter_map(|(kind, write)| write.map(|w| (kind, w)))
    .collect();

    if let Some(write) = &plan.gameinfo {
        gi::validate_braces(&write.after)?;
    }
    for (_, write) in &writes {
        if fs::read_to_string(&write.path)? != write.before {
            return Err(ApplyError::Stale(write.path.clone()));
        }
    }

    store.snapshot_original(FileKind::GameInfo, &paths.gameinfo)?;
    if paths.video.is_file() {
        store.snapshot_original(FileKind::Video, &paths.video)?;
    }
    for (kind, write) in &writes {
        store.backup(*kind, &write.path)?;
        atomic_write(&write.path, write.after.as_bytes())?;
    }
    let mut hud_changed = false;
    if let Some(hud) = &plan.hud {
        hud_install::execute(hud, paths, &store.root)?;
        hud_changed = hud.action != HudAction::Nothing;
    }
    let mut addons_changed = false;
    if let Some(addons) = &plan.addons {
        addons_changed = addons_install::execute(addons, paths, &store.root)?;
    }
    if let Some(record) = &plan.practice_record {
        record.save(&store.root)?;
    }
    // A mounted search path whose directory is missing is a state we cannot vouch for in
    // game, so the addons dir outlives our addons.
    if (plan.hud.is_some() || plan.addons.is_some())
        && searchpaths::has_addons(&fs::read_to_string(&paths.gameinfo)?).unwrap_or(false)
    {
        fs::create_dir_all(hud_install::addons_dir(paths))?;
    }

    let mut report = ApplyReport {
        wrote_gameinfo: plan.gameinfo.is_some(),
        wrote_video: plan.video.is_some(),
        hud_changed,
        addons_changed,
        ..ApplyReport::default()
    };
    if let (Some(bridge), false) = (bridge, plan.live.is_empty()) {
        match bridge.push(&plan.live) {
            Ok(receipt) => {
                report.pushed_live = plan.live.len();
                report.receipt = Some(receipt);
            }
            Err(e) => report.bridge_error = Some(format!("{}: {e}", bridge.name())),
        }
    }
    report.needs_restart = !plan.restart.is_empty()
        || !plan.queued_cheat.is_empty()
        || !plan.sections.is_empty()
        || report.wrote_video
        || report.hud_changed
        || report.addons_changed
        || report.pushed_live < plan.live.len();
    Ok(report)
}
