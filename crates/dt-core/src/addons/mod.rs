//! Performance addons: the community VPKs from Sqooky's OptimizationLock bundle
//! ("Various Addons Relating to Performance", GameBanana mod 656341), rebuilt as
//! DeadTune-owned pak files in `citadel/addons`.
//!
//! Every addon is one row in [`ADDONS`]. Its [`Kind`] says how our pak file is made:
//! a verbatim copy of the upstream file, a generated stylesheet from the installed
//! game's own files, the upstream particle stub at only the paths the player picks,
//! or a filtered copy of a large upstream archive. Its [`Source`] says where the
//! upstream bytes come from. [`AddonsConfig`] is the player's choice and lives in the
//! profile; [`install`] turns it into pak files and keeps an ownership record so no
//! file DeadTune did not write is ever touched.

use std::collections::BTreeSet;
use std::path::PathBuf;

pub mod blur;
pub mod guard;
pub mod install;
pub mod native_blur;
pub mod particles;
pub mod sources;
pub mod textures;
pub mod verify;

pub use crate::texture::{
    Category as TextureCategory, Factor, Stats as TextureStats, TextureDownscale,
};
pub use install::{Action, AddonPlan, AddonsPlan, Blocker, Build, Conflict, InstalledState};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum AddonId {
    ParticleDisabler,
    BlurDisabler,
    SinnerLightFix,
    VindictaScope,
    SoulContainer,
    TextureDownscaler,
}

impl AddonId {
    pub const ALL: [AddonId; 6] = [
        AddonId::ParticleDisabler,
        AddonId::BlurDisabler,
        AddonId::SinnerLightFix,
        AddonId::VindictaScope,
        AddonId::SoulContainer,
        AddonId::TextureDownscaler,
    ];

    /// Same string serde uses; the CLI id and the cache folder name.
    pub fn key(self) -> &'static str {
        match self {
            AddonId::ParticleDisabler => "particle_disabler",
            AddonId::BlurDisabler => "blur_disabler",
            AddonId::SinnerLightFix => "sinner_light_fix",
            AddonId::VindictaScope => "vindicta_scope",
            AddonId::SoulContainer => "soul_container",
            AddonId::TextureDownscaler => "texture_downscaler",
        }
    }

    pub fn parse(text: &str) -> Option<AddonId> {
        AddonId::ALL.into_iter().find(|id| id.key() == text)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Our pak is the upstream file, byte for byte.
    Toggle,
    /// Our pak holds the upstream empty-particle stub at the paths the player hides.
    ParticleGroups,
    /// The upstream file by default; with [`BlurOptions::rebuild`] on, a stylesheet rebuilt
    /// from the installed game's own file instead (experimental: the first such build
    /// stopped Deadlock from starting on 2026-10-05).
    Blur,
    /// Our pak holds the game's own textures with their top mip levels dropped
    /// (`crate::texture`). Built on demand (minutes, gigabytes read), not by Apply.
    Textures,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// A file in Sqooky's GitHub repository, pinned by sha256; fetched with the `fetch`
    /// feature or imported by hand.
    Upstream {
        url: &'static str,
        file: &'static str,
        sha256: &'static str,
    },
    /// Built from the installed game's files; nothing to download.
    Generated,
}

#[derive(Clone, Debug)]
pub struct AddonInfo {
    pub id: AddonId,
    pub name: &'static str,
    pub author: &'static str,
    pub credit_url: &'static str,
    /// One plain sentence: what it changes.
    pub description: &'static str,
    /// One short phrase: what the player gains.
    pub benefit: &'static str,
    pub kind: Kind,
    pub source: Source,
    /// Preferred `pakNN` number; another is chosen when a foreign file sits there.
    pub slot: u8,
}

const REPO: &str = "https://github.com/Sqooky/OptimizationLock";
const GAMEBANANA_PAGE: &str = "https://gamebanana.com/mods/656341";

macro_rules! upstream {
    ($dir:literal, $file:literal, $sha:literal) => {
        Source::Upstream {
            url: concat!(
                "https://raw.githubusercontent.com/Sqooky/OptimizationLock/main/Various%20Addons%20Relating%20to%20Performance/",
                $dir,
                "/",
                $file
            ),
            file: $file,
            sha256: $sha,
        }
    };
}

