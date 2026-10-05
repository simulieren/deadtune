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
use crate::texture::adjust::Rgb;

pub const HEALTH_STYLE: &str = "panorama/styles/hud_health.vcss_c";
pub const HEALTH_CONTAINER_STYLE: &str = "panorama/styles/hud_health_container.vcss_c";
pub const NUMBER_SCALE_RANGE: RangeInclusive<u16> = 80..=200;
pub const THICKNESS_RANGE: RangeInclusive<u16> = 30..=300;
pub const LENGTH_RANGE: RangeInclusive<u16> = 40..=200;

/// Vanilla sizes from `hud_health_container.css`.
const NUMBER_PX: f64 = 32.0;
const NUMBER_LOW_PX: f64 = 36.0;
const NUMBER_BOX: (f64, f64) = (100.0, 65.0);
const HURT_COLOR: &str = "#FFB347";
const LOW_COLOR: &str = "#FF5656";
/// `vivaciousGreen`, the frame's and the backer's wash.
const FRAME_GREEN: &str = "#142304";
/// Every bar stacked in `.health_bar_border`; they share the health bar's size and mask.
const BARS: &str = "#health_bar,#healthLines,#pending_incoming_damage,#pending_incoming_heal,#shield_bar_2,#ratking_armor_bar";
const FILL: &str = "#health_bar .ProgressBarLeft,.team1 #health_bar .ProgressBarLeft,.team2 #health_bar .ProgressBarLeft,.team1.friend #health_bar .ProgressBarLeft,.team2.friend #health_bar .ProgressBarLeft,.healthMid #health_bar .ProgressBarLeft";
const VERT_MASK: &str = "s2r://panorama/images/hud/healthbar/healthbar_backer_vert_mask.vsvg";
const VERT_BORDER: &str = "s2r://panorama/images/hud/healthbar/healthbar_backer_vert_border.vsvg";

/// The bar's outline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarShape {
    /// The game's slanted ruler.
    #[default]
    Ruler,
    /// A plain rectangle.
    Straight,
    /// A rectangle with round ends.
    Rounded,
    /// The game's own narrow wedge (`healthbar_backer_vert_mask`), wide at the full end.
    Wedge,
    /// No bar; the number alone.
    Hidden,
}

impl BarShape {
    pub const ALL: [BarShape; 5] = [
        BarShape::Ruler,
        BarShape::Straight,
        BarShape::Rounded,
        BarShape::Wedge,
        BarShape::Hidden,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BarShape::Ruler => "Ruler",
            BarShape::Straight => "Straight",
            BarShape::Rounded => "Rounded",
            BarShape::Wedge => "Wedge",
            BarShape::Hidden => "None",
        }
    }

    /// The bar's size in game px at 100% thickness and length.
    pub fn base_size(self) -> (f64, f64) {
        match self {
            BarShape::Ruler => (66.0, 212.0),
            BarShape::Straight | BarShape::Rounded | BarShape::Hidden => (28.0, 212.0),
            BarShape::Wedge => (52.0, 212.0),
        }
    }
}

/// Which way the bar stands; it always fills from its bottom end.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarAngle {
    /// The game's lean, 20 degrees.
    #[default]
    Tilted,
    Upright,
    /// Lying down, filling from the left.
    Flat,
}

impl BarAngle {
    pub const ALL: [BarAngle; 3] = [BarAngle::Tilted, BarAngle::Upright, BarAngle::Flat];

    pub fn label(self) -> &'static str {
        match self {
            BarAngle::Tilted => "Tilted",
            BarAngle::Upright => "Upright",
            BarAngle::Flat => "Horizontal",
        }
    }

    /// `.bars_container`'s `rotateZ`, clockwise on screen.
    pub fn degrees(self) -> f64 {
        match self {
            BarAngle::Tilted => -20.0,
            BarAngle::Upright => 0.0,
            BarAngle::Flat => 90.0,
        }
    }
}

