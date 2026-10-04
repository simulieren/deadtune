//! The low-VRAM texture downscaler, generated from the player's own game files rather
//! than imported from Sqooky's 689 MB GameBanana archive. The config and the build
//! contract live here; the mip-stripping itself is `dt_core::texture`, plugged in
//! through [`TextureBuilder`] so this module compiles before it lands.

use std::collections::BTreeSet;
use std::path::Path;

use crate::locate::GamePaths;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum TextureCategory {
    World,
    Heroes,
    Particles,
    Ui,
    Other,
}

impl TextureCategory {
    pub const ALL: [TextureCategory; 5] = [
        TextureCategory::World,
        TextureCategory::Heroes,
        TextureCategory::Particles,
        TextureCategory::Ui,
        TextureCategory::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TextureCategory::World => "World and props",
            TextureCategory::Heroes => "Heroes",
            TextureCategory::Particles => "Particles",
            TextureCategory::Ui => "UI and HUD",
            TextureCategory::Other => "Everything else (weapons, NPCs, items)",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            TextureCategory::World => "world",
            TextureCategory::Heroes => "heroes",
            TextureCategory::Particles => "particles",
            TextureCategory::Ui => "ui",
            TextureCategory::Other => "other",
        }
    }

    /// Which category a game texture path falls in. Lighting data (lightmaps, cubemaps,
    /// irradiance under `maps/`) is not a category: it is governed by
    /// [`TextureDownscale::exclude_lighting`] and never offered as a checkbox.
    pub fn of(path: &str) -> Option<TextureCategory> {
        if is_lighting(path) {
            return None;
        }
        let p = path.to_ascii_lowercase();
        let cat = if p.starts_with("panorama/") || p.starts_with("materials/ui") {
            TextureCategory::Ui
        } else if p.starts_with("particles/") || p.starts_with("materials/particle") {
            TextureCategory::Particles
        } else if p.starts_with("materials/models/heroes/") || p.starts_with("models/heroes/") {
            TextureCategory::Heroes
        } else if WORLD_PREFIXES.iter().any(|w| p.starts_with(w)) {
            TextureCategory::World
        } else {
            TextureCategory::Other
        };
        Some(cat)
    }
}

const WORLD_PREFIXES: &[&str] = &[
    "materials/models/props",
    "materials/models/world",
    "materials/world",
    "materials/environment",
    "materials/nature",
    "materials/structures",
    "materials/props",
    "materials/decals",
    "materials/skybox",
    "materials/terrain",
    "models/props",
    "models/world",
];

/// Lightmaps and probes: downscaling these is what makes the map look bright and flat.
pub fn is_lighting(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    p.starts_with("maps/")
        || p.contains("lightmap")
        || p.contains("cubemap")
        || p.contains("irradiance")
        || p.contains("envmap")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Factor {
    Half,
    #[default]
    Quarter,
}

impl Factor {
    pub const ALL: [Factor; 2] = [Factor::Half, Factor::Quarter];

    pub fn label(self) -> &'static str {
        match self {
            Factor::Half => "1/2",
            Factor::Quarter => "1/4",
        }
    }

    /// Mip levels dropped from the top of each texture.
    pub fn mips_dropped(self) -> u8 {
        match self {
            Factor::Half => 1,
            Factor::Quarter => 2,
        }
    }
}

/// What the player asked the downscaler to do.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TextureDownscale {
    #[serde(default)]
    pub factor: Factor,
    #[serde(default = "all_categories")]
    pub categories: BTreeSet<TextureCategory>,
    /// Leave lightmaps and probes alone (recommended; see the upstream readme).
    #[serde(default = "yes")]
    pub exclude_lighting: bool,
}

fn yes() -> bool {
    true
}

fn all_categories() -> BTreeSet<TextureCategory> {
    TextureCategory::ALL.into_iter().collect()
}

impl Default for TextureDownscale {
    fn default() -> TextureDownscale {
        TextureDownscale {
            factor: Factor::Quarter,
            categories: all_categories(),
            exclude_lighting: true,
        }
    }
}

impl TextureDownscale {
    pub fn is_default(&self) -> bool {
        *self == TextureDownscale::default()
    }

