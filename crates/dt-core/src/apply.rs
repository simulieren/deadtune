//! Turns a profile into console commands + file writes, split by apply class.
//!
//! Pipeline: live files + profile -> `Target` (pure) -> `ApplyPlan` (pure) -> `execute` (effects).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::backup::{BackupStore, FileKind, atomic_write};
use crate::bridge::{Bridge, BridgeError, ConsoleCmd};
use crate::catalog::{ApplyClass, Catalog};
use crate::gi::{self, GiError, Overrides};
use crate::locate::GamePaths;
use crate::preset::{self, PresetId};
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
    /// Profile edits dropped because the convar is on the denylist.
    pub denied: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ApplyPlan {
    /// Sent through the bridge now (also persisted in the files).
    pub live: Vec<ConsoleCmd>,
    /// Cheat-flagged changes held back because `in_sandbox` is false; take effect next launch.
    pub queued_cheat: Vec<String>,
    /// devonly/unknown changes, and removals with no known default; take effect next launch.
    pub restart: Vec<String>,
    /// Profile edits refused because the convar is on the denylist.
    pub denied: Vec<String>,
    pub gameinfo: Option<FileWrite>,
    pub video: Option<FileWrite>,
    pub video_changes: BTreeMap<String, String>,
}

impl ApplyPlan {
    pub fn is_empty(&self) -> bool {
        self.gameinfo.is_none() && self.video.is_none() && self.live.is_empty()
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
    #[error("{0} changed on disk since the plan was made; re-plan before applying")]
    Stale(PathBuf),
    #[error("base file {0}: {1}")]
    Base(PathBuf, std::io::Error),
}

/// Pinned preset text, or the file for `BaseRef::File` (which carries no video.txt).
pub fn resolve_base(profile: &Profile) -> Result<BaseTexts, ApplyError> {
    match &profile.base {
        BaseRef::Preset(id) => {
            let info = preset::info(*id);
            Ok(BaseTexts {
                gameinfo: info.pinned_gameinfo.to_string(),
                video: info.pinned_video.map(str::to_string),
            })
        }
        BaseRef::File(path) => Ok(BaseTexts {
            gameinfo: fs::read_to_string(path).map_err(|e| ApplyError::Base(path.clone(), e))?,
            video: None,
        }),
    }
}

/// Base ConVars block swapped into the live gameinfo (so SearchPaths edits survive), then the
/// profile's convar overrides minus denylisted names; base video settings + profile video edits.
pub fn target(
    live_gameinfo: &str,
    live_video: Option<&str>,
    base: &BaseTexts,
    profile: &Profile,
    catalog: &Catalog,
) -> Result<Target, ApplyError> {
    let (denied, allowed): (Overrides, Overrides) = profile
        .overrides()
        .into_iter()
        .partition(|(name, _)| catalog.is_denied(name));
    let swapped = gi::replace_convars_block(live_gameinfo, &base.gameinfo)?;
    let gameinfo = gi::apply_overrides(&swapped, &allowed)?.text;
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
        denied: denied.into_keys().collect(),
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

/// Ranked-safe target: the original (or vanilla preset) ConVars block, video.txt untouched.
pub fn ranked_safe_target(live_gameinfo: &str, store: &BackupStore) -> Result<Target, ApplyError> {
    let stock = match store.original(FileKind::GameInfo) {
        Some(entry) => fs::read_to_string(&entry.path)?,
        None => preset::info(PresetId::Vanilla).pinned_gameinfo.to_string(),
    };
    Ok(Target {
        gameinfo: gi::replace_convars_block(live_gameinfo, &stock)?,
        video: None,
        denied: Vec::new(),
    })
}

/// Checks every write against disk, snapshots originals, backs up, writes atomically, then
/// pushes `live` through the bridge.
///
/// Anything failing before or during the file writes is an `Err`. A bridge failure after the
/// writes is not: the report says what was written and carries the error in `bridge_error`.
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

    let mut report = ApplyReport {
        wrote_gameinfo: plan.gameinfo.is_some(),
        wrote_video: plan.video.is_some(),
        ..ApplyReport::default()
    };
    if let (Some(bridge), false) = (bridge, plan.live.is_empty()) {
        match bridge.push(&plan.live) {
            Ok(()) => report.pushed_live = plan.live.len(),
            Err(e) => report.bridge_error = Some(format!("{}: {e}", bridge.name())),
        }
    }
    report.needs_restart = !plan.restart.is_empty()
        || !plan.queued_cheat.is_empty()
        || report.wrote_video
        || report.pushed_live < plan.live.len();
    Ok(report)
}