pub static ADDONS: [AddonInfo; 6] = [
    AddonInfo {
        id: AddonId::ParticleDisabler,
        name: "Screen-edge particle disabler",
        author: "Laund",
        credit_url: REPO,
        description: "Hides the full-screen effects drawn at the edge of your view: low health, burns, poison, hero debuffs.",
        benefit: "Fewer overdraw-heavy particles, steadier frame rate in fights",
        kind: Kind::ParticleGroups,
        source: upstream!(
            "Screenspace%20Particle%20Disabler",
            "pak02_dir.vpk",
            "37c8188fa21980ba5959e4ecb5d6e1d4ec2bf4597d2d28cfa89ab3c913074eec"
        ),
        slot: 71,
    },
    AddonInfo {
        id: AddonId::BlurDisabler,
        name: "UI blur disabler",
        author: "Sqooky, with Bytenode",
        credit_url: REPO,
        description: "Turns off the blur behind HUD and menu panels. Sqooky's published pak, copied as is; after a game patch it may need his update.",
        benefit: "Less stutter when menus and the HUD open",
        kind: Kind::Blur,
        source: upstream!(
            "Blur%20Disabler",
            "pak97_dir.vpk",
            "7f5cc43cd8b175b53268238b960d9e62efd05d46e08c0c9f2c8f267d358d5dc5"
        ),
        slot: 72,
    },
    AddonInfo {
        id: AddonId::SinnerLightFix,
        name: "Sinner's Sacrifice light fix",
        author: "HoppCX (original from piggy's Discord)",
        credit_url: REPO,
        description: "Keeps the Sinner's Sacrifice lights readable when texture detail (mip bias) is turned down.",
        benefit: "Readability at low texture detail",
        kind: Kind::Toggle,
        source: upstream!(
            "Sinner%20Light%20Fix%20Mod",
            "pak26_dir.vpk",
            "98f0f8eb81a2e143d547e0fe29e35127104006f75e26378dad1ef294722fc7b7"
        ),
        slot: 73,
    },
    AddonInfo {
        id: AddonId::VindictaScope,
        name: "Vindicta scope downscale",
        author: "Tamara Mochaccinae",
        credit_url: "https://gamebanana.com/mods/659459",
        description: "Replaces Vindicta's 4096x4096 scope texture, which the game never mipmaps, with a 1080p one.",
        benefit: "No frame drops when scoping on low-VRAM cards",
        kind: Kind::Toggle,
        source: upstream!(
            "Vindicta%20Scope%20Downscale",
            "pak89_dir.vpk",
            "86ad5158a25b4c95e0645235f649db5bc1219617847be97dae20308974667281"
        ),
        slot: 74,
    },
    AddonInfo {
        id: AddonId::SoulContainer,
        name: "Optimized soul container",
        author: "Jayie",
        credit_url: "https://gamebanana.com/mods/600780",
        description: "Swaps the soul orb model for one with far fewer polygons.",
        benefit: "Cheaper to draw in every lane fight",
        kind: Kind::Toggle,
        source: upstream!(
            "Optimized%20Soul%20Container",
            "pak01_dir.vpk",
            "0c5a94ebfc087c58eac844089b3c694600a2cd3b9d2e4378789fc98a430669a2"
        ),
        slot: 75,
    },
    AddonInfo {
        id: AddonId::TextureDownscaler,
        name: "Low VRAM texture downscaler",
        author: "DeadTune, after Sqooky's Low VRAM pack",
        credit_url: GAMEBANANA_PAGE,
        description: "Builds a copy of the game's textures at half or a quarter of their size from your own game files. Originals are never modified.",
        benefit: "Much less VRAM; for cards that run out",
        kind: Kind::Textures,
        source: Source::Generated,
        slot: 76,
    },
];

pub fn all() -> &'static [AddonInfo] {
    &ADDONS
}

pub fn info(id: AddonId) -> &'static AddonInfo {
    ADDONS
        .iter()
        .find(|a| a.id == id)
        .expect("every AddonId has an entry in ADDONS")
}

/// How the blur disabler pak is made. Off by default, `rebuild` swaps Sqooky's file for a
/// stylesheet rebuilt from the installed game's own; `hud` and `menu` only matter then.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BlurOptions {
    #[serde(default)]
    pub rebuild: bool,
    #[serde(default = "yes")]
    pub hud: bool,
    #[serde(default = "yes")]
    pub menu: bool,
}

fn yes() -> bool {
    true
}

impl Default for BlurOptions {
    fn default() -> BlurOptions {
        BlurOptions {
            rebuild: false,
            hud: true,
            menu: true,
        }
    }
}

impl BlurOptions {
    pub fn is_default(&self) -> bool {
        *self == BlurOptions::default()
    }
}

