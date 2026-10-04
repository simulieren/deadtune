//! Turns a profile into console commands + file writes, split by apply class.
//!
//! Pipeline: live files + profile -> `Target` (pure) -> `ApplyPlan` (pure) -> `execute` (effects).

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::backup::BackupStore;
use crate::bridge::{Bridge, BridgeError, ConsoleCmd};
use crate::catalog::Catalog;
use crate::gi::GiError;
use crate::locate::GamePaths;
use crate::profile::Profile;
use crate::video::VideoError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileWrite {
    pub path: PathBuf,
    pub before: String,
    pub after: String,
}

impl FileWrite {
    /// Unified diff (via `similar`) for the review pane and `dt-cli diff`.
    pub fn unified_diff(&self) -> String {
        todo!()
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
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ApplyPlan {
    /// Sent through the bridge now (also persisted in the files).
    pub live: Vec<ConsoleCmd>,
    /// Cheat-flagged changes held back because `in_sandbox` is false; take effect next launch.
    pub queued_cheat: Vec<String>,
    /// devonly/unknown changes; take effect next launch.
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
    #[error("base file {0}: {1}")]
    Base(PathBuf, std::io::Error),
}

/// Pinned preset text, or the file for `BaseRef::File`.
pub fn resolve_base(profile: &Profile) -> Result<BaseTexts, ApplyError> {
    let _ = profile;
    todo!()
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
    let _ = (live_gameinfo, live_video, base, profile, catalog);
    todo!()
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
    let _ = (paths, live_gameinfo, live_video, target, catalog, ctx);
    todo!()
}

/// Ranked-safe target: the original (or vanilla preset) ConVars block, video.txt untouched.
pub fn ranked_safe_target(live_gameinfo: &str, store: &BackupStore) -> Result<Target, ApplyError> {
    let _ = (live_gameinfo, store);
    todo!()
}

/// Snapshots originals, backs up, writes atomically, then pushes `live` through the bridge.
/// Re-validates braces of the gameinfo text before writing.
pub fn execute(
    plan: &ApplyPlan,
    store: &BackupStore,
    bridge: Option<&mut dyn Bridge>,
) -> Result<ApplyReport, ApplyError> {
    let _ = (plan, store, bridge);
    todo!()
}
