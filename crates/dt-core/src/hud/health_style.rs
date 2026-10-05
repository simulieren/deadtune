//! Experimental: health bar styling. Not yet tested in game.
//!
//! Rules are appended to the game's own health stylesheets, like the minimap colours,
//! and every one targets a panel or state class the vanilla rules already use. Ideas
//! from bytenode's Minimal Healthbar Redux and budhud-style Alternate Health Bar; the
//! rules here are DeadTune's own.

use std::collections::BTreeMap;
use std::ops::RangeInclusive;

use super::css::emit_rule;
use super::elements::HUD_STYLE;

pub const HEALTH_STYLE: &str = "panorama/styles/hud_health.vcss_c";
pub const HEALTH_CONTAINER_STYLE: &str = "panorama/styles/hud_health_container.vcss_c";
pub const NUMBER_SCALE_RANGE: RangeInclusive<u16> = 80..=200;

/// Vanilla sizes from `hud_health_container.css`.
const NUMBER_PX: f64 = 32.0;
const NUMBER_LOW_PX: f64 = 36.0;
const NUMBER_BOX: (f64, f64) = (100.0, 65.0);
const HURT_COLOR: &str = "#FFB347";

/// `Default` is vanilla: nothing emitted.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct HealthStyle {
    /// Size of the current-health number, percent of vanilla.
    pub number_scale_pct: u16,
    /// The number turns orange when hurt (vanilla already turns it red when low).
    pub color_by_health: bool,
    /// Max health at full strength instead of vanilla's faint 20 %.
    pub clear_max_health: bool,
    /// The green shape behind the health number.
    pub hide_backer: bool,
    /// The shaking and pulsing at mid and low health.
    pub no_shake: bool,
    /// The health regeneration number beside the bar.
    pub hide_regen: bool,
}

