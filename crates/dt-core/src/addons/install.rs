//! Plans and installs one DeadTune-owned pak file per enabled addon in
//! `citadel/addons`. `state_dir/addons.toml` records what we wrote (file name, sha256,
//! size, mtime, the inputs it was built from); a file that does not match the record
//! is foreign and is never written to or deleted. Slots are chosen deterministically:
//! the addon's preferred `pakNN`, else the first free number from 70 up, never the HUD
//! addon's and never a name another mod already uses.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use sha2::{Digest, Sha256};

use super::textures::{self, Progress};
use super::{AddonError, AddonId, AddonsConfig, Kind, Source, blur, info, particles, sources};
use crate::backup::{atomic_write, sha256_hex};
use crate::hud::install::{ADDON_FILE as HUD_ADDON_FILE, GAME_PAK, addons_dir};
use crate::hud::searchpaths;
use crate::hud::vpk::{self, VpkDir};
use crate::locate::{self, GamePaths};
use crate::texture::{self as texture_build, Stats};

pub const RECORD_FILE: &str = "addons.toml";
const FIRST_SLOT: u8 = 70;
const LAST_SLOT: u8 = 99;

#[derive(Clone, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Record {
    /// Keyed by [`AddonId::key`].
    #[serde(default)]
    pub installed: BTreeMap<String, Installed>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Installed {
    pub file: String,
    pub sha256: String,
    pub len: u64,
    /// Nanoseconds since the epoch; with `len` the cheap ownership check before hashing.
    pub mtime: Option<u64>,
    /// Fingerprint of everything the pak was built from; a change means rebuild.
    pub input: String,
    pub build_id: Option<String>,
    /// `pakNN_000.vpk` and on, when the payload needed chunks. Removed with the dir file.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chunks: Vec<String>,
}

