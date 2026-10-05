//! Snapshots of the game files DeadTune depends on: the HUD, settings and menu layouts,
//! stylesheets and scripts out of `pak01_dir.vpk`, plus `gameinfo.gi` and the cfg files,
//! copied into a dated folder under the data dir with decoded text and a manifest of
//! sizes and CRCs. Two snapshots diff into a report that names the DeadTune feature each
//! changed file feeds. Plan: docs/plans/game-files/plan.md.
//!
//! Pipeline: `spec::Inventory::scan` (the pak tree classified by `Category`) ->
//! `export::take` (raw bytes, `decode` to text, `store::Manifest`) -> `diff::compare`.

use std::path::PathBuf;

use crate::addons::{ADDONS, AddonId, Kind};
use crate::hud::apples_tunnels::MINIMAP_LAYOUT;
use crate::hud::elements::HUD_STYLE;
use crate::hud::health_style::{HEALTH_CONTAINER_STYLE, HEALTH_STYLE};
use crate::hud::minimap_colors::MINIMAP_STYLE;
use crate::hud::topbar::{TOP_BAR_LAYOUT, TOP_BAR_STYLE};
use crate::hud::vpk::VpkError;

pub mod decode;
pub mod diff;
pub mod export;
pub mod images;
pub mod spec;
pub mod store;

pub use diff::{ChangeKind, Counts, FileChange, SnapshotDiff, compare};
pub use export::{Outcome, Progress, take};
pub use spec::{Candidate, Category, ImageScope, Inventory, Selection, Source};
pub use store::{Decoded, FileEntry, ImageInfo, Manifest, SnapshotInfo, Stored};

/// The settings menu layout the in-game settings plan rebuilds rows into
/// (docs/plans/ingame-settings/plan.md, Phase 2).
pub const SETTINGS_LAYOUT: &str = "panorama/layout/popups/popup_settings.vxml_c";
/// Loose files, relative to `game/citadel`.
pub const GAMEINFO: &str = "gameinfo.gi";
pub const VIDEO: &str = "cfg/video.txt";

#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Vpk(#[from] VpkError),
    #[error("toml: {0}")]
    Toml(String),
    #[error("game archive {0} not found; is the game fully installed?")]
    MissingGamePak(PathBuf),
    #[error("cancelled")]
    Cancelled,
    #[error("no snapshot named {0}")]
    NoSuch(String),
    #[error("{0} is not a snapshot folder (no manifest.toml)")]
    NotASnapshot(PathBuf),
    #[error("json: {0}")]
    Json(String),
    #[error("no images under {0}")]
    NoImages(String),
}

/// A DeadTune feature that reads a game file. A change to that file is the first thing
/// to check after an update.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Feature {
    HudLayout,
    Minimap,
    TopBar,
    Health,
    Addon(AddonId),
    /// Planned: rows added to the game's own settings menu.
    SettingsRows,
    GameInfo,
    Video,
}

impl Feature {
    pub fn label(self) -> String {
        match self {
            Feature::HudLayout => "HUD layout".into(),
            Feature::Minimap => "Minimap (colours, apples and tunnels)".into(),
            Feature::TopBar => "Top bar".into(),
            Feature::Health => "Health bar".into(),
            Feature::Addon(id) => ADDONS
                .iter()
                .find(|a| a.id == id)
                .map(|a| a.name.to_string())
                .unwrap_or_else(|| id.key().to_string()),
            Feature::SettingsRows => "In-game settings rows (planned)".into(),
            Feature::GameInfo => "Game settings (gameinfo.gi)".into(),
            Feature::Video => "Video settings (video.txt)".into(),
        }
    }
}

/// Every game file a DeadTune feature reads, from the same constants the generators use:
/// the HUD stylesheets and layouts, each native addon's `game_files`, the settings layout
/// and the two loose files the ConVar editor writes.
pub fn dependencies() -> Vec<(Feature, Vec<&'static str>)> {
    let mut deps = vec![
        (Feature::HudLayout, vec![HUD_STYLE]),
        (Feature::Minimap, vec![MINIMAP_STYLE, MINIMAP_LAYOUT]),
        (Feature::TopBar, vec![TOP_BAR_STYLE, TOP_BAR_LAYOUT]),
        (Feature::Health, vec![HEALTH_STYLE, HEALTH_CONTAINER_STYLE]),
        (Feature::SettingsRows, vec![SETTINGS_LAYOUT]),
        (Feature::GameInfo, vec![GAMEINFO]),
        (Feature::Video, vec![VIDEO]),
    ];
    for info in &ADDONS {
        if let Kind::Native(native) = info.kind {
            deps.push((Feature::Addon(info.id), native.game_files().to_vec()));
        }
    }
    deps
}

/// "1.2 MB", "340 KB", "12 bytes".
pub fn human_bytes(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    if bytes >= 1 << 20 {
        format!("{:.1} MB", bytes as f64 / MB)
    } else if bytes >= 1 << 10 {
        format!("{} KB", bytes / 1024)
    } else {
        format!("{bytes} bytes")
    }
}

/// The features that read `path`, in table order.
pub fn features_of(path: &str) -> Vec<Feature> {
    dependencies()
        .into_iter()
        .filter(|(_, paths)| paths.contains(&path))
        .map(|(feature, _)| feature)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::native::Native;

    #[test]
    fn every_native_addon_and_hud_file_is_a_dependency() {
        let deps = dependencies();
        let paths: Vec<&str> = deps.iter().flat_map(|(_, p)| p.iter().copied()).collect();
        for native in [
            Native::Particles,
            Native::Blur,
            Native::Sinner,
            Native::Scope,
            Native::Clutter,
        ] {
            for path in native.game_files() {
                assert!(paths.contains(path), "{native:?} reads {path}");
            }
        }
        for path in [
            HUD_STYLE,
            MINIMAP_LAYOUT,
            TOP_BAR_LAYOUT,
            HEALTH_STYLE,
            GAMEINFO,
        ] {
            assert!(paths.contains(&path), "{path}");
        }
        let particle = features_of(crate::addons::native_particles::EMPTY_PARTICLE);
        assert_eq!(
            particle,
            [
                Feature::Addon(AddonId::ParticleDisabler),
                Feature::Addon(AddonId::ClutterRemover)
            ]
        );
        assert_eq!(features_of(TOP_BAR_LAYOUT), [Feature::TopBar]);
        assert!(features_of("panorama/layout/hud.vxml_c").is_empty());
        assert_eq!(
            Feature::Addon(AddonId::BlurDisabler).label(),
            "UI blur disabler"
        );
    }

    #[test]
    fn human_sizes() {
        assert_eq!(human_bytes(12), "12 bytes");
        assert_eq!(human_bytes(340 * 1024 + 7), "340 KB");
        assert_eq!(human_bytes(1_258_291), "1.2 MB");
    }
}
