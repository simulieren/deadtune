//! The top bar's look and its optional extras. Presentation options compile to CSS
//! appended to the game's own `citadel_hud_top_bar.vcss_c`; the extras (spawn timers,
//! urn soul lead, purchase popups) add one script and one stylesheet of ours to the top
//! bar layout through `inject`. Every selector keys on the game's own state classes, so
//! nothing is shown that the stock bar does not already know.
//! Ideas: out-of-vision dimming by NA-45 (GameBanana 619963), spawn timers by Stovven and
//! bonclide, urn soul lead by bonclide, purchase popups by bonclide; see
//! research/hud/top-bar/NOTES.md.

use std::collections::BTreeMap;
use std::ops::RangeInclusive;

use super::css::{self, emit_rule};
use super::inject::LayoutEdit;
use super::minimap_colors::Color;

pub const TOP_BAR_STYLE: &str = "panorama/styles/citadel_hud_top_bar.vcss_c";
pub const TOP_BAR_LAYOUT: &str = "panorama/layout/citadel_hud_top_bar.vxml_c";
pub const OWN_SCRIPT: &str = "panorama/scripts/deadtune/top_bar.vjs_c";
pub const OWN_STYLE: &str = "panorama/styles/deadtune/top_bar.vcss_c";

const SCRIPT: &str = include_str!("assets/top_bar.js");
const STYLE: &str = include_str!("assets/top_bar.css");

pub const MISSING_OPACITY_RANGE: RangeInclusive<u8> = 10..=100;
pub const PORTRAIT_SCALE_RANGE: RangeInclusive<u16> = 70..=130;
pub const PORTRAIT_GAP_RANGE: RangeInclusive<u8> = 0..=24;

/// How a dead hero's portrait looks until respawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeadLook {
    #[default]
    Vanilla,
    Grayscale,
    Darken,
    Faded,
}

/// A centre element: as the game draws it, smaller, or gone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Treatment {
    #[default]
    Vanilla,
    Compact,
    Hidden,
}

/// `Default` is vanilla: nothing emitted, no addon files.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct TopBarStyle {
    /// Enemy portraits out of vision (the game's `.HealthVisible` class absent): opacity.
    pub missing_opacity_pct: u8,
    pub missing_desaturate: bool,
    pub missing_darken: bool,
    pub dead: DeadLook,
    /// Percent of the game's portrait size.
    pub portrait_scale_pct: u16,
    /// Extra px between portraits.
    pub portrait_gap_px: u8,
    /// Health bar and souls tag colours per side; `None` keeps the game's.
    pub ally_color: Option<Color>,
    pub enemy_color: Option<Color>,
    pub clock: Treatment,
    pub soul_lead: Treatment,
    pub rejuv_charges: Treatment,
    /// Hides the team kill and ability point rows the scoreboard shows.
    pub hide_kill_counts: bool,
    /// Hides every portrait's souls tag.
    pub hide_player_souls: bool,
    /// Shows hero levels on the bar, not only with the scoreboard open.
    pub show_levels: bool,
    pub spawn_timers: bool,
    pub urn_lead: bool,
    pub purchases: bool,
}