/// Why an enabled addon cannot be installed right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Blocker {
    /// The upstream file is not in the cache: fetch or import it.
    NotDownloaded,
    /// The game archive could not be read.
    GameFiles(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Build {
    Bytes(Vec<u8>),
    /// The upstream file, byte for byte.
    Copy(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Write(Build),
    Remove,
    Keep,
    /// Needs a [`build_textures`] run, which Apply does not start on its own.
    Build,
    Unavailable(Blocker),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddonPlan {
    pub id: AddonId,
    pub path: PathBuf,
    pub action: Action,
    input: Option<String>,
    /// Paths our pak overrides, for the conflict scan.
    ships: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub id: AddonId,
    pub addon: PathBuf,
    /// Game paths both we and that addon override; one of them will not apply.
    pub paths: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct AddonsPlan {
    pub addons: Vec<AddonPlan>,
    /// gameinfo.gi lacks `Game citadel/addons`; run `searchpaths::ensure_addons`.
    pub needs_search_path: bool,
    pub conflicts: Vec<Conflict>,
}

impl AddonsPlan {
    /// True when Apply would write or delete nothing.
    pub fn is_empty(&self) -> bool {
        !self
            .addons
            .iter()
            .any(|a| matches!(a.action, Action::Write(_) | Action::Remove))
    }

    pub fn get(&self, id: AddonId) -> Option<&AddonPlan> {
        self.addons.iter().find(|a| a.id == id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstalledState {
    None,
    Current(Installed),
    /// Ours, generated from game files, and the game updated since; rebuild.
    Stale(Installed),
    /// The record names this file but it is not what we wrote.
    Foreign(String),
}

pub fn read_record(state_dir: &Path) -> Result<Record, AddonError> {
    match std::fs::read_to_string(state_dir.join(RECORD_FILE)) {
        Ok(text) => toml::from_str(&text).map_err(|e| AddonError::Toml(e.to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Record::default()),
        Err(e) => Err(e.into()),
    }
}

fn write_record(state_dir: &Path, record: &Record) -> Result<(), AddonError> {
    std::fs::create_dir_all(state_dir)?;
    let text = toml::to_string(record).map_err(|e| AddonError::Toml(e.to_string()))?;
    Ok(atomic_write(&state_dir.join(RECORD_FILE), text.as_bytes())?)
}

fn build_id(paths: &GamePaths) -> Result<Option<String>, AddonError> {
    match &paths.appmanifest {
        Some(acf) => Ok(locate::parse_buildid(&std::fs::read_to_string(acf)?)),
        None => Ok(None),
    }
}

fn fingerprint(parts: &[&str]) -> String {
    sha256_hex(parts.join("\n").as_bytes())
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

fn stamp(
    path: &Path,
    file: String,
    sha256: String,
    input: String,
    build_id: Option<String>,
) -> std::io::Result<Installed> {
    let meta = std::fs::metadata(path)?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64);
    Ok(Installed {
        file,
        sha256,
        len: meta.len(),
        mtime,
        input,
        build_id,
        chunks: Vec::new(),
    })
}

/// `pakNN_NNN.vpk` siblings of a `pakNN_dir.vpk`, sorted.
fn chunks_of(dir: &Path, file: &str) -> Vec<String> {
    let Some(stem) = file.strip_suffix("_dir.vpk") else {
        return Vec::new();
    };
    let prefix = format!("{stem}_");
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .map(|d| {
            d.filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().to_str().map(str::to_string))
                .filter(|n| {
                    n.strip_prefix(&prefix)
                        .and_then(|rest| rest.strip_suffix(".vpk"))
                        .is_some_and(|n| n.len() == 3 && n.bytes().all(|b| b.is_ascii_digit()))
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// Size and mtime first (free); the hash only when they moved.
fn is_ours(path: &Path, rec: &Installed) -> Result<bool, AddonError> {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.into()),
    };
    if meta.len() != rec.len {
        return Ok(false);
    }
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64);
    if mtime.is_some() && mtime == rec.mtime {
        return Ok(true);
    }
    Ok(sha256_file(path)? == rec.sha256)
}

fn owned<'a>(dir: &Path, rec: Option<&'a Installed>) -> Result<Option<&'a Installed>, AddonError> {
    match rec {
        Some(r) if is_ours(&dir.join(&r.file), r)? => Ok(Some(r)),
        _ => Ok(None),
    }
}

/// First free `pakNN_dir.vpk`: the addon's own number, then 70 upwards, skipping the
/// HUD addon, names already claimed in this pass, and any dir file or chunk on disk.
fn allocate(id: AddonId, dir: &Path, taken: &mut BTreeSet<String>) -> Result<String, AddonError> {
    for n in std::iter::once(info(id).slot).chain(FIRST_SLOT..=LAST_SLOT) {
        let name = format!("pak{n:02}_dir.vpk");
        if name == HUD_ADDON_FILE
            || taken.contains(&name)
            || dir.join(&name).exists()
            || !chunks_of(dir, &name).is_empty()
        {
            continue;
        }
        taken.insert(name.clone());
        return Ok(name);
    }
    Err(AddonError::NoFreeSlot(dir.to_path_buf()))
}

/// What an enabled addon would be built from, before deciding whether to build it.
struct Inputs {
    input: String,
    ships: Vec<String>,
    /// Lazily built so an up-to-date install reads no archives.
    make: Box<dyn FnOnce() -> Result<Option<Build>, AddonError>>,
}

fn inputs(
    id: AddonId,
    config: &AddonsConfig,
    paths: &GamePaths,
    cache: &Path,
    build_id: Option<&str>,
) -> Result<Result<Inputs, Blocker>, AddonError> {
    let source = match info(id).source {
        Source::Upstream { .. } => match sources::cached(cache, id)? {
            Some(a) => Some(a),
            None => return Ok(Err(Blocker::NotDownloaded)),
        },
        Source::Generated => None,
    };
    let out = match info(id).kind {
        Kind::Toggle => {
            let src = source.expect("toggle addons have an upstream source");
            let ships = VpkDir::open(&src.path)?.entries.into_keys().collect();
            let path = src.path.clone();
            Inputs {
                input: fingerprint(&["toggle", &src.sha256]),
                ships,
                make: Box::new(move || Ok(Some(Build::Copy(path)))),
            }
        }
        Kind::ParticleGroups => {
            let src = source.expect("the particle disabler has an upstream source");
            let hidden = particles::hidden_paths(&config.keep_particles);
            let mut parts = vec!["particles", src.sha256.as_str()];
            parts.extend(hidden.iter().copied());
            let mut ships: Vec<String> = hidden.iter().map(|p| p.to_string()).collect();
            if !ships.is_empty() {
                ships.push(particles::DEBUG_TEXTURE.to_string());
            }
            let path = src.path.clone();
            Inputs {
                input: fingerprint(&parts),
                ships,
                make: Box::new(move || {
                    let stub = particles::stub_from_upstream(&VpkDir::open(&path)?)?;
                    Ok(particles::build(&stub, &hidden).map(Build::Bytes))
                }),
            }
        }
        Kind::GeneratedCss => {
            let pak_path = paths.citadel_dir.join(GAME_PAK);
            if !pak_path.is_file() {
                return Ok(Err(Blocker::GameFiles(format!(
                    "{GAME_PAK} not found; is the game fully installed?"
                ))));
            }
            let pak = match VpkDir::open(&pak_path) {
                Ok(pak) => pak,
                Err(e) => return Ok(Err(Blocker::GameFiles(e.to_string()))),
            };
            let Some(entry) = pak.entries.get(blur::STYLE) else {
                return Ok(Err(Blocker::GameFiles(format!(
                    "{} is missing from {GAME_PAK}",
                    blur::STYLE
                ))));
            };
            let opts = config.blur;
            Inputs {
                input: fingerprint(&[
                    "blur",
                    &format!("{:08x}", entry.crc),
                    build_id.unwrap_or(""),
                    &format!("hud={} menu={}", opts.hud, opts.menu),
                ]),
                ships: vec![blur::STYLE.to_string()],
                make: Box::new(move || {
                    let files =
                        BTreeMap::from([(blur::STYLE.to_string(), blur::build(&pak, &opts)?)]);
                    Ok(Some(Build::Bytes(vpk::write(&files))))
                }),
            }
        }
        Kind::Textures => Inputs {
            input: fingerprint(&[
                "textures",
                &textures::fingerprint(&config.textures),
                build_id.unwrap_or(""),
            ]),
            ships: Vec::new(),
            make: Box::new(|| Ok(None)),
        },
    };
    Ok(Ok(out))
}

/// Pure planning plus reads: our record, the cache, the game pak for generated addons,
/// other addons' trees for conflicts, gameinfo for the search path.
pub fn plan(
    paths: &GamePaths,
    config: &AddonsConfig,
    state_dir: &Path,
) -> Result<AddonsPlan, AddonError> {
    let dir = addons_dir(paths);
    let record = read_record(state_dir)?;
    let cache = sources::cache_dir(state_dir);
    let build = build_id(paths)?;
    let mut taken: BTreeSet<String> = record.installed.values().map(|i| i.file.clone()).collect();
    let mut addons = Vec::new();
    for id in AddonId::ALL {
        let rec = owned(&dir, record.installed.get(id.key()))?;
        let remove = |rec: &Installed| AddonPlan {
            id,
            path: dir.join(&rec.file),
            action: Action::Remove,
            input: None,
            ships: Vec::new(),
        };
        if !config.is_enabled(id) {
            addons.extend(rec.map(remove));
            continue;
        }
        let prepared = match inputs(id, config, paths, &cache, build.as_deref())? {
            Ok(p) => p,
            Err(blocker) => {
                // An installed copy stays usable while its source is missing.
                let (path, action) = match rec {
                    Some(r) => (dir.join(&r.file), Action::Keep),
                    None => (
                        dir.join(format!("pak{:02}_dir.vpk", info(id).slot)),
                        Action::Unavailable(blocker),
                    ),
                };
                addons.push(AddonPlan {
                    id,
                    path,
                    action,
                    input: None,
                    ships: Vec::new(),
                });
                continue;
            }
        };
        if let Some(r) = rec
            && r.input == prepared.input
        {
            addons.push(AddonPlan {
                id,
                path: dir.join(&r.file),
                action: Action::Keep,
                input: Some(prepared.input),
                ships: prepared.ships,
            });
            continue;
        }
        let action = match info(id).kind {
            Kind::Textures => Action::Build,
            _ => match (prepared.make)() {
                Ok(Some(build)) => Action::Write(build),
                // Nothing left to override (every particle group kept): same as off.
                Ok(None) => {
                    addons.extend(rec.map(remove));
                    continue;
                }
                // One addon failing to build must not block the others; an installed
                // copy stays in place.
                Err(e) => {
                    addons.push(AddonPlan {
                        id,
                        path: rec.map_or_else(
                            || dir.join(format!("pak{:02}_dir.vpk", info(id).slot)),
                            |r| dir.join(&r.file),
                        ),
                        action: Action::Unavailable(Blocker::GameFiles(e.to_string())),
                        input: None,
                        ships: Vec::new(),
                    });
                    continue;
                }
            },
        };
        // Our slot stays ours when the file is merely gone (deleted by hand).
        let file = match (rec, record.installed.get(id.key())) {
            (Some(r), _) => r.file.clone(),
            (None, Some(stale)) if !dir.join(&stale.file).exists() => stale.file.clone(),
            _ => allocate(id, &dir, &mut taken)?,
        };
        addons.push(AddonPlan {
            id,
            path: dir.join(file),
            action,
            input: Some(prepared.input),
            ships: prepared.ships,
        });
    }

    let active = addons
        .iter()
        .any(|a| matches!(a.action, Action::Write(_) | Action::Keep | Action::Build));
    // Checked for Keep too: a game update can restore a stock gameinfo.gi while our
    // addons stay in place, unmounted.
    let needs_search_path = active && {
        let gameinfo = std::fs::read_to_string(&paths.gameinfo)?;
        !searchpaths::has_addons(&gameinfo)?
    };
    let conflicts = conflicts(&dir, &addons, &taken);
    Ok(AddonsPlan {
        addons,
        needs_search_path,
        conflicts,
    })
}

/// Other addons that also ship a path we override. Unreadable archives are skipped:
/// the scan is advisory and must not block an install.
fn conflicts(dir: &Path, addons: &[AddonPlan], ours: &BTreeSet<String>) -> Vec<Conflict> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut others: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                n.ends_with("_dir.vpk") && n != HUD_ADDON_FILE && !ours.contains(n)
            })
        })
        .collect();
    others.sort();
    let mut out = Vec::new();
    for other in others {
        let Ok(vpk) = VpkDir::open(&other) else {
            continue;
        };
        for a in addons {
            let paths: Vec<String> = a
                .ships
                .iter()
                .filter(|p| vpk.contains(p))
                .cloned()
                .collect();
            if !paths.is_empty() {
                out.push(Conflict {
                    id: a.id,
                    addon: other.clone(),
                    paths,
                });
            }
        }
    }
    out
}

fn ensure_writable(
    path: &Path,
    rec: Option<&Installed>,
    writing: Option<&str>,
) -> Result<(), AddonError> {
    if !path.exists() {
        return Ok(());
    }
    let ours = match rec {
        Some(r) if r.file == path.file_name().and_then(|n| n.to_str()).unwrap_or("") => {
            is_ours(path, r)?
        }
        _ => false,
    };
    // Byte-identical to what we write: a crash between the pak write and the record write.
    let same = match writing {
        Some(sha) => sha256_file(path)? == sha,
        None => false,
    };
    if ours || same {
        Ok(())
    } else {
        Err(AddonError::Foreign(path.to_path_buf()))
    }
}

/// Deletes DeadTune's pak (and chunk files) for `id` right away, without a plan. Returns
/// false when nothing of ours is installed; a file someone else replaced is refused.
pub fn remove_now(id: AddonId, paths: &GamePaths, state_dir: &Path) -> Result<bool, AddonError> {
    let mut record = read_record(state_dir)?;
    let Some(entry) = record.installed.get(id.key()).cloned() else {
        return Ok(false);
    };
    let dir = addons_dir(paths);
    ensure_writable(&dir.join(&entry.file), Some(&entry), None)?;
    for file in std::iter::once(&entry.file).chain(&entry.chunks) {
        match std::fs::remove_file(dir.join(file)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
    }
    record.installed.remove(id.key());
    write_record(state_dir, &record)?;
    Ok(true)
}

/// Writes and removes pak files atomically, updating `state_dir/addons.toml` after each
/// one. Does not touch gameinfo.gi and does not build textures. Returns whether any
/// file changed.
pub fn execute(plan: &AddonsPlan, paths: &GamePaths, state_dir: &Path) -> Result<bool, AddonError> {
    let dir = addons_dir(paths);
    // Re-read: the addons dir and the record may have changed since `plan`.
    let mut record = read_record(state_dir)?;
    let build = build_id(paths)?;
    let mut changed = false;
    for a in &plan.addons {
        let key = a.id.key();
        let file = a
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        match &a.action {
            Action::Write(build_from) => {
                let bytes: Cow<[u8]> = match build_from {
                    Build::Bytes(b) => Cow::Borrowed(b),
                    Build::Copy(src) => Cow::Owned(std::fs::read(src)?),
                };
                let sha256 = sha256_hex(&bytes);
                ensure_writable(&a.path, record.installed.get(key), Some(&sha256))?;
                std::fs::create_dir_all(&dir)?;
                atomic_write(&a.path, &bytes)?;
                let input = a.input.clone().unwrap_or_default();
                record.installed.insert(
                    key.to_string(),
                    stamp(&a.path, file, sha256, input, build.clone())?,
                );
                write_record(state_dir, &record)?;
                changed = true;
            }
            Action::Remove => {
                ensure_writable(&a.path, record.installed.get(key), None)?;
                let chunks = record
                    .installed
                    .get(key)
                    .map(|r| r.chunks.clone())
                    .unwrap_or_default();
                for file in std::iter::once(&file).chain(&chunks) {
                    match std::fs::remove_file(dir.join(file)) {
                        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
                        _ => {}
                    }
                }
                record.installed.remove(key);
                write_record(state_dir, &record)?;
                changed = true;
            }
            Action::Keep | Action::Build | Action::Unavailable(_) => {}
        }
    }
    // Entries whose file went missing (deleted by hand) only cause confusion later.
    let before = record.installed.len();
    record.installed.retain(|_, r| dir.join(&r.file).exists());
    if record.installed.len() != before {
        write_record(state_dir, &record)?;
    }
    Ok(changed)
}

/// The game archives textures are read from: every `pakNN_dir.vpk` beside gameinfo.gi.
pub fn game_vpks(paths: &GamePaths) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(&paths.citadel_dir)
        .map(|d| {
            d.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("pak") && n.ends_with("_dir.vpk"))
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// Builds the texture downscaler pak straight into our slot (the builder removes its
/// own partial output on error or cancel) and records the result with its chunks.
/// Long: the caller runs it off the UI thread and shows `progress`; returning
/// `ControlFlow::Break` cancels.
pub fn build_textures(
    paths: &GamePaths,
    config: &AddonsConfig,
    state_dir: &Path,
    progress: &mut dyn FnMut(Progress) -> ControlFlow<()>,
) -> Result<Stats, AddonError> {
    let id = AddonId::TextureDownscaler;
    let dir = addons_dir(paths);
    let mut record = read_record(state_dir)?;
    let build = build_id(paths)?;
    let previous = owned(&dir, record.installed.get(id.key()))?.cloned();
    let file = match &previous {
        Some(r) => r.file.clone(),
        None => {
            let mut taken = record.installed.values().map(|i| i.file.clone()).collect();
            allocate(id, &dir, &mut taken)?
        }
    };
    let path = dir.join(&file);
    ensure_writable(&path, record.installed.get(id.key()), None)?;
    std::fs::create_dir_all(&dir)?;
    // Our previous build goes first: the writer must start from an empty slot.
    if let Some(r) = &previous {
        for old in std::iter::once(&r.file).chain(&r.chunks) {
            match std::fs::remove_file(dir.join(old)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
                _ => {}
            }
        }
        record.installed.remove(id.key());
        write_record(state_dir, &record)?;
    }
    let stats =
        texture_build::build_texture_addon(&game_vpks(paths), &config.textures, &path, &mut |p| {
            progress(p.into())
        })?;
    let sha256 = sha256_file(&path)?;
    let input = fingerprint(&[
        "textures",
        &textures::fingerprint(&config.textures),
        build.as_deref().unwrap_or(""),
    ]);
    let mut installed = stamp(&path, file.clone(), sha256, input, build)?;
    installed.chunks = chunks_of(&dir, &file);
    record.installed.insert(id.key().to_string(), installed);
    write_record(state_dir, &record)?;
    Ok(stats)
}

/// One state per addon the record knows about; addons never installed are absent.
pub fn installed_state(
    paths: &GamePaths,
    state_dir: &Path,
) -> Result<BTreeMap<AddonId, InstalledState>, AddonError> {
    let dir = addons_dir(paths);
    let record = read_record(state_dir)?;
    let build = build_id(paths)?;
    let mut out = BTreeMap::new();
    for (key, rec) in &record.installed {
        let Some(id) = AddonId::parse(key) else {
            continue;
        };
        let path = dir.join(&rec.file);
        let state = if !path.exists() {
            InstalledState::None
        } else if !is_ours(&path, rec)? {
            InstalledState::Foreign(rec.file.clone())
        } else if info(id).source == Source::Generated && rec.build_id != build {
            InstalledState::Stale(rec.clone())
        } else {
            InstalledState::Current(rec.clone())
        };
        out.insert(id, state);
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::addons::TextureCategory;
    use crate::hud::vpk;

    const CLEAN_GAMEINFO: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../research/configs/OptimizationLock/clean gameinfo.gi/gameinfo.gi"
    ));

    pub fn write_manifest(steamapps: &Path, buildid: &str) {
        let acf = format!(
            "\"AppState\"\n{{\n\t\"appid\"\t\t\"1422450\"\n\t\"buildid\"\t\t\"{buildid}\"\n}}\n"
        );
        std::fs::write(steamapps.join("appmanifest_1422450.acf"), acf).unwrap();
    }

    /// A fake game with a clean gameinfo.gi and a pak01 that carries the vanilla base stylesheet.
    pub fn fake_install(buildid: &str) -> (tempfile::TempDir, GamePaths) {
        let steam = tempfile::tempdir().unwrap();
        let steamapps = steam.path().join("steamapps");
        let game_root = steamapps.join("common").join("Deadlock");
        let citadel = game_root.join("game").join("citadel");
        std::fs::create_dir_all(citadel.join("cfg")).unwrap();
        std::fs::write(citadel.join("gameinfo.gi"), CLEAN_GAMEINFO).unwrap();
        write_manifest(&steamapps, buildid);
        crate::addons::blur::tests::fake_game_pak(&citadel);
        let paths = locate::from_game_root(&game_root).unwrap();
        (steam, paths)
    }

    fn state_dir(steam: &tempfile::TempDir) -> PathBuf {
        steam.path().join("data")
    }

    fn import(state: &Path, dir: &str, file: &str) {
        sources::import(
            &sources::cache_dir(state),
            &sources::tests::research(dir, file),
        )
        .unwrap();
    }

    fn enabled(ids: &[AddonId]) -> AddonsConfig {
        AddonsConfig {
            enabled: ids.iter().copied().collect(),
            ..AddonsConfig::default()
        }
    }

    fn action(plan: &AddonsPlan, id: AddonId) -> &Action {
        &plan
            .get(id)
            .unwrap_or_else(|| panic!("{id:?} in plan"))
            .action
    }

    fn files(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .map(|d| {
                d.filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    #[test]
    fn an_addon_that_fails_to_build_only_blocks_itself() {
        let (steam, paths) = fake_install("1");
        let state = state_dir(&steam);
        let broken = BTreeMap::from([(
            blur::STYLE.to_string(),
            b"not a compiled stylesheet".to_vec(),
        )]);
        std::fs::write(paths.citadel_dir.join(GAME_PAK), vpk::write(&broken)).unwrap();
        import(&state, "Sinner Light Fix Mod", "pak26_dir.vpk");
        let config = enabled(&[AddonId::BlurDisabler, AddonId::SinnerLightFix]);
        let plan = plan(&paths, &config, &state).expect("plan survives one broken addon");
        assert!(matches!(
            action(&plan, AddonId::BlurDisabler),
            Action::Unavailable(Blocker::GameFiles(_))
        ));
        assert!(matches!(
            action(&plan, AddonId::SinnerLightFix),
            Action::Write(_)
        ));
    }

    #[test]
    fn remove_now_deletes_only_our_pak_and_forgets_it() {
        let (steam, paths) = fake_install("1");
        let state = state_dir(&steam);
        assert!(
            !remove_now(AddonId::SinnerLightFix, &paths, &state).unwrap(),
            "nothing installed"
        );
        import(&state, "Sinner Light Fix Mod", "pak26_dir.vpk");
        let config = enabled(&[AddonId::SinnerLightFix]);
        execute(&plan(&paths, &config, &state).unwrap(), &paths, &state).unwrap();
        let dir = addons_dir(&paths);
        std::fs::write(dir.join("pak01_dir.vpk"), b"someone else's mod").unwrap();
        assert!(remove_now(AddonId::SinnerLightFix, &paths, &state).unwrap());
        assert_eq!(
            files(&dir),
            vec!["pak01_dir.vpk".to_string()],
            "foreign mod untouched"
        );
        assert!(installed_state(&paths, &state).unwrap().is_empty());
        assert!(
            !remove_now(AddonId::SinnerLightFix, &paths, &state).unwrap(),
            "idempotent"
        );
    }

    #[test]
    fn remove_now_refuses_a_pak_someone_replaced() {
        let (steam, paths) = fake_install("1");
        let state = state_dir(&steam);
        import(&state, "Sinner Light Fix Mod", "pak26_dir.vpk");
        let config = enabled(&[AddonId::SinnerLightFix]);
        let planned = plan(&paths, &config, &state).unwrap();
        execute(&planned, &paths, &state).unwrap();
        let ours = planned.get(AddonId::SinnerLightFix).unwrap().path.clone();
        std::fs::write(&ours, b"replaced by another tool").unwrap();
        assert!(matches!(
            remove_now(AddonId::SinnerLightFix, &paths, &state),
            Err(AddonError::Foreign(_))
        ));
        assert!(ours.exists());
    }

    #[test]
    fn nothing_enabled_plans_nothing_and_writes_no_record() {
        let (steam, paths) = fake_install("1");
        let state = state_dir(&steam);
        let plan = plan(&paths, &AddonsConfig::default(), &state).unwrap();
        assert_eq!(plan, AddonsPlan::default());
        assert!(!execute(&plan, &paths, &state).unwrap());
        assert!(!state.join(RECORD_FILE).exists());
        assert!(!addons_dir(&paths).exists());
        assert!(installed_state(&paths, &state).unwrap().is_empty());
    }

    #[test]
    fn toggle_addon_needs_its_download_then_installs_as_a_copy_and_is_idempotent() {
        let (steam, paths) = fake_install("1");
        let state = state_dir(&steam);
        let config = enabled(&[AddonId::SinnerLightFix]);
        let first = plan(&paths, &config, &state).unwrap();
        assert_eq!(
            action(&first, AddonId::SinnerLightFix),
            &Action::Unavailable(Blocker::NotDownloaded)
        );
        assert!(first.is_empty());
        assert!(!first.needs_search_path, "nothing to mount yet");

        import(&state, "Sinner Light Fix Mod", "pak26_dir.vpk");
        let second = plan(&paths, &config, &state).unwrap();
        let entry = second.get(AddonId::SinnerLightFix).unwrap();
        assert!(matches!(entry.action, Action::Write(Build::Copy(_))));
        assert_eq!(entry.path, addons_dir(&paths).join("pak73_dir.vpk"));
        assert!(second.needs_search_path);
        assert!(execute(&second, &paths, &state).unwrap());
        let upstream = std::fs::read(sources::tests::research(
            "Sinner Light Fix Mod",
            "pak26_dir.vpk",
        ))
        .unwrap();
        assert_eq!(std::fs::read(&entry.path).unwrap(), upstream);

        let third = plan(&paths, &config, &state).unwrap();
        assert_eq!(action(&third, AddonId::SinnerLightFix), &Action::Keep);
        assert!(third.is_empty());
        assert!(!execute(&third, &paths, &state).unwrap());
        assert!(matches!(
            installed_state(&paths, &state).unwrap()[&AddonId::SinnerLightFix],
            InstalledState::Current(_)
        ));
        let record = read_record(&state).unwrap();
        assert_eq!(record.installed["sinner_light_fix"].file, "pak73_dir.vpk");
        assert_eq!(
            record.installed["sinner_light_fix"].build_id.as_deref(),
            Some("1")
        );
    }

    #[test]
    fn disabling_removes_only_our_file_and_the_record_entry() {
        let (steam, paths) = fake_install("1");
        let state = state_dir(&steam);
        import(&state, "Vindicta Scope Downscale", "pak89_dir.vpk");
        let dir = addons_dir(&paths);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("pak01_dir.vpk"), b"qol lite").unwrap();
        let on = enabled(&[AddonId::VindictaScope]);
        execute(&plan(&paths, &on, &state).unwrap(), &paths, &state).unwrap();
        assert_eq!(files(&dir), ["pak01_dir.vpk", "pak74_dir.vpk"]);

        let off = plan(&paths, &AddonsConfig::default(), &state).unwrap();
        assert_eq!(action(&off, AddonId::VindictaScope), &Action::Remove);
        assert!(execute(&off, &paths, &state).unwrap());
        assert_eq!(files(&dir), ["pak01_dir.vpk"]);
        assert!(read_record(&state).unwrap().installed.is_empty());
        assert!(
            plan(&paths, &AddonsConfig::default(), &state)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn preferred_slot_taken_by_another_mod_moves_to_the_first_free_number() {
        let (steam, paths) = fake_install("1");
        let state = state_dir(&steam);
        import(&state, "Sinner Light Fix Mod", "pak26_dir.vpk");
        import(&state, "Vindicta Scope Downscale", "pak89_dir.vpk");
        let dir = addons_dir(&paths);
        std::fs::create_dir_all(&dir).unwrap();
        for taken in ["pak73_dir.vpk", "pak70_dir.vpk", HUD_ADDON_FILE] {
            std::fs::write(dir.join(taken), b"someone else").unwrap();
        }
        let config = enabled(&[AddonId::SinnerLightFix, AddonId::VindictaScope]);
        let plan = plan(&paths, &config, &state).unwrap();
        assert_eq!(
            plan.get(AddonId::SinnerLightFix).unwrap().path,
            dir.join("pak71_dir.vpk")
        );
        assert_eq!(
            plan.get(AddonId::VindictaScope).unwrap().path,
            dir.join("pak74_dir.vpk")
        );
        execute(&plan, &paths, &state).unwrap();
        assert_eq!(
            std::fs::read(dir.join("pak73_dir.vpk")).unwrap(),
            b"someone else"
        );
        assert_eq!(
            std::fs::read(dir.join(HUD_ADDON_FILE)).unwrap(),
            b"someone else"
        );
    }

    #[test]
    fn a_tampered_file_is_foreign_and_never_overwritten_or_deleted() {
        let (steam, paths) = fake_install("1");
        let state = state_dir(&steam);
        import(&state, "Sinner Light Fix Mod", "pak26_dir.vpk");
        let config = enabled(&[AddonId::SinnerLightFix]);
        execute(&plan(&paths, &config, &state).unwrap(), &paths, &state).unwrap();
        let ours = addons_dir(&paths).join("pak73_dir.vpk");
        std::fs::write(&ours, b"replaced by another mod manager").unwrap();
        assert_eq!(
            installed_state(&paths, &state).unwrap()[&AddonId::SinnerLightFix],
            InstalledState::Foreign("pak73_dir.vpk".into())
        );

        let again = plan(&paths, &config, &state).unwrap();
        assert_eq!(
            again.get(AddonId::SinnerLightFix).unwrap().path,
            addons_dir(&paths).join("pak70_dir.vpk"),
            "a new slot, the foreign file stays"
        );
        execute(&again, &paths, &state).unwrap();
        assert_eq!(
            std::fs::read(&ours).unwrap(),
            b"replaced by another mod manager"
        );

        let off = plan(&paths, &AddonsConfig::default(), &state).unwrap();
        execute(&off, &paths, &state).unwrap();
        assert_eq!(files(&addons_dir(&paths)), ["pak73_dir.vpk"]);
    }

    #[test]
    fn particle_pak_holds_exactly_the_hidden_groups_and_all_kept_means_removal() {
        let (steam, paths) = fake_install("1");
        let state = state_dir(&steam);
        import(&state, "Screenspace Particle Disabler", "pak02_dir.vpk");
        let mut config = enabled(&[AddonId::ParticleDisabler]);
        for g in particles::GROUPS {
            if g.id != "low_health" && g.id != "inferno" {
                config.keep_particles.insert(g.id.to_string());
            }
        }
        let first = plan(&paths, &config, &state).unwrap();
        let entry = first.get(AddonId::ParticleDisabler).unwrap();
        assert_eq!(entry.path, addons_dir(&paths).join("pak71_dir.vpk"));
        execute(&first, &paths, &state).unwrap();
        let ours = VpkDir::open(&entry.path).unwrap();
        let mut want: Vec<&str> = particles::hidden_paths(&config.keep_particles);
        want.push(particles::DEBUG_TEXTURE);
        want.sort_unstable();
        let got: Vec<&str> = ours.entries.keys().map(String::as_str).collect();
        assert_eq!(got, want);
        assert_eq!(got.len(), 4 + 4 + 1);

        config.keep_particles.remove("shiv");
        let second = plan(&paths, &config, &state).unwrap();
        assert!(matches!(
            action(&second, AddonId::ParticleDisabler),
            Action::Write(Build::Bytes(_))
        ));
        execute(&second, &paths, &state).unwrap();
        assert_eq!(
            VpkDir::open(&entry.path).unwrap().entries.len(),
            4 + 4 + 5 + 1
        );
        assert!(plan(&paths, &config, &state).unwrap().is_empty());

        config.keep_particles = particles::GROUPS.iter().map(|g| g.id.to_string()).collect();
        let none = plan(&paths, &config, &state).unwrap();
        assert_eq!(action(&none, AddonId::ParticleDisabler), &Action::Remove);
        execute(&none, &paths, &state).unwrap();
        assert!(!entry.path.exists());
    }

    #[test]
    fn blur_is_generated_from_the_game_pak_and_goes_stale_on_update() {
        let (steam, paths) = fake_install("100");
        let state = state_dir(&steam);
        let config = enabled(&[AddonId::BlurDisabler]);
        let first = plan(&paths, &config, &state).unwrap();
        let entry = first.get(AddonId::BlurDisabler).unwrap();
        let Action::Write(Build::Bytes(bytes)) = &entry.action else {
            panic!("{:?}", entry.action);
        };
        let pak = VpkDir::open(&paths.citadel_dir.join(GAME_PAK)).unwrap();
        execute(&first, &paths, &state).unwrap();
        assert_eq!(std::fs::read(&entry.path).unwrap(), *bytes);
        assert_eq!(
            VpkDir::open(&entry.path)
                .unwrap()
                .read(blur::STYLE)
                .unwrap(),
            blur::build(&pak, &config.blur).unwrap()
        );
        assert!(matches!(
            installed_state(&paths, &state).unwrap()[&AddonId::BlurDisabler],
            InstalledState::Current(_)
        ));

        write_manifest(&steam.path().join("steamapps"), "101");
        assert!(matches!(
            installed_state(&paths, &state).unwrap()[&AddonId::BlurDisabler],
            InstalledState::Stale(_)
        ));
        let rebuilt = plan(&paths, &config, &state).unwrap();
        assert!(matches!(
            action(&rebuilt, AddonId::BlurDisabler),
            Action::Write(_)
        ));
        execute(&rebuilt, &paths, &state).unwrap();
        assert!(plan(&paths, &config, &state).unwrap().is_empty());

        let mut menu_only = config.clone();
        menu_only.blur.hud = false;
        assert!(matches!(
            action(
                &plan(&paths, &menu_only, &state).unwrap(),
                AddonId::BlurDisabler
            ),
            Action::Write(_)
        ));
    }

    #[test]
    fn blur_without_game_files_is_blocked_not_an_error() {
        let (steam, paths) = fake_install("1");
        std::fs::remove_file(paths.citadel_dir.join(GAME_PAK)).unwrap();
        let plan = plan(
            &paths,
            &enabled(&[AddonId::BlurDisabler]),
            &state_dir(&steam),
        )
        .unwrap();
        assert!(matches!(
            action(&plan, AddonId::BlurDisabler),
            Action::Unavailable(Blocker::GameFiles(_))
        ));
    }

    #[test]
    fn deleting_our_file_by_hand_reinstalls_and_forgets_the_old_entry() {
        let (steam, paths) = fake_install("1");
        let state = state_dir(&steam);
        import(&state, "Sinner Light Fix Mod", "pak26_dir.vpk");
        let config = enabled(&[AddonId::SinnerLightFix]);
        let first = plan(&paths, &config, &state).unwrap();
        execute(&first, &paths, &state).unwrap();
        let ours = first.get(AddonId::SinnerLightFix).unwrap().path.clone();
        std::fs::remove_file(&ours).unwrap();
        assert_eq!(
            installed_state(&paths, &state).unwrap()[&AddonId::SinnerLightFix],
            InstalledState::None
        );
        let again = plan(&paths, &config, &state).unwrap();
        assert!(matches!(
            action(&again, AddonId::SinnerLightFix),
            Action::Write(_)
        ));
        assert_eq!(again.get(AddonId::SinnerLightFix).unwrap().path, ours);
        execute(&again, &paths, &state).unwrap();
        assert!(ours.exists());

        std::fs::remove_file(&ours).unwrap();
        execute(
            &plan(&paths, &AddonsConfig::default(), &state).unwrap(),
            &paths,
            &state,
        )
        .unwrap();
        assert!(read_record(&state).unwrap().installed.is_empty());
    }

    #[test]
    fn conflicts_name_the_other_addon_and_the_shared_paths() {
        let (steam, paths) = fake_install("1");
        let state = state_dir(&steam);
        let dir = addons_dir(&paths);
        std::fs::create_dir_all(&dir).unwrap();
        let other = BTreeMap::from([
            (blur::STYLE.to_string(), b"their stylesheet".to_vec()),
            ("panorama/styles/hud.vcss_c".to_string(), b"x".to_vec()),
        ]);
        std::fs::write(dir.join("pak03_dir.vpk"), vpk::write(&other)).unwrap();
        std::fs::write(dir.join("pak05_dir.vpk"), b"unreadable").unwrap();
        let plan = plan(&paths, &enabled(&[AddonId::BlurDisabler]), &state).unwrap();
        assert_eq!(
            plan.conflicts,
            vec![Conflict {
                id: AddonId::BlurDisabler,
                addon: dir.join("pak03_dir.vpk"),
                paths: vec![blur::STYLE.to_string()],
            }]
        );
    }

    #[test]
    fn search_path_is_wanted_until_gameinfo_mounts_addons() {
        let (steam, paths) = fake_install("1");
        let state = state_dir(&steam);
        let config = enabled(&[AddonId::BlurDisabler]);
        assert!(plan(&paths, &config, &state).unwrap().needs_search_path);
        let mounted = searchpaths::ensure_addons(CLEAN_GAMEINFO).unwrap();
        std::fs::write(&paths.gameinfo, mounted).unwrap();
        assert!(!plan(&paths, &config, &state).unwrap().needs_search_path);
    }

    fn game_with_textures(paths: &GamePaths) {
        use crate::texture::vtex::tests::{COLOR, MASK};
        let files = BTreeMap::from([
            ("models/props/a/color.vtex_c".to_string(), COLOR.to_vec()),
            ("models/heroes/h/mask.vtex_c".to_string(), MASK.to_vec()),
            ("panorama/images/x.vtex_c".to_string(), MASK.to_vec()),
        ]);
        std::fs::write(paths.citadel_dir.join("pak02_dir.vpk"), vpk::write(&files)).unwrap();
    }

    #[test]
    fn textures_are_built_on_demand_and_rebuilt_when_the_config_changes() {
        let (steam, paths) = fake_install("1");
        let state = state_dir(&steam);
        game_with_textures(&paths);
        let mut config = enabled(&[AddonId::TextureDownscaler]);
        let first = plan(&paths, &config, &state).unwrap();
        assert_eq!(action(&first, AddonId::TextureDownscaler), &Action::Build);
        assert!(first.is_empty(), "Apply never builds textures itself");
        assert!(!execute(&first, &paths, &state).unwrap());
        let ours = addons_dir(&paths).join("pak76_dir.vpk");
        assert!(!ours.exists());

        let cancelled =
            build_textures(&paths, &config, &state, &mut |_| ControlFlow::Break(())).unwrap_err();
        assert!(matches!(
            cancelled,
            AddonError::TextureBuild(crate::texture::AddonError::Cancelled)
        ));
        assert!(read_record(&state).unwrap().installed.is_empty());
        assert!(!ours.exists());

        let mut seen = Vec::new();
        let stats = build_textures(&paths, &config, &state, &mut |p| {
            seen.push(p);
            ControlFlow::Continue(())
        })
        .unwrap();
        assert_eq!(stats.reduced, 2, "{stats:?}");
        assert_eq!(seen.len(), 3);
        assert_eq!(seen.last().unwrap().stats, stats);
        let addon = VpkDir::open(&ours).unwrap();
        assert_eq!(addon.entries.len(), 2);
        assert!(addon.contains("models/props/a/color.vtex_c"));
        assert_eq!(
            action(
                &plan(&paths, &config, &state).unwrap(),
                AddonId::TextureDownscaler
            ),
            &Action::Keep
        );
        assert_eq!(
            read_record(&state).unwrap().installed["texture_downscaler"].chunks,
            Vec::<String>::new()
        );

        config.textures.categories.insert(TextureCategory::Ui);
        assert_eq!(
            action(
                &plan(&paths, &config, &state).unwrap(),
                AddonId::TextureDownscaler
            ),
            &Action::Build
        );
        let stats =
            build_textures(&paths, &config, &state, &mut |_| ControlFlow::Continue(())).unwrap();
        assert_eq!(stats.reduced, 3);
        assert_eq!(VpkDir::open(&ours).unwrap().entries.len(), 3);
        assert_eq!(files(&addons_dir(&paths)), ["pak76_dir.vpk"]);

        let off = plan(&paths, &AddonsConfig::default(), &state).unwrap();
        assert_eq!(action(&off, AddonId::TextureDownscaler), &Action::Remove);
        execute(&off, &paths, &state).unwrap();
        assert!(!ours.exists());
    }

    #[test]
    fn chunk_files_are_recorded_removed_and_block_a_slot() {
        let (steam, paths) = fake_install("1");
        let state = state_dir(&steam);
        let dir = addons_dir(&paths);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("pak73_000.vpk"), b"someone's chunk").unwrap();
        import(&state, "Sinner Light Fix Mod", "pak26_dir.vpk");
        let plan = plan(&paths, &enabled(&[AddonId::SinnerLightFix]), &state).unwrap();
        assert_eq!(
            plan.get(AddonId::SinnerLightFix).unwrap().path,
            dir.join("pak70_dir.vpk")
        );

        let mut record = read_record(&state).unwrap();
        std::fs::write(dir.join("pak76_dir.vpk"), b"dir").unwrap();
        std::fs::write(dir.join("pak76_000.vpk"), b"chunk").unwrap();
        let mut rec = stamp(
            &dir.join("pak76_dir.vpk"),
            "pak76_dir.vpk".into(),
            sha256_hex(b"dir"),
            "x".into(),
            None,
        )
        .unwrap();
        rec.chunks = chunks_of(&dir, "pak76_dir.vpk");
        assert_eq!(rec.chunks, ["pak76_000.vpk"]);
        record.installed.insert("texture_downscaler".into(), rec);
        write_record(&state, &record).unwrap();
        let off = super::plan(&paths, &AddonsConfig::default(), &state).unwrap();
        assert_eq!(action(&off, AddonId::TextureDownscaler), &Action::Remove);
        execute(&off, &paths, &state).unwrap();
        assert_eq!(files(&dir), ["pak73_000.vpk"]);
    }

    #[test]
    fn installed_copy_stays_while_its_source_is_gone() {
        let (steam, paths) = fake_install("1");
        let state = state_dir(&steam);
        import(&state, "Sinner Light Fix Mod", "pak26_dir.vpk");
        let config = enabled(&[AddonId::SinnerLightFix]);
        execute(&plan(&paths, &config, &state).unwrap(), &paths, &state).unwrap();
        std::fs::remove_dir_all(sources::cache_dir(&state)).unwrap();
        let plan = plan(&paths, &config, &state).unwrap();
        assert_eq!(action(&plan, AddonId::SinnerLightFix), &Action::Keep);
        assert!(plan.is_empty());
    }
}