/// The health number's typeface, from the families the game's own styles use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumberFont {
    #[default]
    Game,
    Block,
    Sans,
    Mono,
}

impl NumberFont {
    pub const ALL: [NumberFont; 4] = [
        NumberFont::Game,
        NumberFont::Block,
        NumberFont::Sans,
        NumberFont::Mono,
    ];

    pub fn label(self) -> &'static str {
        match self {
            NumberFont::Game => "Game",
            NumberFont::Block => "Block",
            NumberFont::Sans => "Sans",
            NumberFont::Mono => "Mono",
        }
    }

    fn family(self) -> &'static str {
        match self {
            NumberFont::Game => "numericOracle",
            NumberFont::Block => "numericBlock",
            NumberFont::Sans => "numericSans",
            NumberFont::Mono => "sansMono",
        }
    }
}

/// `Default` is vanilla: nothing emitted.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct HealthStyle {
    /// Size of the current-health number, percent of vanilla.
    pub number_scale_pct: u16,
    /// The number turns orange when hurt (vanilla already turns it red when low); a
    /// coloured bar follows it.
    pub color_by_health: bool,
    /// Max health at full strength instead of vanilla's faint 20 %.
    pub clear_max_health: bool,
    /// The green shape behind the health number.
    pub hide_backer: bool,
    /// The shaking and pulsing at mid and low health.
    pub no_shake: bool,
    /// The health regeneration number beside the bar.
    pub hide_regen: bool,
    pub shape: BarShape,
    pub angle: BarAngle,
    /// The bar's width across, percent of its shape's base.
    pub thickness_pct: u16,
    /// The bar's length, percent of its shape's base.
    pub length_pct: u16,
    /// A flat colour instead of the game's paper texture.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<Rgb>,
    /// The tick every 250 health.
    pub ticks: bool,
    /// The dark green outline round the bar.
    pub frame: bool,
    pub font: NumberFont,
    pub hide_number: bool,
    pub hide_max: bool,
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
            shape: BarShape::Ruler,
            angle: BarAngle::Tilted,
            thickness_pct: 100,
            length_pct: 100,
            fill: None,
            ticks: true,
            frame: true,
            font: NumberFont::Game,
            hide_number: false,
            hide_max: false,
        }
    }
}

/// Ready-made looks, grouped for the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HealthPreset {
    Vanilla,
    Clean,
    BigNumber,
    Upright,
    Slim,
    Wedge,
    Blade,
    Horizontal,
    Pill,
    Terminal,
    Minimal,
    NumberOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresetGroup {
    Game,
    Vertical,
    Horizontal,
    Minimal,
}

impl PresetGroup {
    pub const ALL: [PresetGroup; 4] = [
        PresetGroup::Game,
        PresetGroup::Vertical,
        PresetGroup::Horizontal,
        PresetGroup::Minimal,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PresetGroup::Game => "Game look",
            PresetGroup::Vertical => "Vertical",
            PresetGroup::Horizontal => "Horizontal",
            PresetGroup::Minimal => "Minimal and type",
        }
    }
}

const OFF_WHITE: Rgb = Rgb([0xFF, 0xEF, 0xD7]);

impl HealthPreset {
    pub const ALL: [HealthPreset; 12] = [
        HealthPreset::Vanilla,
        HealthPreset::Clean,
        HealthPreset::BigNumber,
        HealthPreset::Upright,
        HealthPreset::Slim,
        HealthPreset::Wedge,
        HealthPreset::Blade,
        HealthPreset::Horizontal,
        HealthPreset::Pill,
        HealthPreset::Terminal,
        HealthPreset::Minimal,
        HealthPreset::NumberOnly,
    ];