impl Default for TopBarStyle {
    fn default() -> Self {
        TopBarStyle {
            missing_opacity_pct: 100,
            missing_desaturate: false,
            missing_darken: false,
            dead: DeadLook::Vanilla,
            portrait_scale_pct: 100,
            portrait_gap_px: 0,
            ally_color: None,
            enemy_color: None,
            clock: Treatment::Vanilla,
            soul_lead: Treatment::Vanilla,
            rejuv_charges: Treatment::Vanilla,
            hide_kill_counts: false,
            hide_player_souls: false,
            show_levels: false,
            spawn_timers: false,
            urn_lead: false,
            purchases: false,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TopBarError {
    #[error("out-of-vision opacity {0}% outside 10..=100")]
    MissingOpacity(u8),
    #[error("portrait size {0}% outside 70..=130")]
    PortraitScale(u16),
    #[error("portrait gap {0}px outside 0..=24")]
    PortraitGap(u8),
}

/// What the top bar adds to the addon.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TopBarPatch {
    /// Appended to `TOP_BAR_STYLE`.
    pub css: String,
    /// The top bar layout's additions, when an extra is on.
    pub layout: Option<LayoutEdit>,
    /// Our script and stylesheet, when an extra is on.
    pub own_files: BTreeMap<String, String>,
}

impl TopBarStyle {
    pub fn is_vanilla(&self) -> bool {
        *self == TopBarStyle::default()
    }

    pub fn has_extras(&self) -> bool {
        self.spawn_timers || self.urn_lead || self.purchases
    }

    /// Options away from vanilla, for the page's "Changed" count.
    pub fn changed_count(&self) -> usize {
        let d = TopBarStyle::default();
        [
            self.missing_opacity_pct != d.missing_opacity_pct,
            self.missing_desaturate,
            self.missing_darken,
            self.dead != d.dead,
            self.portrait_scale_pct != d.portrait_scale_pct,
            self.portrait_gap_px != d.portrait_gap_px,
            self.ally_color.is_some(),
            self.enemy_color.is_some(),
            self.clock != d.clock,
            self.soul_lead != d.soul_lead,
            self.rejuv_charges != d.rejuv_charges,
            self.hide_kill_counts,
            self.hide_player_souls,
            self.show_levels,
            self.spawn_timers,
            self.urn_lead,
            self.purchases,
        ]
        .into_iter()
        .filter(|&changed| changed)
        .count()
    }

    pub fn validate(&self) -> Result<(), TopBarError> {
        if !MISSING_OPACITY_RANGE.contains(&self.missing_opacity_pct) {
            return Err(TopBarError::MissingOpacity(self.missing_opacity_pct));
        }
        if !PORTRAIT_SCALE_RANGE.contains(&self.portrait_scale_pct) {
            return Err(TopBarError::PortraitScale(self.portrait_scale_pct));
        }
        if !PORTRAIT_GAP_RANGE.contains(&self.portrait_gap_px) {
            return Err(TopBarError::PortraitGap(self.portrait_gap_px));
        }
        Ok(())
    }

    /// Minified CSS for the presentation options, in field order. Deterministic.
    pub fn css(&self) -> Result<String, TopBarError> {
        self.validate()?;
        let mut out = String::new();
        let mut rule = |selector: &str, decls: &[(&str, String)]| {
            if !decls.is_empty() {
                out.push_str(&emit_rule(selector, decls));
            }
        };

        const MISSING: &str = "#TeamEnemy CitadelHudTopBarPlayer:not(.HealthVisible)";
        if self.missing_opacity_pct != 100 {
            rule(
                &format!("{MISSING} #HeroContents"),
                &[(
                    "opacity",
                    number(f64::from(self.missing_opacity_pct) / 100.0),
                )],
            );
        }
        let mut image = Vec::new();
        if self.missing_desaturate {
            image.push(("saturation", "0".to_string()));
        }
        if self.missing_darken {
            image.push(("wash-color", "#00000090".to_string()));
        }
        rule(&format!("{MISSING} #HeroImageArea"), &image);

        match self.dead {
            DeadLook::Vanilla => {}
            DeadLook::Grayscale => rule(".Dead #HeroImageArea", &[("saturation", "0".into())]),
            DeadLook::Darken => rule(
                ".Dead #HeroImageArea",
                &[("wash-color", "#000000A0".into())],
            ),
            DeadLook::Faded => rule(".Dead .HeroContents", &[("opacity", "0.45".into())]),
        }

        let mut player = Vec::new();
        if self.portrait_scale_pct != 100 {
            player.push(("ui-scale", format!("{}%", self.portrait_scale_pct)));
        }
        if self.portrait_gap_px != 0 {
            player.push(("margin", format!("0px {}px", self.portrait_gap_px)));
        }
        rule("CitadelHudTopBarPlayer", &player);

        for (side, color) in [
            ("#TeamFriendly", self.ally_color),
            ("#TeamEnemy", self.enemy_color),
        ] {
            if let Some(color) = color {
                rule(
                    &format!("{side} #HealthBar_Fill,{side} .SoulsValueContainer"),
                    &[("background-color", color.to_string())],
                );
            }
        }

        for (selector, treatment, compact) in [
            (".GameClock", self.clock, "80%"),
            (".TeamNetworth", self.soul_lead, "75%"),
            ("#RejuvenatorCharges", self.rejuv_charges, "80%"),
        ] {
            match treatment {
                Treatment::Vanilla => {}
                Treatment::Compact => rule(selector, &[("ui-scale", compact.into())]),
                Treatment::Hidden => rule(selector, &[("visibility", "collapse".into())]),
            }
        }

        if self.hide_kill_counts {
            rule(
                ".wants_scoreboard .KillsAndAbilityPointsContainer",
                &[("visibility", "collapse".into())],
            );
        }
        if self.hide_player_souls {
            rule(".SoulsValueContainer", &[("visibility", "collapse".into())]);
        }
        if self.show_levels {
            rule(".HeroLevelBacker", &[("visibility", "visible".into())]);
        }
        Ok(out)
    }

    /// The script with this style's extras switched on, or `None` when none is.
    pub fn script(&self) -> Option<String> {
        self.has_extras().then(|| {
            format!(
                "var DT_TOP_BAR = {{ spawnTimers: {}, urnLead: {}, purchases: {} }};\n{SCRIPT}",
                self.spawn_timers, self.urn_lead, self.purchases
            )
        })
    }

    pub fn compile(&self) -> Result<TopBarPatch, TopBarError> {
        let css = self.css()?;
        let mut patch = TopBarPatch {
            css,
            ..TopBarPatch::default()
        };
        if let Some(script) = self.script() {
            patch.layout = Some(LayoutEdit {
                style_includes: vec![format!("s2r://{OWN_STYLE}")],
                script_includes: vec![format!("s2r://{OWN_SCRIPT}")],
                panels: Vec::new(),
            });
            patch.own_files.insert(OWN_SCRIPT.to_string(), script);
            patch.own_files.insert(
                OWN_STYLE.to_string(),
                css::minify(STYLE).expect("the bundled stylesheet is well formed"),
            );
        }
        Ok(patch)
    }
}

/// Shortest decimal form: 0.2, 1.
fn number(value: f64) -> String {
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Starting points on the Top bar page, as plain `TopBarStyle` values to keep editing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopBarPreset {
    Vanilla,
    FightReadability,
    Clean,
    HighContrast,
    TopBarPlus,
    Minimal,
}

impl TopBarPreset {
    pub const ALL: [TopBarPreset; 6] = [
        TopBarPreset::Vanilla,
        TopBarPreset::FightReadability,
        TopBarPreset::Clean,
        TopBarPreset::HighContrast,
        TopBarPreset::TopBarPlus,
        TopBarPreset::Minimal,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TopBarPreset::Vanilla => "Vanilla",
            TopBarPreset::FightReadability => "Fight readability",
            TopBarPreset::Clean => "Clean",
            TopBarPreset::HighContrast => "High contrast",
            TopBarPreset::TopBarPlus => "Top Bar Plus style",
            TopBarPreset::Minimal => "Minimal",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            TopBarPreset::Vanilla => "The game's own top bar.",
            TopBarPreset::FightReadability => {
                "Enemies out of vision fade out, so who is in the fight reads at a glance. Idea by NA-45."
            }
            TopBarPreset::Clean => "Smaller clock and soul lead, no souls tags, no kill rows.",
            TopBarPreset::HighContrast => {
                "Faded and grey when missing, grey when dead, strong team colours."
            }
            TopBarPreset::TopBarPlus => {
                "Spawn timers, urn soul lead and purchase popups, with missing enemies faded. Ideas by bonclide and NA-45."
            }
            TopBarPreset::Minimal => {
                "Spawn timers and urn lead with a smaller soul lead and darkened missing enemies. Idea by Stovven."
            }
        }
    }

    pub fn style(self) -> TopBarStyle {
        let d = TopBarStyle::default();
        match self {
            TopBarPreset::Vanilla => d,
            TopBarPreset::FightReadability => TopBarStyle {
                missing_opacity_pct: 20,
                ..d
            },
            TopBarPreset::Clean => TopBarStyle {
                clock: Treatment::Compact,
                soul_lead: Treatment::Compact,
                rejuv_charges: Treatment::Compact,
                hide_kill_counts: true,
                hide_player_souls: true,
                ..d
            },
            TopBarPreset::HighContrast => TopBarStyle {
                missing_opacity_pct: 35,
                missing_desaturate: true,
                dead: DeadLook::Grayscale,
                ally_color: Some(Color([0x7C, 0xFF, 0x6B, 255])),
                enemy_color: Some(Color([0xFF, 0x4A, 0x4A, 255])),
                ..d
            },
            TopBarPreset::TopBarPlus => TopBarStyle {
                missing_opacity_pct: 20,
                spawn_timers: true,
                urn_lead: true,
                purchases: true,
                ..d
            },
            TopBarPreset::Minimal => TopBarStyle {
                missing_darken: true,
                soul_lead: Treatment::Compact,
                spawn_timers: true,
                urn_lead: true,
                ..d
            },
        }
    }
}

/// `inject::patched_layout` note for the rebuilt top bar layout.
pub fn layout_note() -> String {
    format!(
        "Rebuilt by DeadTune from the game's own {TOP_BAR_LAYOUT}: adds {OWN_STYLE} and {OWN_SCRIPT}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn css(style: &TopBarStyle) -> String {
        style.css().expect("valid")
    }

    #[test]
    fn vanilla_emits_nothing() {
        let s = TopBarStyle::default();
        assert!(s.is_vanilla());
        assert_eq!(s.changed_count(), 0);
        assert_eq!(css(&s), "");
        assert_eq!(s.compile().unwrap(), TopBarPatch::default());
        assert_eq!(s.script(), None);
    }

    #[test]
    fn each_option_emits_its_rule() {
        let d = TopBarStyle::default();
        let cases: Vec<(TopBarStyle, &str)> = vec![
            (
                TopBarStyle {
                    missing_opacity_pct: 20,
                    ..d.clone()
                },
                "#TeamEnemy CitadelHudTopBarPlayer:not(.HealthVisible) #HeroContents{opacity:0.2;}",
            ),
            (
                TopBarStyle {
                    missing_desaturate: true,
                    missing_darken: true,
                    ..d.clone()
                },
                "#TeamEnemy CitadelHudTopBarPlayer:not(.HealthVisible) #HeroImageArea{saturation:0;wash-color:#00000090;}",
            ),
            (
                TopBarStyle {
                    dead: DeadLook::Grayscale,
                    ..d.clone()
                },
                ".Dead #HeroImageArea{saturation:0;}",
            ),
            (
                TopBarStyle {
                    dead: DeadLook::Darken,
                    ..d.clone()
                },
                ".Dead #HeroImageArea{wash-color:#000000A0;}",
            ),
            (
                TopBarStyle {
                    dead: DeadLook::Faded,
                    ..d.clone()
                },
                ".Dead .HeroContents{opacity:0.45;}",
            ),
            (
                TopBarStyle {
                    portrait_scale_pct: 85,
                    portrait_gap_px: 6,
                    ..d.clone()
                },
                "CitadelHudTopBarPlayer{ui-scale:85%;margin:0px 6px;}",
            ),
            (
                TopBarStyle {
                    enemy_color: Some(Color([255, 0x4A, 0x4A, 255])),
                    ..d.clone()
                },
                "#TeamEnemy #HealthBar_Fill,#TeamEnemy .SoulsValueContainer{background-color:#FF4A4A;}",
            ),
            (
                TopBarStyle {
                    ally_color: Some(Color([0x7C, 0xFF, 0x6B, 255])),
                    ..d.clone()
                },
                "#TeamFriendly #HealthBar_Fill,#TeamFriendly .SoulsValueContainer{background-color:#7CFF6B;}",
            ),
            (
                TopBarStyle {
                    clock: Treatment::Compact,
                    ..d.clone()
                },
                ".GameClock{ui-scale:80%;}",
            ),
            (
                TopBarStyle {
                    soul_lead: Treatment::Hidden,
                    ..d.clone()
                },
                ".TeamNetworth{visibility:collapse;}",
            ),
            (
                TopBarStyle {
                    rejuv_charges: Treatment::Compact,
                    ..d.clone()
                },
                "#RejuvenatorCharges{ui-scale:80%;}",
            ),
            (
                TopBarStyle {
                    hide_kill_counts: true,
                    ..d.clone()
                },
                ".wants_scoreboard .KillsAndAbilityPointsContainer{visibility:collapse;}",
            ),
            (
                TopBarStyle {
                    hide_player_souls: true,
                    ..d.clone()
                },
                ".SoulsValueContainer{visibility:collapse;}",
            ),
            (
                TopBarStyle {
                    show_levels: true,
                    ..d.clone()
                },
                ".HeroLevelBacker{visibility:visible;}",
            ),
        ];
        for (style, want) in cases {
            assert_eq!(css(&style), want);
            assert!(style.changed_count() >= 1, "{style:?}");
            assert!(!style.has_extras());
            assert!(style.compile().unwrap().layout.is_none());
        }
    }

    #[test]
    fn rules_follow_field_order() {
        let style = TopBarStyle {
            show_levels: true,
            missing_opacity_pct: 50,
            clock: Treatment::Hidden,
            ..TopBarStyle::default()
        };
        assert_eq!(
            css(&style),
            "#TeamEnemy CitadelHudTopBarPlayer:not(.HealthVisible) #HeroContents{opacity:0.5;}\
             .GameClock{visibility:collapse;}\
             .HeroLevelBacker{visibility:visible;}"
        );
        assert_eq!(style.changed_count(), 3);
    }

    #[test]
    fn extras_add_the_script_and_stylesheet_through_a_layout_edit() {
        let style = TopBarStyle {
            spawn_timers: true,
            purchases: true,
            ..TopBarStyle::default()
        };
        let patch = style.compile().unwrap();
        assert_eq!(patch.css, "");
        let edit = patch.layout.expect("layout edit");
        assert_eq!(
            edit.style_includes,
            ["s2r://panorama/styles/deadtune/top_bar.vcss_c"]
        );
        assert_eq!(
            edit.script_includes,
            ["s2r://panorama/scripts/deadtune/top_bar.vjs_c"]
        );
        assert!(edit.panels.is_empty(), "panels are created by the script");
        let script = &patch.own_files[OWN_SCRIPT];
        assert!(script.starts_with(
            "var DT_TOP_BAR = { spawnTimers: true, urnLead: false, purchases: true };\n(function () {"
        ));
        assert!(script.ends_with("tick();\n})();\n"));
        let sheet = &patch.own_files[OWN_STYLE];
        assert!(
            sheet.starts_with("#DtSpawnTimers{horizontal-align:center;"),
            "{sheet}"
        );
        assert!(!sheet.contains('\n'));
        assert!(css::parse_rules(sheet).unwrap().len() > 20);
        for name in ["offWhite", "courageBrightColor", "ingameHudBlur"] {
            assert!(
                !sheet.contains(name),
                "our stylesheet uses no game defines: {name}"
            );
        }
        assert!(super::super::inject::script_resource(script).len() > script.len());
    }

    #[test]
    fn out_of_range_values_are_refused() {
        let d = TopBarStyle::default();
        assert_eq!(
            TopBarStyle {
                missing_opacity_pct: 5,
                ..d.clone()
            }
            .css(),
            Err(TopBarError::MissingOpacity(5))
        );
        assert_eq!(
            TopBarStyle {
                portrait_scale_pct: 131,
                ..d.clone()
            }
            .css(),
            Err(TopBarError::PortraitScale(131))
        );
        assert_eq!(
            TopBarStyle {
                portrait_gap_px: 25,
                ..d
            }
            .compile()
            .unwrap_err(),
            TopBarError::PortraitGap(25)
        );
    }

    #[test]
    fn presets_are_distinct_and_valid_and_vanilla_is_default() {
        assert_eq!(TopBarPreset::Vanilla.style(), TopBarStyle::default());
        let mut seen = Vec::new();
        for preset in TopBarPreset::ALL {
            let style = preset.style();
            style.compile().unwrap();
            assert!(
                !seen.contains(&style),
                "{preset:?} duplicates another preset"
            );
            seen.push(style);
        }
        assert_eq!(
            TopBarPreset::FightReadability.style().css(),
            Ok(
                "#TeamEnemy CitadelHudTopBarPlayer:not(.HealthVisible) #HeroContents{opacity:0.2;}"
                    .into()
            )
        );
        assert!(TopBarPreset::TopBarPlus.style().has_extras());
        assert!(TopBarPreset::Minimal.style().has_extras());
        assert!(!TopBarPreset::Clean.style().has_extras());
    }

    #[test]
    fn toml_round_trip_skips_nothing_and_reads_partial_tables() {
        let style = TopBarPreset::HighContrast.style();
        let text = toml::to_string(&style).unwrap();
        assert!(text.contains("dead = \"grayscale\""), "{text}");
        assert!(text.contains("enemy_color = \"#FF4A4A\""), "{text}");
        assert_eq!(toml::from_str::<TopBarStyle>(&text).unwrap(), style);
        let partial: TopBarStyle =
            toml::from_str("spawn_timers = true\nclock = \"compact\"\n").unwrap();
        assert_eq!(
            partial,
            TopBarStyle {
                spawn_timers: true,
                clock: Treatment::Compact,
                ..TopBarStyle::default()
            }
        );
    }
}
