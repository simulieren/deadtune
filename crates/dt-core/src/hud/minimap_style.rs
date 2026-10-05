//! Experimental: minimap marker sizes, map opacity and a frameless minimap. Not yet
//! tested in game.
//!
//! Marker sizes use `pre-transform-scale2d`, the property the vanilla rules size
//! `.map_button` with. Vanilla shrinks hero markers in the zoomed minimap, so hero
//! sizes are emitted once per zoom state with the vanilla factor multiplied in, in
//! vanilla's order. Every selector carries `#hud_minimap`, so it outranks its vanilla
//! rule regardless of order.

use std::collections::BTreeMap;
use std::ops::RangeInclusive;

use super::css::emit_rule;
use super::elements::HUD_STYLE;
use super::minimap_colors::MINIMAP_STYLE;

pub const MARKER_SCALE_RANGE: RangeInclusive<u16> = 50..=200;
pub const MAP_OPACITY_RANGE: RangeInclusive<u8> = 20..=100;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum MarkerGroup {
    LocalHero,
    AllyHeroes,
    EnemyHeroes,
    Objectives,
    Shops,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarkerSpec {
    pub group: MarkerGroup,
    pub label: &'static str,
    pub selectors: &'static [&'static str],
    /// Hero markers, which vanilla rescales in the zoomed minimap.
    pub hero: bool,
}

/// Emission order: your hero after allied heroes, whose selector it also matches.
pub static MARKERS: &[MarkerSpec] = &[
    MarkerSpec {
        group: MarkerGroup::AllyHeroes,
        label: "Allied heroes",
        selectors: &[".map_button.player.friend"],
        hero: true,
    },
    MarkerSpec {
        group: MarkerGroup::LocalHero,
        label: "Your hero",
        selectors: &[".map_button.player.friend.localplayer"],
        hero: true,
    },
    MarkerSpec {
        group: MarkerGroup::EnemyHeroes,
        label: "Enemy heroes",
        selectors: &[".map_button.player.enemy"],
        hero: true,
    },
    MarkerSpec {
        group: MarkerGroup::Objectives,
        label: "Objectives",
        selectors: &[
            ".map_button.boss_icon_t1",
            ".map_button.boss_icon_t2",
            ".map_button.boss_icon_t3",
            ".map_button.boss_barracks_icon",
            ".map_button.boss_buildingzip",
        ],
        hero: false,
    },
    MarkerSpec {
        group: MarkerGroup::Shops,
        label: "Shops",
        selectors: &[".map_button.tier1_shop"],
        hero: false,
    },
];

/// Vanilla's hero-marker scale per minimap state (`hud_minimap.css`), in vanilla's order.
const HERO_STATES: [(&str, f64); 6] = [
    ("#hud_minimap", 1.0),
    (".useZoomedMinimap #hud_minimap", 0.3),
    (".useZoomedMinimap #hud_minimap.gScoreboardOpen", 1.0),
    (".useZoomedMinimap #hud_minimap.zoomLevel9", 0.2),
    (".useZoomedMinimap #hud_minimap.zoomLevel10", 0.2),
    (".useZoomedMinimap #hud_minimap.zoomLevel11", 0.2),
];

/// The broker carries `.tier1_shop` too; it keeps its own size and pulse.
const BROKER: &str = "#hud_minimap .map_button.tier1_shop.corrupted_item_shop";

pub fn spec(group: MarkerGroup) -> &'static MarkerSpec {
    MARKERS
        .iter()
        .find(|m| m.group == group)
        .expect("every marker group has a spec")
}

/// `Default` is vanilla: nothing emitted.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct MinimapStyle {
    /// Percent per group; 100 is never stored.
    pub marker_scale_pct: BTreeMap<MarkerGroup, u16>,
    /// The map picture only; markers keep their own opacity.
    pub map_opacity_pct: u8,
    /// Hides the frame and blur around the minimap.
    pub minimal: bool,
}

