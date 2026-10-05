//! Performance addons: the community mods from Sqooky's OptimizationLock bundle
//! ("Various Addons Relating to Performance", GameBanana mod 656341), rebuilt as
//! DeadTune-owned pak files in `citadel/addons`.
//!
//! Every addon is one row in [`ADDONS`]. Its [`Kind`] says how our pak file is made:
//! a verbatim copy of the author's file, a [`Native`] rebuild of the author's method from
//! the installed game's own files, or a filtered copy of the game's textures. Its
//! [`Source`] says where the bytes come from. [`AddonsConfig`] is the player's choice and
//! lives in the profile; [`install`] turns it into pak files and keeps an ownership record
//! so no file DeadTune did not write is ever touched.

use std::collections::BTreeSet;
use std::path::PathBuf;

pub mod guard;
pub mod install;
pub mod native;
pub mod native_blur;
pub mod native_particles;
pub mod native_scope;
pub mod native_sinner;
pub mod particles;
pub mod sources;
pub mod textures;
pub mod verify;

pub use crate::texture::{
    Category as TextureCategory, Factor, Stats as TextureStats, TextureDownscale,
};
pub use install::{Action, AddonPlan, AddonsPlan, Blocker, Build, Conflict, InstalledState};
pub use native::Native;
pub use native_scope::ScopeOptions;

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
    /// Our pak is the author's file, byte for byte.
    Toggle,
    /// Our pak is the author's method applied to the game's own files, by one of the
    /// [`Native`] builders. Rebuilt by Apply after every game update.
    Native(Native),
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

impl AddonInfo {
    /// The credit line under the name. A rebuilt addon names the method's author and says
    /// DeadTune made the file.
    pub fn credit(&self) -> String {
        match self.kind {
            Kind::Native(_) => format!(
                "Method by {}; rebuilt by DeadTune from your game files.",
                self.author
            ),
            Kind::Toggle | Kind::Textures => format!("by {}", self.author),
        }
    }
}

const REPO: &str = "https://github.com/Sqooky/OptimizationLock";
const GAMEBANANA_PAGE: &str = "https://gamebanana.com/mods/656341";