    pub fn label(self) -> &'static str {
        match self {
            HealthPreset::Vanilla => "Vanilla",
            HealthPreset::Clean => "Clean",
            HealthPreset::BigNumber => "Big number",
            HealthPreset::Upright => "Upright",
            HealthPreset::Slim => "Slim",
            HealthPreset::Wedge => "Wedge",
            HealthPreset::Blade => "Blade",
            HealthPreset::Horizontal => "Horizontal",
            HealthPreset::Pill => "Pill",
            HealthPreset::Terminal => "Terminal",
            HealthPreset::Minimal => "Minimal",
            HealthPreset::NumberOnly => "Number only",
        }
    }

    pub fn group(self) -> PresetGroup {
        match self {
            HealthPreset::Vanilla
            | HealthPreset::Clean
            | HealthPreset::BigNumber
            | HealthPreset::Upright => PresetGroup::Game,
            HealthPreset::Slim | HealthPreset::Wedge | HealthPreset::Blade => PresetGroup::Vertical,
            HealthPreset::Horizontal | HealthPreset::Pill | HealthPreset::Terminal => {
                PresetGroup::Horizontal
            }
            HealthPreset::Minimal | HealthPreset::NumberOnly => PresetGroup::Minimal,
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            HealthPreset::Vanilla => "The game's own health bar.",
            HealthPreset::Clean => "No backer, no shaking, max health easy to read.",
            HealthPreset::BigNumber => {
                "A big health number that turns orange when hurt, after budhud from TF2."
            }
            HealthPreset::Upright => "The game's ruler standing straight, without the lean.",
            HealthPreset::Slim => "A thin straight bar on paper, ticks kept.",
            HealthPreset::Wedge => "The game's narrow wedge bar, upright and calm.",
            HealthPreset::Blade => "A straight bar with the game's lean and paper.",
            HealthPreset::Horizontal => "A long bar lying flat, green, filling from the left.",
            HealthPreset::Pill => "A rounded amber bar lying flat, no ticks.",
            HealthPreset::Terminal => "A flat cyan bar with a monospaced number.",
            HealthPreset::Minimal => "A hairline bar and a small number, nothing else.",
            HealthPreset::NumberOnly => "No bar at all: one big number that changes colour.",
        }
    }

    pub fn style(self) -> HealthStyle {
        let calm = HealthStyle {
            hide_backer: true,
            no_shake: true,
            clear_max_health: true,
            ..HealthStyle::default()
        };
        match self {
            HealthPreset::Vanilla => HealthStyle::default(),
            HealthPreset::Clean => calm,
            HealthPreset::BigNumber => HealthStyle {
                number_scale_pct: 160,
                color_by_health: true,
                ..calm
            },
            HealthPreset::Upright => HealthStyle {
                angle: BarAngle::Upright,
                no_shake: true,
                ..HealthStyle::default()
            },
            HealthPreset::Slim => HealthStyle {
                shape: BarShape::Straight,
                angle: BarAngle::Upright,
                thickness_pct: 60,
                fill: Some(OFF_WHITE),
                ..calm
            },
            HealthPreset::Wedge => HealthStyle {
                shape: BarShape::Wedge,
                angle: BarAngle::Upright,
                ..calm
            },
            HealthPreset::Blade => HealthStyle {
                shape: BarShape::Straight,
                thickness_pct: 110,
                ..calm
            },
            HealthPreset::Horizontal => HealthStyle {
                shape: BarShape::Straight,
                angle: BarAngle::Flat,
                thickness_pct: 90,
                length_pct: 150,
                fill: Some(Rgb([0x5F, 0xCB, 0x8C])),
                color_by_health: true,
                ..calm
            },
            HealthPreset::Pill => HealthStyle {
                shape: BarShape::Rounded,
                angle: BarAngle::Flat,
                thickness_pct: 80,
                length_pct: 130,
                fill: Some(Rgb([0xF0, 0xB3, 0x41])),
                ticks: false,
                frame: false,
                font: NumberFont::Sans,
                color_by_health: true,
                ..calm
            },
            HealthPreset::Terminal => HealthStyle {
                shape: BarShape::Straight,
                angle: BarAngle::Flat,
                thickness_pct: 60,
                length_pct: 120,
                fill: Some(Rgb([0x2D, 0xDB, 0xF1])),
                font: NumberFont::Mono,
                ..calm
            },
            HealthPreset::Minimal => HealthStyle {
                shape: BarShape::Straight,
                angle: BarAngle::Upright,
                thickness_pct: 30,
                length_pct: 80,
                fill: Some(Rgb::WHITE),
                ticks: false,
                frame: false,
                number_scale_pct: 80,
                font: NumberFont::Sans,
                hide_regen: true,
                hide_max: true,
                ..calm
            },
            HealthPreset::NumberOnly => HealthStyle {
                shape: BarShape::Hidden,
                angle: BarAngle::Upright,
                number_scale_pct: 200,
                font: NumberFont::Block,
                color_by_health: true,
                hide_regen: true,
                ..calm
            },
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum HealthError {
    #[error("health number size {0}% outside 80..=200")]
    NumberScale(u16),
    #[error("health bar thickness {0}% outside 30..=300")]
    Thickness(u16),
    #[error("health bar length {0}% outside 40..=200")]
    Length(u16),
}

impl HealthStyle {
    pub fn is_vanilla(&self) -> bool {
        *self == HealthStyle::default()
    }

    pub fn preset(&self) -> Option<HealthPreset> {
        HealthPreset::ALL.into_iter().find(|p| p.style() == *self)
    }

    pub fn changed_count(&self) -> usize {
        let v = HealthStyle::default();
        [
            self.number_scale_pct != v.number_scale_pct,
            self.color_by_health,
            self.clear_max_health,
            self.hide_backer,
            self.no_shake,
            self.hide_regen,
            self.shape != v.shape,
            self.angle != v.angle,
            self.thickness_pct != v.thickness_pct,
            self.length_pct != v.length_pct,
            self.fill.is_some(),
            !self.ticks,
            !self.frame,
            self.font != v.font,
            self.hide_number,
            self.hide_max,
        ]
        .into_iter()
        .filter(|c| *c)
        .count()
    }

    /// The bar's size in game px.
    pub fn bar_size(&self) -> (f64, f64) {
        let (w, h) = self.shape.base_size();
        (
            w * f64::from(self.thickness_pct) / 100.0,
            h * f64::from(self.length_pct) / 100.0,
        )
    }

    /// Minified CSS per style file. Deterministic.
    pub fn compile(&self) -> Result<BTreeMap<&'static str, String>, HealthError> {
        if !NUMBER_SCALE_RANGE.contains(&self.number_scale_pct) {
            return Err(HealthError::NumberScale(self.number_scale_pct));
        }
        if !THICKNESS_RANGE.contains(&self.thickness_pct) {
            return Err(HealthError::Thickness(self.thickness_pct));
        }
        if !LENGTH_RANGE.contains(&self.length_pct) {
            return Err(HealthError::Length(self.length_pct));
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
        if self.font != NumberFont::Game {
            add(
                HEALTH_CONTAINER_STYLE,
                ".currentHealthLabel,.totalHealthLabel",
                &[("font-family", self.font.family().to_string())],
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
        if self.hide_max {
            add(
                HEALTH_CONTAINER_STYLE,
                ".totalHealthLabel",
                &[("visibility", "collapse".to_string())],
            );
        }
        if self.hide_number {
            add(
                HEALTH_CONTAINER_STYLE,
                ".healthContainer",
                &[("visibility", "collapse".to_string())],
            );
        }
        if self.hide_backer {
            add(
                HEALTH_CONTAINER_STYLE,
                ".healthBacker",
                &[("opacity", "0".to_string())],
            );
        }
        if self.angle != BarAngle::Tilted {
            add(
                HEALTH_CONTAINER_STYLE,
                "#HealthRegenAndTotal",
                &[("transform", "rotateZ(0deg)".to_string())],
            );
        }
        self.compile_bar(&mut add);
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

    /// The bar's outline, size, angle, fill, ticks and frame, into `hud_health`.
    fn compile_bar(&self, add: &mut impl FnMut(&'static str, &str, &[(&str, String)])) {
        if self.shape == BarShape::Hidden {
            add(
                HEALTH_STYLE,
                ".bars_container",
                &[("visibility", "collapse".to_string())],
            );
            return;
        }
        let (w, h) = self.bar_size();
        let resized = self.thickness_pct != 100 || self.length_pct != 100;
        if self.angle != BarAngle::Tilted || resized {
            let mut decls = vec![("transform", format!("rotateZ({}deg)", self.angle.degrees()))];
            if resized || self.shape != BarShape::Ruler {
                decls.push(("width", "fit-children".to_string()));
                decls.push(("height", "fit-children".to_string()));
                decls.push(("max-height", "none".to_string()));
            }
            add(HEALTH_STYLE, ".bars_container", &decls);
        }
        let mask = match self.shape {
            BarShape::Ruler => None,
            BarShape::Wedge => Some(format!("url(\"{VERT_MASK}\")")),
            _ => Some("none".to_string()),
        };
        if resized || mask.is_some() {
            let mut decls = vec![("width", px(w)), ("height", px(h))];
            if let Some(mask) = mask {
                decls.push(("opacity-mask", mask));
            }
            if self.shape == BarShape::Rounded {
                decls.push(("border-radius", px(w.min(h) / 2.0)));
            }
            add(HEALTH_STYLE, BARS, &decls);
        }
        match (self.shape, self.frame) {
            (_, false) => add(
                HEALTH_STYLE,
                "#health_bar_frame",
                &[("visibility", "collapse".to_string())],
            ),
            (BarShape::Ruler, true) if resized => add(
                HEALTH_STYLE,
                "#health_bar_frame",
                &[("width", px(w + 2.0)), ("height", px(h + 8.0))],
            ),
            (BarShape::Ruler, true) => {}
            (BarShape::Wedge, true) => add(
                HEALTH_STYLE,
                "#health_bar_frame",
                &[
                    ("width", px(w + 2.0)),
                    ("height", px(h + 8.0)),
                    ("background-image", format!("url(\"{VERT_BORDER}\")")),
                ],
            ),
            (_, true) => {
                add(
                    HEALTH_STYLE,
                    "#health_bar_frame",
                    &[("visibility", "collapse".to_string())],
                );
                let mut decls = vec![("border", format!("2px solid {FRAME_GREEN}"))];
                if self.shape == BarShape::Rounded {
                    decls.push(("border-radius", px(w.min(h) / 2.0)));
                }
                add(HEALTH_STYLE, "#health_bar", &decls);
            }
        }
        if !self.ticks {
            add(
                HEALTH_STYLE,
                "#healthLines",
                &[("visibility", "collapse".to_string())],
            );
        }
        if let Some(fill) = self.fill {
            add(
                HEALTH_STYLE,
                FILL,
                &[
                    ("background-image", "none".to_string()),
                    ("background-color", fill.hex()),
                ],
            );
            if self.color_by_health {
                add(
                    HEALTH_STYLE,
                    ".healthMid #health_bar .ProgressBarLeft",
                    &[("background-color", HURT_COLOR.to_string())],
                );
                add(
                    HEALTH_STYLE,
                    ".healthLow #health_bar .ProgressBarLeft",
                    &[("background-color", LOW_COLOR.to_string())],
                );
            }
        }
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
    fn every_preset_compiles_and_is_found_again() {
        for p in HealthPreset::ALL {
            assert!(p.style().compile().is_ok(), "{p:?}");
            assert_eq!(p.style().preset(), Some(p), "{p:?}");
        }
        let groups: Vec<PresetGroup> = HealthPreset::ALL.iter().map(|p| p.group()).collect();
        for g in PresetGroup::ALL {
            assert!(groups.contains(&g), "{g:?} has a preset");
        }
    }

    #[test]
    fn a_horizontal_bar_lies_down_unmasked_and_coloured() {
        let files = HealthPreset::Horizontal.style().compile().unwrap();
        let bar = &files[HEALTH_STYLE];
        assert!(
            bar.contains(".bars_container{transform:rotateZ(90deg);width:fit-children;height:fit-children;max-height:none;}"),
            "{bar}"
        );
        assert!(
            bar.contains(&format!(
                "{BARS}{{width:25px;height:318px;opacity-mask:none;}}"
            )),
            "{bar}"
        );
        assert!(bar.contains(
            "#health_bar_frame{visibility:collapse;}#health_bar{border:2px solid #142304;}"
        ));
        assert!(bar.contains("background-image:none;background-color:#5fcb8c;"));
        let mid = bar
            .find(".healthMid #health_bar .ProgressBarLeft{background-color:#FFB347;}")
            .unwrap();
        assert!(
            mid > bar.find("#5fcb8c").unwrap(),
            "the hurt colour comes after the fill"
        );
        assert!(
            files[HEALTH_CONTAINER_STYLE]
                .contains("#HealthRegenAndTotal{transform:rotateZ(0deg);}")
        );
    }

    #[test]
    fn shapes_masks_and_fonts() {
        let wedge = HealthPreset::Wedge.style().compile().unwrap();
        assert!(wedge[HEALTH_STYLE].contains("opacity-mask:url(\"s2r://panorama/images/hud/healthbar/healthbar_backer_vert_mask.vsvg\")"));
        assert!(wedge[HEALTH_STYLE].contains("healthbar_backer_vert_border.vsvg"));
        let pill = HealthPreset::Pill.style().compile().unwrap();
        assert!(pill[HEALTH_STYLE].contains("border-radius:"));
        assert!(pill[HEALTH_STYLE].contains("#healthLines{visibility:collapse;}"));
        assert!(pill[HEALTH_CONTAINER_STYLE].contains("font-family:numericSans"));
        let only = HealthPreset::NumberOnly.style().compile().unwrap();
        assert!(only[HEALTH_STYLE].starts_with(".bars_container{visibility:collapse;}"));
        assert!(!only[HEALTH_STYLE].contains("opacity-mask"));
        let upright = HealthPreset::Upright.style().compile().unwrap();
        assert!(upright[HEALTH_STYLE].starts_with(".bars_container{transform:rotateZ(0deg);}"));
        assert!(
            !upright[HEALTH_STYLE].contains(BARS),
            "the ruler keeps its size"
        );
    }

    #[test]
    fn old_profiles_load_as_the_ruler() {
        let old: HealthStyle =
            toml::from_str("number_scale_pct = 150\nhide_backer = true").unwrap();
        assert_eq!(old.shape, BarShape::Ruler);
        assert_eq!(old.angle, BarAngle::Tilted);
        assert!(old.ticks && old.frame);
        let back: HealthStyle =
            toml::from_str(&toml::to_string(&HealthPreset::Pill.style()).unwrap()).unwrap();
        assert_eq!(back, HealthPreset::Pill.style());
    }

    #[test]
    fn out_of_range_size_is_refused() {
        let s = HealthStyle {
            number_scale_pct: 300,
            ..HealthStyle::default()
        };
        assert_eq!(s.compile(), Err(HealthError::NumberScale(300)));
        let thin = HealthStyle {
            thickness_pct: 10,
            ..HealthStyle::default()
        };
        assert_eq!(thin.compile(), Err(HealthError::Thickness(10)));
        let long = HealthStyle {
            length_pct: 500,
            ..HealthStyle::default()
        };
        assert_eq!(long.compile(), Err(HealthError::Length(500)));
    }
}
