//! Builds and installs the DeadTune HUD addon. Every build starts from the
//! installed game's own `pak01_dir.vpk`, so a game update never leaves stale
//! vanilla copies behind: when `buildid` changes, rebuild.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::inject::{self, SCRIPTS_DIR, STYLES_DIR};
use super::layout::{self, HudLayout, HudPatch};
use super::vpk::{self, VpkDir};
use super::{resource, searchpaths};
use crate::backup::{atomic_write, sha256_hex};
use crate::locate::{self, GamePaths};

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
    pub patch: HudPatch,
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
    #[error("{0}: {1}")]
    Inject(String, super::inject::InjectError),
    #[error("{0} is not a script or stylesheet of ours")]
    OwnFile(String),
    #[error(transparent)]
    Layout(#[from] super::layout::LayoutError),
    #[error(transparent)]
    SearchPaths(#[from] super::searchpaths::SearchPathsError),
    #[error("toml: {0}")]
    Toml(String),
    #[error("{0} is not ours; refusing to overwrite")]
    Foreign(PathBuf),
    #[error("game archive {0} not found; is the game fully installed?")]
    MissingGamePak(PathBuf),
}

pub const RECORD_FILE: &str = "hud.toml";

pub fn addons_dir(paths: &GamePaths) -> PathBuf {
    paths.citadel_dir.join("addons")
}

/// Pure planning plus reads: game pak, our record, other addons' trees, gameinfo.
pub fn plan(paths: &GamePaths, layout: &HudLayout, state_dir: &Path) -> Result<HudPlan, HudError> {
    plan_patch(paths, layout::compile(layout)?, state_dir)
}

/// `plan` after compilation. An empty patch plans removal of our addon.
pub fn plan_patch(
    paths: &GamePaths,
    patch: HudPatch,
    state_dir: &Path,
) -> Result<HudPlan, HudError> {
    let addon_path = addons_dir(paths).join(ADDON_FILE);
    let record = read_record(state_dir)?;
    let installed = file_sha(&addon_path)?;

    if patch.is_empty() {
        ensure_owned(&addon_path, installed.as_deref(), record.as_ref(), None)?;
        let action = if installed.is_some() || record.is_some() {
            HudAction::Remove
        } else {
            HudAction::Nothing
        };
        return Ok(HudPlan {
            addon_path,
            action,
            patch,
            conflicts: Vec::new(),
            needs_search_path: false,
        });
    }

    let pak_path = paths.citadel_dir.join(GAME_PAK);
    if !pak_path.is_file() {
        return Err(HudError::MissingGamePak(pak_path));
    }
    let bytes = build_addon(&VpkDir::open(&pak_path)?, &patch)?;
    let built = sha256_hex(&bytes);
    ensure_owned(
        &addon_path,
        installed.as_deref(),
        record.as_ref(),
        Some(&built),
    )?;

    let build = build_id(paths)?;
    let up_to_date = installed.as_deref() == Some(built.as_str())
        && record.is_some_and(|r| r.sha256 == built && r.build_id == build);
    let action = if up_to_date {
        HudAction::Nothing
    } else {
        HudAction::Write(bytes)
    };
    // Checked for Nothing too: a game update can restore a stock gameinfo.gi while
    // our addon stays in place, unmounted.
    let gameinfo = std::fs::read_to_string(&paths.gameinfo)?;
    let needs_search_path = !searchpaths::addons_ready(&gameinfo)?;
    let conflicts = conflicts(&addons_dir(paths), &patch);
    Ok(HudPlan {
        addon_path,
        action,
        patch,
        conflicts,
        needs_search_path,
    })
}

/// Writes or removes the addon atomically and updates `state_dir/hud.toml`.
/// Does not touch gameinfo.gi.
pub fn execute(plan: &HudPlan, paths: &GamePaths, state_dir: &Path) -> Result<(), HudError> {
    let record_path = state_dir.join(RECORD_FILE);
    // Re-checked here because the addons dir may have changed since `plan`.
    let record = read_record(state_dir)?;
    let installed = file_sha(&plan.addon_path)?;
    match &plan.action {
        HudAction::Nothing => Ok(()),
        HudAction::Write(bytes) => {
            let sha256 = sha256_hex(bytes);
            ensure_owned(
                &plan.addon_path,
                installed.as_deref(),
                record.as_ref(),
                Some(&sha256),
            )?;
            if let Some(dir) = plan.addon_path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            atomic_write(&plan.addon_path, bytes)?;
            let record = InstallRecord {
                sha256,
                build_id: build_id(paths)?,
                patched: patched_paths(&plan.patch).map(str::to_string).collect(),
            };
            let text = toml::to_string(&record).map_err(|e| HudError::Toml(e.to_string()))?;
            std::fs::create_dir_all(state_dir)?;
            atomic_write(&record_path, text.as_bytes())?;
            Ok(())
        }
        HudAction::Remove => {
            ensure_owned(
                &plan.addon_path,
                installed.as_deref(),
                record.as_ref(),
                None,
            )?;
            remove_if_present(&plan.addon_path)?;
            remove_if_present(&record_path)?;
            Ok(())
        }
    }
}

pub fn installed_state(paths: &GamePaths, state_dir: &Path) -> Result<InstalledState, HudError> {
    let Some(installed) = file_sha(&addons_dir(paths).join(ADDON_FILE))? else {
        return Ok(InstalledState::None);
    };
    match read_record(state_dir)? {
        Some(record) if record.sha256 == installed => {
            if record.build_id == build_id(paths)? {
                Ok(InstalledState::Current(record))
            } else {
                Ok(InstalledState::Stale(record))
            }
        }
        _ => Ok(InstalledState::Foreign),
    }
}

/// Builds the addon bytes from the game pak and a patch: the game's stylesheets with our
/// CSS appended, the game's layouts rebuilt as text with our includes, and our own
/// scripts and stylesheets. Exposed for tests and the example binary.
pub fn build_addon(game_pak: &VpkDir, patch: &HudPatch) -> Result<Vec<u8>, HudError> {
    let mut files = BTreeMap::new();
    for (path, css) in patch.styles.iter().filter(|(_, css)| !css.is_empty()) {
        let compiled = game_pak.read(path)?;
        files.insert(path.clone(), resource::append_style(&compiled, css)?);
    }
    for (path, edit) in &patch.layouts {
        let compiled = game_pak.read(path)?;
        let includes: Vec<&str> = edit
            .style_includes
            .iter()
            .chain(&edit.script_includes)
            .map(String::as_str)
            .collect();
        let note = format!(
            "Rebuilt by DeadTune from the game's own {path}; adds {}",
            includes.join(", ")
        );
        let built = inject::patched_layout(&compiled, edit, &[], &note)
            .map_err(|e| HudError::Inject(path.clone(), e))?;
        files.insert(path.clone(), built);
    }
    for (path, text) in &patch.own_files {
        let built = if path.starts_with(SCRIPTS_DIR) && path.ends_with(".vjs_c") {
            inject::script_resource(text)
        } else if path.starts_with(STYLES_DIR) && path.ends_with(".vcss_c") {
            inject::style_resource(text)
        } else {
            return Err(HudError::OwnFile(path.clone()));
        };
        files.insert(path.clone(), built);
    }
    Ok(vpk::write(&files))
}

fn patched_paths(patch: &HudPatch) -> impl Iterator<Item = &str> {
    patch.paths()
}

/// A file at our path is ours if it matches our record, or is byte-identical to what
/// we are about to write (a crash between the addon write and the record write).
fn ensure_owned(
    addon_path: &Path,
    installed: Option<&str>,
    record: Option<&InstallRecord>,
    building: Option<&str>,
) -> Result<(), HudError> {
    match installed {
        Some(sha) if record.is_none_or(|r| r.sha256 != sha) && building != Some(sha) => {
            Err(HudError::Foreign(addon_path.to_path_buf()))
        }
        _ => Ok(()),
    }
}

/// Other addons that also ship a style file we patch. Unreadable archives are skipped:
/// the scan is advisory and must not block an install.
fn conflicts(addons: &Path, patch: &HudPatch) -> Vec<Conflict> {
    let Ok(dir) = std::fs::read_dir(addons) else {
        return Vec::new();
    };
    let mut others: Vec<PathBuf> = dir
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with("_dir.vpk") && n != ADDON_FILE)
        })
        .collect();
    others.sort();
    others
        .into_iter()
        .filter_map(|addon| {
            let other = VpkDir::open(&addon).ok()?;
            let paths: Vec<String> = patched_paths(patch)
                .filter(|p| other.contains(p))
                .map(str::to_string)
                .collect();
            (!paths.is_empty()).then_some(Conflict { addon, paths })
        })
        .collect()
}

fn read_record(state_dir: &Path) -> Result<Option<InstallRecord>, HudError> {
    match std::fs::read_to_string(state_dir.join(RECORD_FILE)) {
        Ok(text) => toml::from_str(&text)
            .map(Some)
            .map_err(|e| HudError::Toml(e.to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn file_sha(path: &Path) -> Result<Option<String>, HudError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(sha256_hex(&bytes))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn build_id(paths: &GamePaths) -> Result<Option<String>, HudError> {
    match &paths.appmanifest {
        Some(acf) => Ok(locate::parse_buildid(&std::fs::read_to_string(acf)?)),
        None => Ok(None),
    }
}

fn remove_if_present(path: &Path) -> Result<(), HudError> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}
