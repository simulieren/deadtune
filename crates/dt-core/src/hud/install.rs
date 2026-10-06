//! Builds and installs the DeadTune HUD addon. Every build starts from the
//! installed game's own `pak01_dir.vpk`, so a game update never leaves stale
//! vanilla copies behind: when `buildid` changes, rebuild.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::icons::{self, IconProblem};
use super::inject::{self, SCRIPTS_DIR, STYLES_DIR};
use super::layout::{self, HudFeature, HudLayout, HudPatch};
use super::vpk::{self, VpkDir};
use super::{resource, searchpaths};
use crate::addons::verify;
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
    /// Icon overrides left out of this build, and why; the rest of the addon still ships.
    pub icon_problems: Vec<IconProblem>,
    /// What the layout changes, recorded with the pak so a failed launch can name it.
    pub features: Vec<HudFeature>,
    /// The layout as TOML, recorded with the pak so a game update can rebuild it.
    pub layout: Option<String>,
    /// The sha256 of every game stylesheet and layout the pak carries a copy of.
    pub sources: BTreeMap<String, String>,
}

impl HudPlan {
    /// Paths the addon carries: the patch's, minus icon overrides that failed to build.
    pub fn shipped(&self) -> impl Iterator<Item = &str> {
        self.patch
            .paths()
            .filter(|p| !self.icon_problems.iter().any(|i| i.game_path == *p))
    }
}

/// What `state_dir/hud.toml` records after `execute`, to recognise our file and
/// detect game updates.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InstallRecord {
    pub sha256: String,
    pub build_id: Option<String>,
    pub patched: Vec<String>,
    #[serde(default)]
    pub features: Vec<HudFeature>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sources: BTreeMap<String, String>,
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
    #[error("{0} is held open by the running game")]
    Locked(PathBuf),
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
    #[error("the built HUD addon failed its read-back check, so it was not installed: {0}")]
    Verify(String),
}

pub const RECORD_FILE: &str = "hud.toml";

pub fn addons_dir(paths: &GamePaths) -> PathBuf {
    paths.citadel_dir.join("addons")
}

/// Pure planning plus reads: game pak, our record, other addons' trees, gameinfo.
pub fn plan(paths: &GamePaths, layout: &HudLayout, state_dir: &Path) -> Result<HudPlan, HudError> {
    let mut plan = plan_patch(paths, layout::compile(layout)?, state_dir)?;
    plan.features = layout.features();
    plan.layout = Some(toml::to_string(layout).map_err(|e| HudError::Toml(e.to_string()))?);
    Ok(plan)
}

/// `plan` after compilation. An empty patch, or one whose every file failed to build (only
/// broken icon overrides), plans removal of our addon. The built pak is read back through
/// `addons::verify` against the game's files before it can be written.
pub fn plan_patch(
    paths: &GamePaths,
    patch: HudPatch,
    state_dir: &Path,
) -> Result<HudPlan, HudError> {
    let addon_path = addons_dir(paths).join(ADDON_FILE);
    let record = read_record(state_dir)?;
    let installed = file_sha(&addon_path)?;

    let mut sources = BTreeMap::new();
    let (bytes, icon_problems) = if patch.is_empty() {
        (None, Vec::new())
    } else {
        let pak_path = paths.citadel_dir.join(GAME_PAK);
        if !pak_path.is_file() {
            return Err(HudError::MissingGamePak(pak_path));
        }
        let game = VpkDir::open(&pak_path)?;
        let built = build_addon(&game, &patch, state_dir)?;
        sources = copied_sources(&game, &patch)?;
        if built.files.is_empty() {
            (None, built.icon_problems)
        } else {
            let bytes = vpk::write(&built.files);
            let pak = VpkDir::in_memory(bytes.clone())?;
            let verified = verify::verify(&pak, &verify::expect_for_hud(&game, &pak));
            if !verified.is_ok() {
                return Err(HudError::Verify(verified.to_string()));
            }
            (Some(bytes), built.icon_problems)
        }
    };

    let Some(bytes) = bytes else {
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
            icon_problems,
            features: Vec::new(),
            layout: None,
            sources: BTreeMap::new(),
        });
    };
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
        icon_problems,
        features: Vec::new(),
        layout: None,
        sources,
    })
}

/// The game files the pak carries whole copies of, by sha256.
fn copied_sources(game: &VpkDir, patch: &HudPatch) -> Result<BTreeMap<String, String>, HudError> {
    patch
        .styles
        .iter()
        .filter(|(_, css)| !css.is_empty())
        .map(|(path, _)| path)
        .chain(patch.layouts.keys())
        .map(|path| Ok((path.clone(), sha256_hex(&game.read(path)?))))
        .collect()
}