/// The player's addon choices, stored in the profile.
#[derive(Clone, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct AddonsConfig {
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub enabled: BTreeSet<AddonId>,
    /// Particle groups ([`particles::GROUPS`] ids) left visible while the disabler is on.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub keep_particles: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BlurOptions::is_default")]
    pub blur: BlurOptions,
    #[serde(default, skip_serializing_if = "textures::is_default")]
    pub textures: TextureDownscale,
}

impl AddonsConfig {
    pub fn is_default(&self) -> bool {
        *self == AddonsConfig::default()
    }

    pub fn is_enabled(&self, id: AddonId) -> bool {
        self.enabled.contains(&id)
    }

    pub fn set_enabled(&mut self, id: AddonId, on: bool) {
        if on {
            self.enabled.insert(id);
        } else {
            self.enabled.remove(&id);
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AddonError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Vpk(#[from] crate::hud::vpk::VpkError),
    #[error(transparent)]
    Resource(#[from] crate::hud::resource::ResourceError),
    #[error(transparent)]
    SearchPaths(#[from] crate::hud::searchpaths::SearchPathsError),
    #[error("toml: {0}")]
    Toml(String),
    #[error("{0} is not ours; refusing to overwrite")]
    Foreign(PathBuf),
    #[error("game archive {0} not found; is the game fully installed?")]
    MissingGamePak(PathBuf),
    #[error("{0} is not a known addon file")]
    NotRecognised(PathBuf),
    #[error("texture build: {0}")]
    TextureBuild(#[from] crate::texture::AddonError),
    #[error("{file} does not match the pinned upstream file (sha256 {expected}, got {got})")]
    ShaMismatch {
        file: String,
        expected: String,
        got: String,
    },
    #[error("upstream pak02 does not carry the expected empty-particle stub")]
    BadStub,
    #[error("no free pakNN slot in {0}")]
    NoFreeSlot(PathBuf),
    #[error("the built pak failed DeadTune's check and was not installed: {0}")]
    Invalid(String),
    #[error("download: {0}")]
    Http(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_id_has_one_row_and_keys_match_serde() {
        assert_eq!(all().iter().map(|a| a.id).collect::<Vec<_>>(), AddonId::ALL);
        for id in AddonId::ALL {
            assert_eq!(info(id).id, id);
            assert_eq!(AddonId::parse(id.key()), Some(id));
            let value =
                toml::Value::try_from(std::collections::BTreeMap::from([("k", id)])).unwrap();
            assert_eq!(value["k"].as_str(), Some(id.key()));
        }
        assert_eq!(AddonId::parse("nope"), None);
    }

    #[test]
    fn slots_are_distinct_and_never_the_hud_addon() {
        let mut slots: Vec<u8> = all().iter().map(|a| a.slot).collect();
        slots.sort_unstable();
        slots.dedup();
        assert_eq!(slots.len(), all().len());
        assert!(
            slots
                .iter()
                .all(|s| format!("pak{s:02}_dir.vpk") != crate::hud::install::ADDON_FILE)
        );
    }

    #[test]
    fn upstream_urls_are_raw_github_without_unescaped_spaces() {
        for a in all() {
            if let Source::Upstream { url, file, sha256 } = a.source {
                assert!(
                    url.starts_with("https://raw.githubusercontent.com/"),
                    "{url}"
                );
                assert!(!url.contains(' '), "{url}");
                assert!(url.ends_with(file));
                assert_eq!(sha256.len(), 64);
            }
        }
    }

    #[test]
    fn config_serialises_only_what_differs_from_default() {
        let config = AddonsConfig::default();
        assert!(config.is_default());
        assert_eq!(toml::to_string(&config).unwrap(), "");
        let mut config = AddonsConfig::default();
        config.set_enabled(AddonId::BlurDisabler, true);
        config.blur.rebuild = true;
        config.blur.menu = false;
        config.keep_particles.insert("low_health".into());
        config.textures.factor = Factor::Quarter;
        let text = toml::to_string(&config).unwrap();
        assert!(text.contains("enabled = [\"blur_disabler\"]"), "{text}");
        assert!(text.contains("rebuild = true"), "{text}");
        assert!(text.contains("factor = \"quarter\""), "{text}");
        let back: AddonsConfig = toml::from_str(&text).unwrap();
        assert_eq!(back, config);
        let partial: AddonsConfig = toml::from_str("enabled = [\"soul_container\"]").unwrap();
        assert!(partial.is_enabled(AddonId::SoulContainer));
        assert!(textures::is_default(&partial.textures));
        assert!(partial.blur.is_default());
    }
}
