//! Builds and installs the DeadTune HUD addon. Every build starts from the
//! installed game's own `pak01_dir.vpk`, so a game update never leaves stale
//! vanilla copies behind: when `buildid` changes, rebuild.

use std::path::{Path, PathBuf};

use super::layout::{HudLayout, StylePatch};
use crate::locate::GamePaths;

/// Our addon. One file, owned by DeadTune; anything else in `addons/` is foreign.
pub const ADDON_FILE: &str = "pak77_dir.vpk";
pub const GAME_PAK: &str = "pak01_dir.vpk";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub addon: PathBuf,
    /// Style files both we and that addon override; one of them will not apply.
    pub paths: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HudAction {
    /// Write this VPK to `addon_path`.
    Write(Vec<u8>),
    /// Layout is vanilla: delete our addon if present.
    Remove,
    Nothing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HudPlan {
    pub addon_path: PathBuf,
    pub action: HudAction,
    pub patch: StylePatch,
    pub conflicts: Vec<Conflict>,
    /// gameinfo.gi lacks `Game citadel/addons`; run `searchpaths::ensure_addons`.
    pub needs_search_path: bool,
}

/// What `state_dir/hud.toml` records after `execute`, to recognise our file and
/// detect game updates.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InstallRecord {
    pub sha256: String,
    pub build_id: Option<String>,
    pub patched: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstalledState {
    None,
    Current(InstallRecord),
    /// Ours, but the game updated since; rebuild.
    Stale(InstallRecord),
    /// A file at our path that we did not write.
    Foreign,
}

#[derive(Debug, thiserror::Error)]
pub enum HudError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Vpk(#[from] super::vpk::VpkError),
    #[error(transparent)]
    Resource(#[from] super::resource::ResourceError),
    #[error(transparent)]
    Layout(#[from] super::layout::LayoutError),
    #[error(transparent)]
    SearchPaths(#[from] super::searchpaths::SearchPathsError),
    #[error("toml: {0}")]
    Toml(String),
    #[error("{0} is not ours; refusing to overwrite")]
    Foreign(PathBuf),
}

pub fn addons_dir(paths: &GamePaths) -> PathBuf {
    paths.citadel_dir.join("addons")
}

/// Pure planning plus reads: game pak, our record, other addons' trees, gameinfo.
pub fn plan(paths: &GamePaths, layout: &HudLayout, state_dir: &Path) -> Result<HudPlan, HudError> {
    todo!()
}

/// Writes or removes the addon atomically and updates `state_dir/hud.toml`.
/// Does not touch gameinfo.gi.
pub fn execute(plan: &HudPlan, paths: &GamePaths, state_dir: &Path) -> Result<(), HudError> {
    todo!()
}

pub fn installed_state(paths: &GamePaths, state_dir: &Path) -> Result<InstalledState, HudError> {
    todo!()
}

/// Builds the addon bytes from the game pak and a patch. Exposed for tests and the
/// example binary.
pub fn build_addon(game_pak: &super::vpk::VpkDir, patch: &StylePatch) -> Result<Vec<u8>, HudError> {
    todo!()
}
