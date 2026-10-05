//! Practice mode: `SceneSystem` keys that still cut GPU work after 2026-09, when the engine
//! started ignoring the equivalent ConVars set in gameinfo.gi. Learned from the SideLock
//! config (GameBanana 722944). The game's matchmaking check refuses to queue with changes to
//! the sections in [`MATCHMAKING_SECTIONS`], so this is for bots, sandbox and unranked play;
//! ranked-safe puts the stock values back.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::Path;

use crate::backup::atomic_write;
use crate::gi::GiError;
use crate::gi_sections::{self, KeyEdit, SectionEdits};

pub const SECTION: &str = "SceneSystem";

/// Sections whose edits trip `Citadel_StartMatchmaking_UnverifiedPGI`.
pub const MATCHMAKING_SECTIONS: [&str; 7] = [
    "Engine2",
    "MaterialSystem2",
    "NetworkSystem",
    "Particles",
    "RenderSystem",
    "SceneSystem",
    "WorldRenderer",
];

/// Valve's gameinfo.gi as shipped on 2026-09-29: the stock values the drift check compares
/// against, and the source of every `stock` in [`KEYS`].
pub const STOCK: &str = include_str!("../tests/fixtures/gameinfo_live_2026-09-29.gi");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Group {
    Shadows,
    Fog,
    Batching,
}

impl Group {
    pub const ALL: [Group; 3] = [Group::Shadows, Group::Fog, Group::Batching];

    pub fn id(self) -> &'static str {
        match self {
            Group::Shadows => "shadows",
            Group::Fog => "fog",
            Group::Batching => "batching",
        }
    }
}

pub struct ManagedKey {
    pub key: &'static str,
    pub group: Group,
    pub practice: &'static str,
    /// `None` when Valve's file does not set the key: turning the group off removes the line.
    pub stock: Option<&'static str>,
}

const fn key(
    key: &'static str,
    group: Group,
    practice: &'static str,
    stock: Option<&'static str>,
) -> ManagedKey {
    ManagedKey {
        key,
        group,
        practice,
        stock,
    }
}

pub const KEYS: [ManagedKey; 13] = [
    key(
        "FogCachedShadowAtlasWidth",
        Group::Shadows,
        "0",
        Some("2048"),
    ),
    key(
        "FogCachedShadowAtlasHeight",
        Group::Shadows,
        "0",
        Some("2048"),
    ),
    key("FogCachedShadowTileSize", Group::Shadows, "0", Some("128")),
    key("CSMCascadeResolution", Group::Shadows, "0", Some("2048")),
    key(
        "DefaultShadowTextureWidth",
        Group::Shadows,
        "0",
        Some("6144"),
    ),
    key(
        "DefaultShadowTextureHeight",
        Group::Shadows,
        "0",
        Some("6144"),
    ),
    key("DynamicShadowResolution", Group::Shadows, "0", Some("1")),
    key("NonTexturedGradientFog", Group::Fog, "0", Some("1")),
    key("CubemapFog", Group::Fog, "0", Some("1")),
    key("VolumetricFog", Group::Fog, "0", Some("1")),
    key("LayerBatchThresholdFullsort", Group::Batching, "20", None),
    key(
        "DisableLateAllocatedTransformBuffer",
        Group::Batching,
        "1",
        None,
    ),
    key(
        "MinimumLateAllocatedVertexCacheBufferSizeMB",
        Group::Batching,
        "64",
        None,
    ),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct PracticeMode {
    #[serde(default)]
    pub shadows: bool,
    #[serde(default)]
    pub fog: bool,
    #[serde(default)]
    pub batching: bool,
}

impl PracticeMode {
    pub const ALL_ON: PracticeMode = PracticeMode {
        shadows: true,
        fog: true,
        batching: true,
    };

    pub fn is_off(&self) -> bool {
        *self == PracticeMode::default()
    }

    pub fn any(self) -> bool {
        !self.is_off()
    }

    pub fn get(self, group: Group) -> bool {
        match group {
            Group::Shadows => self.shadows,
            Group::Fog => self.fog,
            Group::Batching => self.batching,
        }
    }

    pub fn set(&mut self, group: Group, on: bool) {
        match group {
            Group::Shadows => self.shadows = on,
            Group::Fog => self.fog = on,
            Group::Batching => self.batching = on,
        }
    }

    pub fn groups_on(self) -> Vec<Group> {
        Group::ALL.into_iter().filter(|g| self.get(*g)).collect()
    }
}

pub const RECORD_FILE: &str = "practice.toml";

/// What each managed key held before DeadTune wrote its practice value, so turning a group
/// off puts that back. Keys DeadTune never wrote are not here and are never touched.
/// Lives at `<data dir>/practice.toml` next to the other records.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Record {
    #[serde(default)]
    pub values: BTreeMap<String, String>,
    /// Keys the file did not have before DeadTune added them.
    #[serde(default)]
    pub absent: BTreeSet<String>,
}

impl Record {
    /// `Some(Some(v))` was present, `Some(None)` was absent, `None` was never written by us.
    pub fn prior(&self, key: &str) -> Option<Option<&str>> {
        if self.absent.contains(key) {
            return Some(None);
        }
        self.values.get(key).map(|v| Some(v.as_str()))
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty() && self.absent.is_empty()
    }