/// True when a game file the record's pak copied is gone or different now.
fn sources_changed(paths: &GamePaths, record: &InstallRecord) -> Result<bool, HudError> {
    if record.sources.is_empty() {
        return Ok(false);
    }
    let game = VpkDir::open(&paths.citadel_dir.join(GAME_PAK))?;
    Ok(record
        .sources
        .iter()
        .any(|(path, sha)| game.read(path).map_or(true, |b| sha256_hex(&b) != *sha)))
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
            atomic_write(&plan.addon_path, bytes).map_err(|e| pak_error(&plan.addon_path, e))?;
            let record = InstallRecord {
                sha256,
                build_id: build_id(paths)?,
                patched: plan.shipped().map(str::to_string).collect(),
                features: plan.features.clone(),
                layout: plan.layout.clone(),
                sources: plan.sources.clone(),
            };
            write_record(state_dir, &record)
        }
        HudAction::Remove => {
            ensure_owned(
                &plan.addon_path,
                installed.as_deref(),
                record.as_ref(),
                None,
            )?;
            match std::fs::remove_file(&plan.addon_path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    return Err(pak_error(&plan.addon_path, e));
                }
                _ => {}
            }
            remove_if_present(&record_path)?;
            Ok(())
        }
    }
}

/// Where the last HUD pak a launch proved is kept, as `<sha>.vpk` plus its `<sha>.toml`
/// record, so a failed launch puts it back instead of leaving the HUD vanilla.
pub const VERIFIED_DIR: &str = "hud-verified";

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudRollback {
    /// The kept copy of the last HUD that started is back in place.
    Restored,
    /// No usable kept copy, so our pak is gone and the HUD is the game's own.
    Removed,
}

/// Keeps a copy of our installed pak once a launch proved it, replacing any older copy.
/// Does nothing unless the pak on disk is ours and hashes to `sha256`.
pub fn keep_verified(paths: &GamePaths, state_dir: &Path, sha256: &str) -> Result<(), HudError> {
    let dir = state_dir.join(VERIFIED_DIR);
    let pak = dir.join(format!("{sha256}.vpk"));
    if pak.is_file() {
        return Ok(());
    }
    let Some(record) = read_record(state_dir)?.filter(|r| r.sha256 == sha256) else {
        return Ok(());
    };
    let bytes = match std::fs::read(addons_dir(paths).join(ADDON_FILE)) {
        Ok(bytes) if sha256_hex(&bytes) == sha256 => bytes,
        Ok(_) => return Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    std::fs::create_dir_all(&dir)?;
    let text = toml::to_string(&record).map_err(|e| HudError::Toml(e.to_string()))?;
    atomic_write(&dir.join(format!("{sha256}.toml")), text.as_bytes())?;
    atomic_write(&pak, &bytes)?;
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.file_stem().and_then(|s| s.to_str()) != Some(sha256) {
            remove_if_present(&path)?;
        }
    }
    Ok(())
}

/// The kept copy, if it is intact and was built for the installed game build: a copy from
/// before a game update may break the start the same way a fresh build did.
fn kept_copy(
    paths: &GamePaths,
    state_dir: &Path,
) -> Result<Option<(Vec<u8>, InstallRecord)>, HudError> {
    let dir = state_dir.join(VERIFIED_DIR);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(None);
    };
    let build = build_id(paths)?;
    for entry in entries {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let Ok(record) = toml::from_str::<InstallRecord>(&std::fs::read_to_string(&path)?) else {
            continue;
        };
        let Ok(bytes) = std::fs::read(path.with_extension("vpk")) else {
            continue;
        };
        if sha256_hex(&bytes) == record.sha256 && record.build_id == build {
            return Ok(Some((bytes, record)));
        }
    }
    Ok(None)
}

/// Takes our pak out of a failed launch: the kept copy of the last HUD that started goes
/// back in when there is one for this game build, otherwise our pak and record are removed.
pub fn roll_back(paths: &GamePaths, state_dir: &Path) -> Result<HudRollback, HudError> {
    let addon_path = addons_dir(paths).join(ADDON_FILE);
    let record = read_record(state_dir)?;
    let installed = file_sha(&addon_path)?;
    ensure_owned(&addon_path, installed.as_deref(), record.as_ref(), None)?;
    match kept_copy(paths, state_dir)?.filter(|(_, r)| installed.as_ref() != Some(&r.sha256)) {
        Some((bytes, kept)) => {
            if let Some(dir) = addon_path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            atomic_write(&addon_path, &bytes)?;
            write_record(state_dir, &kept)?;
            Ok(HudRollback::Restored)
        }
        None => {
            remove_if_present(&addon_path)?;
            remove_if_present(&state_dir.join(RECORD_FILE))?;
            Ok(HudRollback::Removed)
        }
    }
}

