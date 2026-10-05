//! Practice mode: `SceneSystem` keys that still cut GPU work after 2026-09, when the engine
//! started ignoring the equivalent ConVars set in gameinfo.gi. Learned from the SideLock
//! config (GameBanana 722944). The game's matchmaking check refuses to queue with changes to
//! the sections in [`MATCHMAKING_SECTIONS`], so this is for bots, sandbox and unranked play;
//! ranked-safe puts the stock values back.

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

/// Every managed key: its practice value for a group that is on, its stock value otherwise.
pub fn edits(mode: PracticeMode) -> SectionEdits {
    KEYS.iter()
        .map(|k| {
            let edit = match (mode.get(k.group), k.stock) {
                (true, _) => KeyEdit::Set(k.practice.to_string()),
                (false, Some(stock)) => KeyEdit::Set(stock.to_string()),
                (false, None) => KeyEdit::Remove,
            };
            (k.key.to_string(), edit)
        })
        .collect()
}

/// Writes `mode` into the SceneSystem section. A file without one is left alone when the mode
/// is off (nothing to restore) and refused when it is on.
pub fn apply(text: &str, mode: PracticeMode) -> Result<String, GiError> {
    if mode.is_off() && gi_sections::section_values(text, SECTION)?.is_none() {
        return Ok(text.to_string());
    }
    gi_sections::edit_section(text, SECTION, &edits(mode))
}

/// Stock values for every managed key: one call back to queueable.
pub fn restore_stock(text: &str) -> Result<String, GiError> {
    apply(text, PracticeMode::default())
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