    fn remember(&mut self, key: &str, prior: Option<&str>) {
        match prior {
            Some(v) => {
                self.values.insert(key.to_string(), v.to_string());
            }
            None => {
                self.absent.insert(key.to_string());
            }
        }
    }

    fn forget(&mut self, key: &str) {
        self.values.remove(key);
        self.absent.remove(key);
    }

    /// A missing file is an empty record.
    pub fn load(root: &Path) -> io::Result<Record> {
        match std::fs::read_to_string(root.join(RECORD_FILE)) {
            Ok(text) => toml::from_str(&text).map_err(io::Error::other),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Record::default()),
            Err(e) => Err(e),
        }
    }

    /// An empty record removes the file.
    pub fn save(&self, root: &Path) -> io::Result<()> {
        let path = root.join(RECORD_FILE);
        if self.is_empty() {
            return match std::fs::remove_file(&path) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                other => other,
            };
        }
        std::fs::create_dir_all(root)?;
        let text = toml::to_string(self).map_err(io::Error::other)?;
        atomic_write(&path, text.as_bytes())
    }
}

/// `mode` written into the SceneSystem section, and the record to keep once it is on disk.
/// A group that is on gets its practice values, remembering what each key held unless the
/// file already had the practice value (then it is the player's own and stays theirs). A
/// group that is off puts back what the record holds for its keys and forgets them; keys
/// with no record are left exactly as they are. A file without the section is left alone
/// when the mode is off and refused when it is on.
pub fn plan(text: &str, mode: PracticeMode, record: &Record) -> Result<(String, Record), GiError> {
    let Some(live) = gi_sections::section_values(text, SECTION)? else {
        if mode.is_off() {
            return Ok((text.to_string(), record.clone()));
        }
        return Err(GiError::NoSection(SECTION.to_string()));
    };
    let mut next = record.clone();
    let mut edits = SectionEdits::new();
    for k in &KEYS {
        let current = live.get(k.key).map(String::as_str);
        if mode.get(k.group) {
            if next.prior(k.key).is_none() && current != Some(k.practice) {
                next.remember(k.key, current);
            }
            edits.insert(k.key.to_string(), KeyEdit::Set(k.practice.to_string()));
        } else if let Some(prior) = next.prior(k.key) {
            let edit = prior.map_or(KeyEdit::Remove, |v| KeyEdit::Set(v.to_string()));
            edits.insert(k.key.to_string(), edit);
            next.forget(k.key);
        }
    }
    Ok((gi_sections::edit_section(text, SECTION, &edits)?, next))
}

/// Stock values for every managed key, whoever wrote them: the ranked-safe action.
pub fn restore_stock(text: &str) -> Result<String, GiError> {
    if gi_sections::section_values(text, SECTION)?.is_none() {
        return Ok(text.to_string());
    }
    let edits: SectionEdits = KEYS
        .iter()
        .map(|k| {
            let edit = k
                .stock
                .map_or(KeyEdit::Remove, |stock| KeyEdit::Set(stock.to_string()));
            (k.key.to_string(), edit)
        })
        .collect();
    gi_sections::edit_section(text, SECTION, &edits)
}

/// A group reads as on when every one of its keys holds the practice value.
pub fn detect(text: &str) -> Result<PracticeMode, GiError> {
    let values = gi_sections::section_values(text, SECTION)?.unwrap_or_default();
    let mut mode = PracticeMode::default();
    for group in Group::ALL {
        let on = KEYS
            .iter()
            .filter(|k| k.group == group)
            .all(|k| values.get(k.key).map(String::as_str) == Some(k.practice));
        mode.set(group, on);
    }
    Ok(mode)
}

/// A scalar in a matchmaking-sensitive section that is not what the stock file holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Drift {
    pub section: &'static str,
    pub key: String,
    pub stock: Option<String>,
    pub live: Option<String>,
}

impl Drift {
    /// One of the keys practice mode owns, as opposed to another tool's leftover.
    pub fn managed(&self) -> bool {
        self.section == SECTION && KEYS.iter().any(|k| k.key == self.key)
    }
}

/// Every difference in [`MATCHMAKING_SECTIONS`] between `live` and [`STOCK`], in section order.
pub fn matchmaking_drift(live: &str) -> Result<Vec<Drift>, GiError> {
    drift(live, STOCK)
}

pub fn drift(live: &str, stock: &str) -> Result<Vec<Drift>, GiError> {
    let mut out = Vec::new();
    for section in MATCHMAKING_SECTIONS {
        let live_values = gi_sections::section_values(live, section)?.unwrap_or_default();
        let stock_values = gi_sections::section_values(stock, section)?.unwrap_or_default();
        let keys: std::collections::BTreeSet<&String> =
            live_values.keys().chain(stock_values.keys()).collect();
        for key in keys {
            let (l, s) = (live_values.get(key), stock_values.get(key));
            if l != s {
                out.push(Drift {
                    section,
                    key: key.clone(),
                    stock: s.cloned(),
                    live: l.cloned(),
                });
            }
        }
    }
    Ok(out)
}