fn write_record(state_dir: &Path, record: &InstallRecord) -> Result<(), HudError> {
    let text = toml::to_string(record).map_err(|e| HudError::Toml(e.to_string()))?;
    std::fs::create_dir_all(state_dir)?;
    Ok(atomic_write(&state_dir.join(RECORD_FILE), text.as_bytes())?)
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

/// What `refresh_after_update` did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refreshed {
    /// Nothing of ours, or ours and built from the installed game's files.
    Current,
    /// Steam is still updating the game; try again once it is done.
    SteamBusy,
    Rebuilt,
    /// Rebuilt without the parts that no longer build; why they did not.
    Partial {
        dropped: Vec<HudFeature>,
        why: String,
    },
    /// Could not be rebuilt for the new game, so our pak is out; why.
    Removed(String),
}

/// After a game update our pak still carries the old build's copies of the files it
/// changes, and an old layout can stop the game from starting (build 25738777 dropped a
/// panel the old `hud_minimap` still named). Rebuilds the pak from the new game files with
/// the layout it was built from. When that fails, the parts that rebuild game layouts are
/// dropped and the rest rebuilt; when even that fails, the pak is taken out.
pub fn refresh_after_update(paths: &GamePaths, state_dir: &Path) -> Result<Refreshed, HudError> {
    if locate::steam_busy(paths) {
        return Ok(Refreshed::SteamBusy);
    }
    let record = match installed_state(paths, state_dir)? {
        InstalledState::Stale(record) => record,
        InstalledState::Current(record) if sources_changed(paths, &record)? => record,
        _ => return Ok(Refreshed::Current),
    };
    let rebuild = |layout: &HudLayout| {
        plan(paths, layout, state_dir)
            .and_then(|plan| execute(&plan, paths, state_dir))
            .map_err(|e| e.to_string())
    };
    let layout = match record.layout.as_deref().map(toml::from_str::<HudLayout>) {
        Some(Ok(layout)) => layout,
        Some(Err(e)) => return take_out(paths, state_dir, e.to_string()),
        None => {
            return take_out(
                paths,
                state_dir,
                "an older DeadTune built it and did not record how".into(),
            );
        }
    };
    let why = match rebuild(&layout) {
        Ok(()) => return Ok(Refreshed::Rebuilt),
        Err(why) => why,
    };
    let (smaller, dropped) = layout.without_layout_rebuilds();
    if dropped.is_empty() {
        return take_out(paths, state_dir, why);
    }
    match rebuild(&smaller) {
        Ok(()) => Ok(Refreshed::Partial { dropped, why }),
        Err(_) => take_out(paths, state_dir, why),
    }
}

fn take_out(paths: &GamePaths, state_dir: &Path, why: String) -> Result<Refreshed, HudError> {
    remove_if_present(&addons_dir(paths).join(ADDON_FILE))?;
    remove_if_present(&state_dir.join(RECORD_FILE))?;
    Ok(Refreshed::Removed(why))
}

/// The addon's files, from the game pak and a patch.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuiltAddon {
    pub files: BTreeMap<String, Vec<u8>>,
    /// Icon overrides that could not be built; everything else is in `files`.
    pub icon_problems: Vec<IconProblem>,
}

/// Builds the addon's files from the game pak and a patch: the game's stylesheets with our
/// CSS appended, the game's layouts rebuilt as text with our includes, our own scripts and
/// stylesheets, and the player's icons encoded from the game's images (stored images are
/// read from `data_dir`). Exposed for tests and the example binary.
pub fn build_addon(
    game_pak: &VpkDir,
    patch: &HudPatch,
    data_dir: &Path,
) -> Result<BuiltAddon, HudError> {
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
    let (images, icon_problems) = icons::build(game_pak, &patch.icons, data_dir);
    files.extend(images);
    Ok(BuiltAddon {
        files,
        icon_problems,
    })
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
            let paths: Vec<String> = patch
                .paths()
                .filter(|p| other.contains(p))
                .map(str::to_string)
                .collect();
            (!paths.is_empty()).then_some(Conflict { addon, paths })
        })
        .collect()
}

pub fn read_record(state_dir: &Path) -> Result<Option<InstallRecord>, HudError> {
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

fn pak_error(path: &Path, e: std::io::Error) -> HudError {
    if crate::backup::is_locked(&e) {
        HudError::Locked(path.to_path_buf())
    } else {
        e.into()
    }
}

fn remove_if_present(path: &Path) -> Result<(), HudError> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}