impl Default for HealthStyle {
    fn default() -> Self {
        HealthStyle {
            number_scale_pct: 100,
            color_by_health: false,
            clear_max_health: false,
            hide_backer: false,
            no_shake: false,
            hide_regen: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HealthPreset {
    Vanilla,
    Clean,
    BigNumber,
}

impl HealthPreset {
    pub const ALL: [HealthPreset; 3] = [
        HealthPreset::Vanilla,
        HealthPreset::Clean,
        HealthPreset::BigNumber,
    ];

    pub fn label(self) -> &'static str {
        match self {
            HealthPreset::Vanilla => "Vanilla",
            HealthPreset::Clean => "Clean",
            HealthPreset::BigNumber => "Big number",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            HealthPreset::Vanilla => "The game's own health bar.",
            HealthPreset::Clean => "No backer, no shaking, max health easy to read.",
            HealthPreset::BigNumber => {
                "A big health number that turns orange when hurt, after budhud from TF2."
            }
        }
    }

    pub fn style(self) -> HealthStyle {
        match self {
            HealthPreset::Vanilla => HealthStyle::default(),
            HealthPreset::Clean => HealthStyle {
                clear_max_health: true,
                hide_backer: true,
                no_shake: true,
                ..HealthStyle::default()
            },
            HealthPreset::BigNumber => HealthStyle {
                number_scale_pct: 160,
                color_by_health: true,
                clear_max_health: true,
                hide_backer: true,
                no_shake: true,
                hide_regen: false,
            },
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum HealthError {
    #[error("health number size {0}% outside 80..=200")]
    NumberScale(u16),
}

impl HealthStyle {
    pub fn is_vanilla(&self) -> bool {
        *self == HealthStyle::default()
    }

    pub fn preset(&self) -> Option<HealthPreset> {
        HealthPreset::ALL.into_iter().find(|p| p.style() == *self)
    }

    pub fn changed_count(&self) -> usize {
        [
            self.number_scale_pct != 100,
            self.color_by_health,
            self.clear_max_health,
            self.hide_backer,
            self.no_shake,
            self.hide_regen,
        ]
        .into_iter()
        .filter(|c| *c)
        .count()
    }

    /// Minified CSS per style file. Deterministic.
    pub fn compile(&self) -> Result<BTreeMap<&'static str, String>, HealthError> {
        if !NUMBER_SCALE_RANGE.contains(&self.number_scale_pct) {
            return Err(HealthError::NumberScale(self.number_scale_pct));
        }
        let mut files: BTreeMap<&'static str, String> = BTreeMap::new();
        let mut add = |file: &'static str, selector: &str, decls: &[(&str, String)]| {
            files
                .entry(file)
                .or_default()
                .push_str(&emit_rule(selector, decls));
        };
        if self.number_scale_pct != 100 {
            let s = f64::from(self.number_scale_pct) / 100.0;
            // The label shrinks text to fit its box, so the box grows with it.
            add(
                HEALTH_CONTAINER_STYLE,
                ".healthContainer",
                &[
                    ("width", px(NUMBER_BOX.0 * s)),
                    ("height", px(NUMBER_BOX.1 * s)),
                ],
            );
            add(
                HEALTH_CONTAINER_STYLE,
                ".currentHealthLabel",
                &[("font-size", px(NUMBER_PX * s))],
            );
            add(
                HEALTH_CONTAINER_STYLE,
                ".localPlayerLowHealth .currentHealthLabel,.healthLow .currentHealthLabel",
                &[("font-size", px(NUMBER_LOW_PX * s))],
            );
        }
        if self.color_by_health {
            add(
                HEALTH_CONTAINER_STYLE,
                ".localPlayerMidHealth .currentHealthLabel,.healthMid .currentHealthLabel",
                &[("color", HURT_COLOR.to_string())],
            );
        }
        if self.clear_max_health {
            add(
                HEALTH_CONTAINER_STYLE,
                ".totalHealthLabel",
                &[("opacity", "0.85".to_string())],
            );
        }
        if self.hide_backer {
            add(
                HEALTH_CONTAINER_STYLE,
                ".healthBacker",
                &[("opacity", "0".to_string())],
            );
        }
        if self.no_shake {
            let none = [("animation-name", "none".to_string())];
            add(
                HEALTH_STYLE,
                ".healthMid .health_bar_line,.healthLow .health_bar_line,.healthLow #current_health,.healthLow #health_bar,.healthMid #health_bar",
                &none,
            );
            add(
                HUD_STYLE,
                "#health_and_abilities_container.localPlayerLowHealth,#health_and_abilities_container.localPlayerMidHealth",
                &none,
            );
        }
        if self.hide_regen {
            add(
                HEALTH_STYLE,
                ".regen_container",
                &[("opacity", "0".to_string())],
            );
        }
        Ok(files)
    }
}

fn px(value: f64) -> String {
    format!("{}px", value.round())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vanilla_emits_nothing_and_presets_round_trip() {
        assert!(HealthStyle::default().compile().unwrap().is_empty());
        assert_eq!(HealthStyle::default().preset(), Some(HealthPreset::Vanilla));
        for p in HealthPreset::ALL {
            assert_eq!(p.style().preset(), Some(p));
        }
        let custom = HealthStyle {
            hide_regen: true,
            ..HealthStyle::default()
        };
        assert_eq!(custom.preset(), None);
        assert_eq!(custom.changed_count(), 1);
    }

    #[test]
    fn a_bigger_number_grows_its_box_too() {
        let s = HealthStyle {
            number_scale_pct: 150,
            ..HealthStyle::default()
        };
        let css = &s.compile().unwrap()[HEALTH_CONTAINER_STYLE];
        assert_eq!(
            css,
            ".healthContainer{width:150px;height:98px;}\
             .currentHealthLabel{font-size:48px;}\
             .localPlayerLowHealth .currentHealthLabel,.healthLow .currentHealthLabel{font-size:54px;}"
        );
    }

    #[test]
    fn big_number_preset_touches_all_three_files() {
        let files = HealthPreset::BigNumber.style().compile().unwrap();
        assert_eq!(
            files.keys().copied().collect::<Vec<_>>(),
            [HUD_STYLE, HEALTH_STYLE, HEALTH_CONTAINER_STYLE]
        );
        assert!(
            files[HEALTH_CONTAINER_STYLE]
                .contains(".healthMid .currentHealthLabel{color:#FFB347;}")
        );
        assert!(files[HEALTH_CONTAINER_STYLE].contains(".healthBacker{opacity:0;}"));
        assert!(files[HEALTH_STYLE].contains("animation-name:none"));
        assert!(files[HUD_STYLE].contains("localPlayerLowHealth"));
        assert!(!files[HEALTH_STYLE].contains("regen_container"));
    }

    #[test]
    fn out_of_range_size_is_refused() {
        let s = HealthStyle {
            number_scale_pct: 300,
            ..HealthStyle::default()
        };
        assert_eq!(s.compile(), Err(HealthError::NumberScale(300)));
    }
}