pub static ADDONS: [AddonInfo; 6] = [
    AddonInfo {
        id: AddonId::ParticleDisabler,
        name: "Screen-edge particle disabler",
        author: "Laund",
        credit_url: REPO,
        description: "Hides the full-screen effects drawn at the edge of your view: low health, burns, poison, hero debuffs.",
        benefit: "Fewer overdraw-heavy particles, steadier frame rate in fights",
        kind: Kind::Native(Native::Particles),
        source: Source::Generated,
        slot: 71,
    },
    AddonInfo {
        id: AddonId::BlurDisabler,
        name: "UI blur disabler",
        author: "Sqooky, with Bytenode",
        credit_url: REPO,
        description: "Turns off the blur behind HUD and menu panels.",
        benefit: "Less stutter when menus and the HUD open",
        kind: Kind::Native(Native::Blur),
        source: Source::Generated,
        slot: 72,
    },
    AddonInfo {
        id: AddonId::SinnerLightFix,
        name: "Sinner's Sacrifice light fix",
        author: "HoppCX (original from piggy's Discord)",
        credit_url: REPO,
        description: "Keeps the Sinner's Sacrifice lights readable at low texture detail: full-size light pattern only, always the full-detail vault model.",
        benefit: "Readability at low texture detail",
        kind: Kind::Native(Native::Sinner),
        source: Source::Generated,
        slot: 73,
    },
    AddonInfo {
        id: AddonId::VindictaScope,
        name: "Vindicta scope downscale",
        author: "Tamara Mochaccinae",
        credit_url: "https://gamebanana.com/mods/659459",
        description: "Shrinks Vindicta's 4096x4096 scope overlay, which the game never mipmaps, to a size that fits your screen.",
        benefit: "No frame drops when scoping on low-VRAM cards",
        kind: Kind::Native(Native::Scope),
        source: Source::Generated,
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
        source: Source::Upstream {
            url: "https://raw.githubusercontent.com/Sqooky/OptimizationLock/main/Various%20Addons%20Relating%20to%20Performance/Optimized%20Soul%20Container/pak01_dir.vpk",
            file: "pak01_dir.vpk",
            sha256: "0c5a94ebfc087c58eac844089b3c694600a2cd3b9d2e4378789fc98a430669a2",
        },
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

/// Which blur the disabler turns off. Both by default; neither means nothing to install.
/// Older profiles carry a `rebuild` flag from the experimental generator; serde ignores it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BlurOptions {
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
    #[serde(default, skip_serializing_if = "ScopeOptions::is_default")]
    pub scope: ScopeOptions,
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
    #[error("{0} is not a known addon file")]
    NotRecognised(PathBuf),
    #[error("texture build: {0}")]
    TextureBuild(#[from] crate::texture::AddonError),
    #[error(transparent)]
    Scope(#[from] native_scope::ScopeError),
    #[error("{file} does not match the pinned upstream file (sha256 {expected}, got {got})")]
    ShaMismatch {
        file: String,
        expected: String,
        got: String,
    },
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
    fn only_the_soul_container_is_downloaded_and_the_rest_are_rebuilt() {
        for a in all() {
            let upstream = matches!(a.source, Source::Upstream { .. });
            assert_eq!(upstream, a.kind == Kind::Toggle, "{:?}", a.id);
            assert_eq!(upstream, a.id == AddonId::SoulContainer, "{:?}", a.id);
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
        let native: Vec<Native> = all()
            .iter()
            .filter_map(|a| match a.kind {
                Kind::Native(n) => Some(n),
                _ => None,
            })
            .collect();
        assert_eq!(
            native,
            [
                Native::Particles,
                Native::Blur,
                Native::Sinner,
                Native::Scope
            ]
        );
    }

    #[test]
    fn rebuilt_addons_credit_the_method_author_and_name_deadtune() {
        assert_eq!(
            info(AddonId::ParticleDisabler).credit(),
            "Method by Laund; rebuilt by DeadTune from your game files."
        );
        assert_eq!(
            info(AddonId::VindictaScope).credit(),
            "Method by Tamara Mochaccinae; rebuilt by DeadTune from your game files."
        );
        assert_eq!(info(AddonId::SoulContainer).credit(), "by Jayie");
        for a in all() {
            assert!(!a.description.contains("Method by"), "{:?}", a.id);
        }
    }

    #[test]
    fn config_serialises_only_what_differs_from_default() {
        let config = AddonsConfig::default();
        assert!(config.is_default());
        assert_eq!(toml::to_string(&config).unwrap(), "");
        let mut config = AddonsConfig::default();
        config.set_enabled(AddonId::BlurDisabler, true);
        config.blur.menu = false;
        config.scope.side = 720;
        config.keep_particles.insert("low_health".into());
        config.textures.factor = Factor::Quarter;
        let text = toml::to_string(&config).unwrap();
        assert!(text.contains("enabled = [\"blur_disabler\"]"), "{text}");
        assert!(text.contains("menu = false"), "{text}");
        assert!(text.contains("side = 720"), "{text}");
        assert!(text.contains("factor = \"quarter\""), "{text}");
        let back: AddonsConfig = toml::from_str(&text).unwrap();
        assert_eq!(back, config);
        let partial: AddonsConfig = toml::from_str("enabled = [\"soul_container\"]").unwrap();
        assert!(partial.is_enabled(AddonId::SoulContainer));
        assert!(textures::is_default(&partial.textures));
        assert!(partial.blur.is_default());
        assert!(partial.scope.is_default());
    }

    #[test]
    fn a_profile_from_the_experimental_blur_generator_still_loads() {
        let old = "enabled = [\"blur_disabler\", \"sinner_light_fix\"]\n\n[blur]\nrebuild = true\nhud = false\n";
        let config: AddonsConfig = toml::from_str(old).unwrap();
        assert!(config.is_enabled(AddonId::BlurDisabler));
        assert_eq!(
            config.blur,
            BlurOptions {
                hud: false,
                menu: true
            }
        );
        assert!(!toml::to_string(&config).unwrap().contains("rebuild"));
    }
}