impl Default for MinimapStyle {
    fn default() -> Self {
        MinimapStyle {
            marker_scale_pct: BTreeMap::new(),
            map_opacity_pct: 100,
            minimal: false,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum StyleError {
    #[error("{0:?}: marker size {1}% outside 50..=200")]
    MarkerScale(MarkerGroup, u16),
    #[error("map opacity {0}% outside 20..=100")]
    MapOpacity(u8),
}

impl MinimapStyle {
    pub fn is_vanilla(&self) -> bool {
        *self == MinimapStyle::default()
    }

    pub fn changed_count(&self) -> usize {
        self.marker_scale_pct.len()
            + usize::from(self.map_opacity_pct != 100)
            + usize::from(self.minimal)
    }

    pub fn scale(&self, group: MarkerGroup) -> u16 {
        self.marker_scale_pct.get(&group).copied().unwrap_or(100)
    }

    /// Minified CSS per style file. Deterministic.
    pub fn compile(&self) -> Result<BTreeMap<&'static str, String>, StyleError> {
        let mut files: BTreeMap<&'static str, String> = BTreeMap::new();
        for (&group, &pct) in &self.marker_scale_pct {
            if !MARKER_SCALE_RANGE.contains(&pct) {
                return Err(StyleError::MarkerScale(group, pct));
            }
        }
        if !MAP_OPACITY_RANGE.contains(&self.map_opacity_pct) {
            return Err(StyleError::MapOpacity(self.map_opacity_pct));
        }
        let minimap = files.entry(MINIMAP_STYLE).or_default();
        // Your hero also matches the allied selector, so it is restated whenever allies change.
        let restate_local = self.marker_scale_pct.contains_key(&MarkerGroup::AllyHeroes);
        for spec in MARKERS {
            let set = self.marker_scale_pct.contains_key(&spec.group);
            if !set && !(spec.group == MarkerGroup::LocalHero && restate_local) {
                continue;
            }
            let scale = f64::from(self.scale(spec.group)) / 100.0;
            let states: &[(&str, f64)] = if spec.hero {
                &HERO_STATES
            } else {
                &HERO_STATES[..1]
            };
            for (scope, factor) in states {
                let selector = spec
                    .selectors
                    .iter()
                    .map(|s| format!("{scope} {s}"))
                    .collect::<Vec<_>>()
                    .join(",");
                minimap.push_str(&scale_rule(&selector, scale * factor));
            }
            if spec.group == MarkerGroup::Shops {
                minimap.push_str(&scale_rule(BROKER, 1.0));
            }
        }
        if self.map_opacity_pct != 100 {
            minimap.push_str(&emit_rule(
                "#hud_minimap .NewMinimapBackgroundsContainer",
                &[("opacity", number(f64::from(self.map_opacity_pct) / 100.0))],
            ));
        }
        if self.minimal {
            files.entry(HUD_STYLE).or_default().push_str(&emit_rule(
                "#minimap_persp #minimap_frame,#minimap_persp #minimap_blur",
                &[("opacity", "0".to_string())],
            ));
        }
        files.retain(|_, css| !css.is_empty());
        Ok(files)
    }
}

fn scale_rule(selector: &str, scale: f64) -> String {
    emit_rule(selector, &[("pre-transform-scale2d", number(scale))])
}

fn number(value: f64) -> String {
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(sizes: &[(MarkerGroup, u16)]) -> MinimapStyle {
        MinimapStyle {
            marker_scale_pct: sizes.iter().copied().collect(),
            ..MinimapStyle::default()
        }
    }

    #[test]
    fn vanilla_emits_nothing() {
        let s = MinimapStyle::default();
        assert!(s.is_vanilla());
        assert_eq!(s.changed_count(), 0);
        assert!(s.compile().unwrap().is_empty());
    }

    #[test]
    fn enemy_hero_size_follows_every_zoom_state() {
        let css = &style(&[(MarkerGroup::EnemyHeroes, 150)]).compile().unwrap()[MINIMAP_STYLE];
        assert_eq!(
            css,
            "#hud_minimap .map_button.player.enemy{pre-transform-scale2d:1.5;}\
             .useZoomedMinimap #hud_minimap .map_button.player.enemy{pre-transform-scale2d:0.45;}\
             .useZoomedMinimap #hud_minimap.gScoreboardOpen .map_button.player.enemy{pre-transform-scale2d:1.5;}\
             .useZoomedMinimap #hud_minimap.zoomLevel9 .map_button.player.enemy{pre-transform-scale2d:0.3;}\
             .useZoomedMinimap #hud_minimap.zoomLevel10 .map_button.player.enemy{pre-transform-scale2d:0.3;}\
             .useZoomedMinimap #hud_minimap.zoomLevel11 .map_button.player.enemy{pre-transform-scale2d:0.3;}"
        );
    }

    #[test]
    fn allied_size_restates_your_hero_after_it() {
        let css = &style(&[(MarkerGroup::AllyHeroes, 70)]).compile().unwrap()[MINIMAP_STYLE];
        let ally = css
            .find("#hud_minimap .map_button.player.friend{pre-transform-scale2d:0.7;}")
            .unwrap();
        let local = css
            .find("#hud_minimap .map_button.player.friend.localplayer{pre-transform-scale2d:1;}")
            .unwrap();
        assert!(local > ally, "{css}");
        assert_eq!(css.matches("localplayer").count(), HERO_STATES.len());

        let css = &style(&[(MarkerGroup::LocalHero, 120)]).compile().unwrap()[MINIMAP_STYLE];
        assert!(!css.contains(".friend{"), "{css}");
    }

    #[test]
    fn objectives_and_shops_have_one_state_and_the_broker_keeps_its_size() {
        let css = &style(&[(MarkerGroup::Objectives, 80), (MarkerGroup::Shops, 130)])
            .compile()
            .unwrap()[MINIMAP_STYLE];
        assert!(!css.contains("useZoomedMinimap"), "{css}");
        assert!(css.starts_with(
            "#hud_minimap .map_button.boss_icon_t1,#hud_minimap .map_button.boss_icon_t2,"
        ));
        assert!(css.ends_with(
            "#hud_minimap .map_button.tier1_shop{pre-transform-scale2d:1.3;}\
             #hud_minimap .map_button.tier1_shop.corrupted_item_shop{pre-transform-scale2d:1;}"
        ));
    }

    #[test]
    fn map_opacity_and_minimal_frame() {
        let s = MinimapStyle {
            map_opacity_pct: 60,
            minimal: true,
            ..MinimapStyle::default()
        };
        assert_eq!(s.changed_count(), 2);
        let files = s.compile().unwrap();
        assert_eq!(
            files[MINIMAP_STYLE],
            "#hud_minimap .NewMinimapBackgroundsContainer{opacity:0.6;}"
        );
        assert_eq!(
            files[HUD_STYLE],
            "#minimap_persp #minimap_frame,#minimap_persp #minimap_blur{opacity:0;}"
        );
    }

    #[test]
    fn out_of_range_values_are_refused() {
        assert_eq!(
            style(&[(MarkerGroup::Shops, 300)]).compile(),
            Err(StyleError::MarkerScale(MarkerGroup::Shops, 300))
        );
        let s = MinimapStyle {
            map_opacity_pct: 5,
            ..MinimapStyle::default()
        };
        assert_eq!(s.compile(), Err(StyleError::MapOpacity(5)));
    }

    #[test]
    fn every_group_has_one_spec() {
        for m in MARKERS {
            assert_eq!(spec(m.group), m);
        }
        assert_eq!(MARKERS.len(), 5);
    }
}