    /// Whether the build should touch this game texture.
    pub fn includes(&self, path: &str) -> bool {
        match TextureCategory::of(path) {
            None => !self.exclude_lighting,
            Some(cat) => self.categories.contains(&cat),
        }
    }

    /// Stable text for the install record's input fingerprint.
    pub fn fingerprint(&self) -> String {
        let cats: Vec<&str> = self.categories.iter().map(|c| c.key()).collect();
        format!(
            "factor={:?};categories={};lighting={}",
            self.factor,
            cats.join(","),
            if self.exclude_lighting {
                "sharp"
            } else {
                "downscaled"
            }
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    /// The texture being worked on.
    pub current: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct BuildStats {
    pub textures: usize,
    pub bytes_before: u64,
    pub bytes_after: u64,
}

/// Writes the downscaled-texture addon VPK to `out` from the game's own archives.
/// Implemented by `dt_core::texture`; [`Unavailable`] stands in until it lands.
pub trait TextureBuilder: Sync {
    fn build(
        &self,
        paths: &GamePaths,
        cfg: &TextureDownscale,
        out: &Path,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<BuildStats, String>;
}

pub struct Unavailable;

impl TextureBuilder for Unavailable {
    fn build(
        &self,
        _paths: &GamePaths,
        _cfg: &TextureDownscale,
        _out: &Path,
        _progress: &mut dyn FnMut(Progress),
    ) -> Result<BuildStats, String> {
        Err("Texture downscaling is not available in this build of DeadTune yet.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categorises_by_path_prefix_and_keeps_lighting_apart() {
        let of = TextureCategory::of;
        assert_eq!(
            of("materials/models/heroes/haze/haze_color.vtex_c"),
            Some(TextureCategory::Heroes)
        );
        assert_eq!(
            of("materials/models/props/crate_color.vtex_c"),
            Some(TextureCategory::World)
        );
        assert_eq!(
            of("materials/world/street/asphalt.vtex_c"),
            Some(TextureCategory::World)
        );
        assert_eq!(
            of("panorama/images/hud/x.vtex_c"),
            Some(TextureCategory::Ui)
        );
        assert_eq!(
            of("materials/particle/smoke.vtex_c"),
            Some(TextureCategory::Particles)
        );
        assert_eq!(
            of("materials/models/weapons/gun.vtex_c"),
            Some(TextureCategory::Other)
        );
        assert_eq!(of("maps/street_test/irradiance.vtex_c"), None);
        assert_eq!(of("materials/lightmaps/a.vtex_c"), None);
        assert!(is_lighting("materials/skybox/sky_cubemap.vtex_c"));
    }

    #[test]
    fn includes_follows_categories_and_the_lighting_switch() {
        let mut cfg = TextureDownscale::default();
        assert!(cfg.includes("materials/models/heroes/haze/haze_color.vtex_c"));
        assert!(!cfg.includes("maps/street_test/irradiance.vtex_c"));
        cfg.categories.remove(&TextureCategory::Heroes);
        assert!(!cfg.includes("materials/models/heroes/haze/haze_color.vtex_c"));
        cfg.exclude_lighting = false;
        assert!(cfg.includes("maps/street_test/irradiance.vtex_c"));
        assert_ne!(cfg.fingerprint(), TextureDownscale::default().fingerprint());
    }

    #[test]
    fn config_round_trips_and_defaults_fill_in() {
        let cfg: TextureDownscale = toml::from_str("factor = \"half\"").unwrap();
        assert_eq!(cfg.factor, Factor::Half);
        assert_eq!(cfg.factor.mips_dropped(), 1);
        assert!(cfg.exclude_lighting);
        assert_eq!(cfg.categories.len(), TextureCategory::ALL.len());
        let text = toml::to_string(&cfg).unwrap();
        assert_eq!(toml::from_str::<TextureDownscale>(&text).unwrap(), cfg);
        assert!(TextureDownscale::default().is_default());
    }

    #[test]
    fn placeholder_builder_refuses_with_a_plain_message() {
        let (_dir, paths) = crate::addons::install::tests::fake_install("1");
        let err = Unavailable
            .build(
                &paths,
                &TextureDownscale::default(),
                Path::new("x"),
                &mut |_| {},
            )
            .unwrap_err();
        assert!(err.contains("not available"));
    }
}
