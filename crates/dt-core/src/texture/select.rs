//! Which textures to reduce, and by how much. Category comes from the VPK path prefix;
//! "lighting" is a separate attribute matched on path tokens, because the known
//! failure of whole-game downscaling (a bright, flat map) comes from baked lighting
//! data, which can sit under any prefix.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::vtex::SkipReason;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Factor {
    Half,
    Quarter,
}

impl Factor {
    /// Mip levels to drop.
    pub fn levels(self) -> u8 {
        match self {
            Factor::Half => 1,
            Factor::Quarter => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// `models/`, `materials/`, `maps/`: props, buildings, terrain.
    World,
    /// Anything under a `heroes` directory.
    Heroes,
    /// `particles/` or a `particle(s)` directory.
    Particles,
    /// `panorama/`: HUD and menu images.
    Ui,
    /// Any other prefix.
    Other,
}

impl Category {
    pub const ALL: [Category; 5] = [
        Category::World,
        Category::Heroes,
        Category::Particles,
        Category::Ui,
        Category::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::World => "world",
            Category::Heroes => "heroes",
            Category::Particles => "particles",
            Category::Ui => "ui",
            Category::Other => "other",
        }
    }
}

fn segments(path: &str) -> impl Iterator<Item = &str> {
    path.split('/')
}

pub fn category(path: &str) -> Category {
    let lower = path.to_ascii_lowercase();
    let p = lower.as_str();
    if p.starts_with("panorama/") {
        return Category::Ui;
    }
    if segments(p).any(|s| s == "heroes") {
        return Category::Heroes;
    }
    if p.starts_with("particles/") || segments(p).any(|s| s == "particle" || s == "particles") {
        return Category::Particles;
    }
    if p.starts_with("models/") || p.starts_with("materials/") || p.starts_with("maps/") {
        return Category::World;
    }
    Category::Other
}

const LIGHTING_TOKENS: [&str; 22] = [
    "lightmap",
    "lightmaps",
    "cubemap",
    "cubemaps",
    "envmap",
    "envmaps",
    "env",
    "ibl",
    "lightprobe",
    "lightprobes",
    "lpv",
    "irradiance",
    "radiance",
    "skybox",
    "sky",
    "ao",
    "light",
    "lights",
    "lighting",
    "shadow",
    "shadows",
    "sh",
];

/// Baked lighting: everything under `maps/` (lightmaps, light probes), plus any path
/// token in `LIGHTING_TOKENS`. Deliberately broad; a street light prop counts.
pub fn is_lighting(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    if lower.starts_with("maps/") || lower.contains("lightmap") || lower.contains("cubemap") {
        return true;
    }
    lower
        .split(['/', '_', '.', '-', ' '])
        .any(|t| LIGHTING_TOKENS.contains(&t))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextureDownscale {
    pub factor: Factor,
    pub categories: BTreeSet<Category>,
    pub exclude_lighting: bool,
}

impl Default for TextureDownscale {
    fn default() -> Self {
        TextureDownscale {
            factor: Factor::Half,
            categories: BTreeSet::from([Category::World, Category::Heroes, Category::Particles]),
            exclude_lighting: true,
        }
    }
}

impl TextureDownscale {
    /// Whether a texture at this VPK path is a candidate, or why not.
    pub fn selects(&self, path: &str) -> Result<(), SkipReason> {
        if self.exclude_lighting && is_lighting(path) {
            return Err(SkipReason::Lighting);
        }
        if !self.categories.contains(&category(path)) {
            return Err(SkipReason::NotSelected);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_paths() {
        use Category::*;
        let cases = [
            (
                "models/props_gameplay/soul_container/materials/soul_container_color_png_75e69035.vtex_c",
                World,
                false,
            ),
            (
                "models/props_gameplay/soul_container/materials/soul_container_ao_png_e9d127e1.vtex_c",
                World,
                true,
            ),
            (
                "materials/environment/street/brick_01_color.vtex_c",
                World,
                false,
            ),
            (
                "materials/props/street_light/lamp_color.vtex_c",
                World,
                true,
            ),
            (
                "models/heroes/inferno/inferno_body_color.vtex_c",
                Heroes,
                false,
            ),
            (
                "materials/models/heroes/haze/haze_normal.vtex_c",
                Heroes,
                false,
            ),
            ("particles/fire/flame_sheet.vtex_c", Particles, false),
            ("materials/particle/smoke/smoke_01.vtex_c", Particles, false),
            (
                "panorama/images/hud/crosshair/scope_common_psd.vtex_c",
                Ui,
                false,
            ),
            ("maps/street_test/lightmaps/irradiance.vtex_c", World, true),
            ("maps/street_test/direct_light_indices.vtex_c", World, true),
            ("materials/skybox/sky_day_hdr.vtex_c", World, true),
            ("materials/default/cubemap_default.vtex_c", World, true),
            ("materials/models/heroes/haze/haze_ibl.vtex_c", Heroes, true),
            ("scripts/odd/thing.vtex_c", Other, false),
            ("thing.vtex_c", Other, false),
            ("Models/Heroes/X/Y.vtex_c", Heroes, false),
        ];
        for (path, cat, lighting) in cases {
            assert_eq!(category(path), cat, "{path}");
            assert_eq!(is_lighting(path), lighting, "{path}");
        }
    }

    #[test]
    fn default_selection() {
        let cfg = TextureDownscale::default();
        assert_eq!(cfg.factor.levels(), 1);
        assert_eq!(cfg.selects("models/props/a/b_color.vtex_c"), Ok(()));
        assert_eq!(
            cfg.selects("panorama/images/x.vtex_c"),
            Err(SkipReason::NotSelected)
        );
        assert_eq!(
            cfg.selects("maps/x/irradiance.vtex_c"),
            Err(SkipReason::Lighting)
        );
        let all = TextureDownscale {
            factor: Factor::Quarter,
            categories: Category::ALL.into_iter().collect(),
            exclude_lighting: false,
        };
        assert_eq!(all.factor.levels(), 2);
        assert_eq!(all.selects("maps/x/irradiance.vtex_c"), Ok(()));
        assert_eq!(all.selects("whatever.vtex_c"), Ok(()));
    }

    #[test]
    fn serde_round_trip() {
        let cfg = TextureDownscale {
            factor: Factor::Quarter,
            categories: BTreeSet::from([Category::Ui, Category::World]),
            exclude_lighting: false,
        };
        let text = toml::to_string(&cfg).unwrap();
        assert_eq!(
            text,
            "factor = \"quarter\"\ncategories = [\"world\", \"ui\"]\nexclude_lighting = false\n"
        );
        assert_eq!(toml::from_str::<TextureDownscale>(&text).unwrap(), cfg);
        assert_eq!(
            toml::from_str::<TextureDownscale>("").unwrap(),
            TextureDownscale::default()
        );
        assert_eq!(
            toml::from_str::<TextureDownscale>("factor = \"half\"\n").unwrap(),
            TextureDownscale::default()
        );
    }
}
