//! Experimental: the lower-left player stats cluster, part by part. Not yet tested in
//! game.
//!
//! Seven parts (the three category numbers, their gain popups, the level jar, the souls
//! count, the item grid, the quickbuy slot and the status effects) can each be hidden,
//! moved, resized and faded, and most have details of their own. Every rule is appended
//! to the stylesheet where the game styles that panel, keyed on the game's own ids and
//! classes. Moves are `transform` translations and sizes `pre-transform-scale2d` about the
//! panel's middle, which no vanilla rule sets on these panels, so the game's own margins
//! (and their shop-open variants) stay in charge of the layout. The item grid sizes with
//! `ui-scale` instead, the way the game itself enlarges it, so it grows from its corner.
//! Selectors, sizes and risks: research/hud/player-stats/NOTES.md.

use std::collections::BTreeMap;
use std::ops::RangeInclusive;

use super::css::emit_rule;
use super::elements::HUD_STYLE;
use super::minimap_colors::Color;

pub const ACTIVE_STATS_STYLE: &str = "panorama/styles/citadel_hud_active_player_stats.vcss_c";
pub const GOLD_STYLE: &str = "panorama/styles/hud_gold_and_ap_container.vcss_c";
pub const LEVEL_STYLE: &str = "panorama/styles/citadel_hud_player_level.vcss_c";
pub const MOD_ICON_STYLE: &str = "panorama/styles/citadel_shop_mod_icon.vcss_c";
pub const MODS_PANEL_STYLE: &str = "panorama/styles/citadel_mods_purchased_panel.vcss_c";
pub const QUICKBUY_STYLE: &str = "panorama/styles/hud_quickbuy.vcss_c";
pub const STATUS_STYLE: &str = "panorama/styles/citadel_status_effect.vcss_c";

/// Every stylesheet this page may append to.
pub const STYLES: [&str; 8] = [
    HUD_STYLE,
    ACTIVE_STATS_STYLE,
    GOLD_STYLE,
    LEVEL_STYLE,
    MOD_ICON_STYLE,
    MODS_PANEL_STYLE,
    QUICKBUY_STYLE,
    STATUS_STYLE,
];

pub const OFFSET_RANGE: RangeInclusive<i16> = -900..=900;
pub const SCALE_RANGE: RangeInclusive<u16> = 50..=250;
pub const NUMBER_PX_RANGE: RangeInclusive<u8> = 12..=64;
pub const SOULS_PX_RANGE: RangeInclusive<u8> = 12..=72;
pub const TILE_GAP_RANGE: RangeInclusive<u8> = 0..=12;

/// The game's own sizes.
pub const NUMBER_PX: u8 = 26;
pub const LEVEL_PX: u8 = 28;
pub const SOULS_PX: u8 = 32;
pub const TILE_GAP_PX: u8 = 3;
/// `CitadelModIcon`'s 45 px tile.
const TILE_PX: u32 = 45;
/// `.ModsContainer{ui-scale:120%}`.
const ITEMS_UI_SCALE: u32 = 120;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum StatsPart {
    Numbers,
    Popups,
    Level,
    Souls,
    Items,
    Quickbuy,
    StatusEffects,
}

/// How a part's size is set in game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sizing {
    /// `pre-transform-scale2d`, about the panel's middle.
    Middle,
    /// `ui-scale` times the game's own `base` percent. The panel keeps its margins and
    /// grows from its bottom-left corner.
    UiScale { base: u32 },
    /// Not resizable (the popups' panel is far larger than what shows).
    Fixed,
}

#[derive(Clone, Copy, Debug)]
pub struct PartSpec {
    pub label: &'static str,
    pub blurb: &'static str,
    /// The panel, scoped so only this cluster's copy matches.
    pub selector: &'static str,
    pub file: &'static str,
    pub sizing: Sizing,
    /// The game's own opacity, percent.
    pub vanilla_opacity: u8,
}

impl StatsPart {
    pub const ALL: [StatsPart; 7] = [
        StatsPart::Numbers,
        StatsPart::Popups,
        StatsPart::Level,
        StatsPart::Souls,
        StatsPart::Items,
        StatsPart::Quickbuy,
        StatsPart::StatusEffects,
    ];

    pub fn spec(self) -> PartSpec {
        match self {
            StatsPart::Numbers => PartSpec {
                label: "Category numbers",
                blurb: "Weapon, spirit and vitality totals over their icons.",
                selector: "#hudActivePlayerStats #HudStatBlock",
                file: ACTIVE_STATS_STYLE,
                sizing: Sizing::Middle,
                vanilla_opacity: 100,
            },
            StatsPart::Popups => PartSpec {
                label: "Gain popups",
                blurb: "The green +18 boxes that pop up when a stat goes up.",
                selector: "#hudActivePlayerStats #StatList",
                file: ACTIVE_STATS_STYLE,
                sizing: Sizing::Fixed,
                vanilla_opacity: 100,
            },
            StatsPart::Level => PartSpec {
                label: "Level jar",
                blurb: "Your level in the soul jar that fills towards the next one.",
                selector: "#gold_and_ap_container #PlayerLevelContainer",
                file: GOLD_STYLE,
                sizing: Sizing::Middle,
                vanilla_opacity: 100,
            },
            StatsPart::Souls => PartSpec {
                label: "Souls",
                blurb: "The souls you have to spend, with their icon and label.",
                selector: "#gold_and_ap_container #hudGoldContainer",
                file: GOLD_STYLE,
                sizing: Sizing::Middle,
                vanilla_opacity: 100,
            },
            StatsPart::Items => PartSpec {
                label: "Item grid",
                blurb: "The items you bought, two rows, and the flex slots.",
                selector: "#StatsAndModsContainer .ModsContainer",
                file: HUD_STYLE,
                sizing: Sizing::UiScale {
                    base: ITEMS_UI_SCALE,
                },
                vanilla_opacity: 100,
            },
            StatsPart::Quickbuy => PartSpec {
                label: "Quickbuy slot",
                blurb: "The next item to buy. Faint until you can afford it, then fully lit.",
                selector: "#StatsAndModsContainer #HudMini",
                file: QUICKBUY_STYLE,
                sizing: Sizing::Middle,
                vanilla_opacity: 20,
            },
            StatsPart::StatusEffects => PartSpec {
                label: "Status effects",
                blurb: "Buff and debuff icons above the souls.",
                selector: "#StatsAndModsContainer #LowerLeft CitadelStatusEffect",
                file: STATUS_STYLE,
                sizing: Sizing::Middle,
                vanilla_opacity: 100,
            },
        }
    }
}

/// One part's placement. `Default` is the game's own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct PartEdit {
    pub hidden: bool,
    /// Game px at 1080p, right and down.
    pub offset_x: i16,
    pub offset_y: i16,
    /// Percent of the game's size; 0 reads as 100 so a missing key stays vanilla.
    #[serde(skip_serializing_if = "is_zero_or_100")]
    pub scale_pct: u16,
    /// `None` keeps the game's.
    pub opacity_pct: Option<u8>,
}

fn is_zero_or_100(v: &u16) -> bool {
    *v == 0 || *v == 100
}

impl PartEdit {
    pub fn scale(&self) -> u16 {
        if self.scale_pct == 0 {
            100
        } else {
            self.scale_pct
        }
    }

    pub fn is_vanilla(&self) -> bool {
        !self.hidden
            && self.offset_x == 0
            && self.offset_y == 0
            && self.scale() == 100
            && self.opacity_pct.is_none()
    }
}

/// `Default` is vanilla: nothing emitted.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct PlayerStatsStyle {
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub parts: BTreeMap<StatsPart, PartEdit>,
    /// Category numbers.
    pub number_px: u8,
    pub number_color: Option<Color>,
    /// The game tilts the numbers 3 degrees.
    pub straight_numbers: bool,
    /// The category icons behind the numbers; the game draws them at 50 %.
    pub icon_opacity_pct: u8,
    /// Weapon, spirit and vitality: the icons behind the numbers and the item corners.
    pub weapon_color: Option<Color>,
    pub spirit_color: Option<Color>,
    pub vitality_color: Option<Color>,
    /// The small "+18" beside a number that just changed.
    pub hide_deltas: bool,
    /// The thin bar beside each number.
    pub hide_bars: bool,
    /// The glow behind a number that spikes or maxes out.
    pub hide_glow: bool,
    /// Level jar.
    pub level_px: u8,
    pub level_color: Option<Color>,
    pub jar_color: Option<Color>,
    /// Souls.
    pub souls_px: u8,
    pub souls_color: Option<Color>,
    pub hide_souls_icon: bool,
    pub hide_souls_label: bool,
    /// Item grid: padding around each tile.
    pub tile_gap_px: u8,
    /// Slots you haven't filled; the game draws them at 80 %.
    pub empty_opacity_pct: u8,
    /// The dark cover on an item that is cooling down; the game's is 93 %.
    pub cooldown_pct: u8,
    pub hide_tiers: bool,
    pub hide_upgrades: bool,
    /// The locked flex slots beside the grid.
    pub hide_flex: bool,
}

impl Default for PlayerStatsStyle {
    fn default() -> Self {
        PlayerStatsStyle {
            parts: BTreeMap::new(),
            number_px: NUMBER_PX,
            number_color: None,
            straight_numbers: false,
            icon_opacity_pct: 50,
            weapon_color: None,
            spirit_color: None,
            vitality_color: None,
            hide_deltas: false,
            hide_bars: false,
            hide_glow: false,
            level_px: LEVEL_PX,
            level_color: None,
            jar_color: None,
            souls_px: SOULS_PX,
            souls_color: None,
            hide_souls_icon: false,
            hide_souls_label: false,
            tile_gap_px: TILE_GAP_PX,
            empty_opacity_pct: 80,
            cooldown_pct: 93,
            hide_tiers: false,
            hide_upgrades: false,
            hide_flex: false,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PlayerStatsError {
    #[error("{0:?}: offset {1} outside -900..=900")]
    Offset(StatsPart, i16),
    #[error("{0:?}: size {1}% outside 50..=250")]
    Scale(StatsPart, u16),
    #[error("{0:?}: opacity {1}% above 100")]
    Opacity(StatsPart, u8),
    #[error("{0} {1}px outside {2}")]
    Px(&'static str, u8, &'static str),
    #[error("{0} {1}% above 100")]
    Percent(&'static str, u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatsPreset {
    Vanilla,
    Clean,
    Compact,
    BigNumbers,
}

impl StatsPreset {
    pub const ALL: [StatsPreset; 4] = [
        StatsPreset::Vanilla,
        StatsPreset::Clean,
        StatsPreset::Compact,
        StatsPreset::BigNumbers,
    ];

    pub fn label(self) -> &'static str {
        match self {
            StatsPreset::Vanilla => "Vanilla",
            StatsPreset::Clean => "Clean",
            StatsPreset::Compact => "Compact",
            StatsPreset::BigNumbers => "Big numbers",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            StatsPreset::Vanilla => "The game's own player stats.",
            StatsPreset::Clean => {
                "No gain popups, change numbers, glow or SOULS label; straight numbers."
            }
            StatsPreset::Compact => "Everything a little smaller and tighter.",
            StatsPreset::BigNumbers => "Bigger category numbers, level and souls.",
        }
    }

    pub fn style(self) -> PlayerStatsStyle {
        let d = PlayerStatsStyle::default();
        match self {
            StatsPreset::Vanilla => d,
            StatsPreset::Clean => PlayerStatsStyle {
                parts: BTreeMap::from([(
                    StatsPart::Popups,
                    PartEdit {
                        hidden: true,
                        ..PartEdit::default()
                    },
                )]),
                straight_numbers: true,
                hide_deltas: true,
                hide_glow: true,
                hide_souls_label: true,
                ..d
            },
            StatsPreset::Compact => {
                let small = |scale_pct| PartEdit {
                    scale_pct,
                    ..PartEdit::default()
                };
                PlayerStatsStyle {
                    parts: BTreeMap::from([
                        (StatsPart::Numbers, small(85)),
                        (StatsPart::Level, small(85)),
                        (StatsPart::Souls, small(85)),
                        (StatsPart::Items, small(90)),
                    ]),
                    tile_gap_px: 2,
                    ..d
                }
            }
            StatsPreset::BigNumbers => PlayerStatsStyle {
                number_px: 34,
                level_px: 34,
                souls_px: 40,
                ..d
            },
        }
    }
}

impl PlayerStatsStyle {
    pub fn is_vanilla(&self) -> bool {
        *self == PlayerStatsStyle::default()
    }

    pub fn part(&self, part: StatsPart) -> PartEdit {
        self.parts.get(&part).copied().unwrap_or_default()
    }

    /// Stores `edit`, dropping it when it is the game's own.
    pub fn set_part(&mut self, part: StatsPart, edit: PartEdit) {
        if edit.is_vanilla() {
            self.parts.remove(&part);
        } else {
            self.parts.insert(part, edit);
        }
    }

    pub fn preset(&self) -> Option<StatsPreset> {
        StatsPreset::ALL.into_iter().find(|p| p.style() == *self)
    }

    /// Settings away from vanilla, for the page's "Changed" count.
    pub fn changed_count(&self) -> usize {
        let d = PlayerStatsStyle::default();
        let parts = self.parts.values().filter(|e| !e.is_vanilla()).count();
        parts
            + [
                self.number_px != d.number_px,
                self.number_color.is_some(),
                self.straight_numbers,
                self.icon_opacity_pct != d.icon_opacity_pct,
                self.weapon_color.is_some(),
                self.spirit_color.is_some(),
                self.vitality_color.is_some(),
                self.hide_deltas,
                self.hide_bars,
                self.hide_glow,
                self.level_px != d.level_px,
                self.level_color.is_some(),
                self.jar_color.is_some(),
                self.souls_px != d.souls_px,
                self.souls_color.is_some(),
                self.hide_souls_icon,
                self.hide_souls_label,
                self.tile_gap_px != d.tile_gap_px,
                self.empty_opacity_pct != d.empty_opacity_pct,
                self.cooldown_pct != d.cooldown_pct,
                self.hide_tiers,
                self.hide_upgrades,
                self.hide_flex,
            ]
            .into_iter()
            .filter(|&b| b)
            .count()
    }

    /// Settings of one part away from vanilla, placement included.
    pub fn part_changed_count(&self, part: StatsPart) -> usize {
        let d = PlayerStatsStyle::default();
        let details: &[bool] = &match part {
            StatsPart::Numbers => vec![
                self.number_px != d.number_px,
                self.number_color.is_some(),
                self.straight_numbers,
                self.icon_opacity_pct != d.icon_opacity_pct,
                self.weapon_color.is_some(),
                self.spirit_color.is_some(),
                self.vitality_color.is_some(),
                self.hide_deltas,
                self.hide_bars,
                self.hide_glow,
            ],
            StatsPart::Level => vec![
                self.level_px != d.level_px,
                self.level_color.is_some(),
                self.jar_color.is_some(),
            ],
            StatsPart::Souls => vec![
                self.souls_px != d.souls_px,
                self.souls_color.is_some(),
                self.hide_souls_icon,
                self.hide_souls_label,
            ],
            StatsPart::Items => vec![
                self.tile_gap_px != d.tile_gap_px,
                self.empty_opacity_pct != d.empty_opacity_pct,
                self.cooldown_pct != d.cooldown_pct,
                self.hide_tiers,
                self.hide_upgrades,
                self.hide_flex,
            ],
            StatsPart::Popups | StatsPart::Quickbuy | StatsPart::StatusEffects => vec![],
        };
        usize::from(!self.part(part).is_vanilla()) + details.iter().filter(|&&b| b).count()
    }

    /// Back to the game's own for one part: its placement and its details.
    pub fn reset_part(&mut self, part: StatsPart) {
        let d = PlayerStatsStyle::default();
        self.parts.remove(&part);
        match part {
            StatsPart::Numbers => {
                self.number_px = d.number_px;
                self.number_color = None;
                self.straight_numbers = false;
                self.icon_opacity_pct = d.icon_opacity_pct;
                self.weapon_color = None;
                self.spirit_color = None;
                self.vitality_color = None;
                self.hide_deltas = false;
                self.hide_bars = false;
                self.hide_glow = false;
            }
            StatsPart::Level => {
                self.level_px = d.level_px;
                self.level_color = None;
                self.jar_color = None;
            }
            StatsPart::Souls => {
                self.souls_px = d.souls_px;
                self.souls_color = None;
                self.hide_souls_icon = false;
                self.hide_souls_label = false;
            }
            StatsPart::Items => {
                self.tile_gap_px = d.tile_gap_px;
                self.empty_opacity_pct = d.empty_opacity_pct;
                self.cooldown_pct = d.cooldown_pct;
                self.hide_tiers = false;
                self.hide_upgrades = false;
                self.hide_flex = false;
            }
            StatsPart::Popups | StatsPart::Quickbuy | StatsPart::StatusEffects => {}
        }
    }

    pub fn validate(&self) -> Result<(), PlayerStatsError> {
        for (&part, edit) in &self.parts {
            for offset in [edit.offset_x, edit.offset_y] {
                if !OFFSET_RANGE.contains(&offset) {
                    return Err(PlayerStatsError::Offset(part, offset));
                }
            }
            if !SCALE_RANGE.contains(&edit.scale()) {
                return Err(PlayerStatsError::Scale(part, edit.scale()));
            }
            if let Some(o) = edit.opacity_pct.filter(|&o| o > 100) {
                return Err(PlayerStatsError::Opacity(part, o));
            }
        }
        for (name, px, range, text) in [
            ("number size", self.number_px, &NUMBER_PX_RANGE, "12..=64"),
            ("level size", self.level_px, &NUMBER_PX_RANGE, "12..=64"),
            ("souls size", self.souls_px, &SOULS_PX_RANGE, "12..=72"),
            ("tile gap", self.tile_gap_px, &TILE_GAP_RANGE, "0..=12"),
        ] {
            if !range.contains(&px) {
                return Err(PlayerStatsError::Px(name, px, text));
            }
        }
        for (name, pct) in [
            ("icon opacity", self.icon_opacity_pct),
            ("empty slot opacity", self.empty_opacity_pct),
            ("cooldown cover", self.cooldown_pct),
        ] {
            if pct > 100 {
                return Err(PlayerStatsError::Percent(name, pct));
            }
        }
        Ok(())
    }

    /// CSS to append, by stylesheet path, in `STYLES` order. Empty when vanilla.
    pub fn compile(&self) -> Result<Vec<(&'static str, String)>, PlayerStatsError> {
        self.validate()?;
        let mut files: BTreeMap<&'static str, String> = BTreeMap::new();
        let mut add = |file: &'static str, selector: &str, decls: &[(&str, String)]| {
            if !decls.is_empty() {
                files
                    .entry(file)
                    .or_default()
                    .push_str(&emit_rule(selector, decls));
            }
        };
        for part in StatsPart::ALL {
            let spec = part.spec();
            add(spec.file, spec.selector, &placement(spec, self.part(part)));
        }
        let d = PlayerStatsStyle::default();
        let collapse = || vec![("visibility", "collapse".to_string())];

        let numbers = "#hudActivePlayerStats #HudStatBlock #CoreStats";
        let mut number = Vec::new();
        if self.number_px != d.number_px {
            number.push(("font-size", format!("{}px", self.number_px)));
        }
        if let Some(c) = self.number_color {
            number.push(("color", c.to_string()));
        }
        if self.straight_numbers {
            number.push(("transform", "rotateZ(0deg)".into()));
        }
        add(
            ACTIVE_STATS_STYLE,
            &format!("{numbers} .statNumber"),
            &number,
        );
        if self.icon_opacity_pct != d.icon_opacity_pct {
            add(
                ACTIVE_STATS_STYLE,
                &format!("{numbers} .core_bg"),
                &[("opacity", fraction(self.icon_opacity_pct))],
            );
        }
        for (id, tier, color) in self.category_colors() {
            add(
                ACTIVE_STATS_STYLE,
                &format!("{numbers} #{id} .core_bg"),
                &[("wash-color", color.to_string())],
            );
            add(
                MOD_ICON_STYLE,
                &format!("#ModsContainer .{tier} .tier_bg"),
                &[("wash-color", color.to_string())],
            );
        }
        if self.hide_deltas {
            add(
                ACTIVE_STATS_STYLE,
                &format!("{numbers} .statNumberDelta,{numbers} .has_delta .statNumberDelta"),
                &collapse(),
            );
        }
        if self.hide_bars {
            add(
                ACTIVE_STATS_STYLE,
                "#hudActivePlayerStats #HudStatBlock #BarGraphContainer",
                &collapse(),
            );
        }
        if self.hide_glow {
            add(
                ACTIVE_STATS_STYLE,
                "#hudActivePlayerStats #CoreStatFXLayer",
                &collapse(),
            );
        }

        let mut level = Vec::new();
        if self.level_px != d.level_px {
            level.push(("font-size", format!("{}px", self.level_px)));
        }
        if let Some(c) = self.level_color {
            level.push(("color", c.to_string()));
        }
        add(LEVEL_STYLE, "CitadelPlayerLevel #PlayerLevelNumber", &level);
        if let Some(c) = self.jar_color {
            add(
                LEVEL_STYLE,
                "CitadelPlayerLevel #SoulsFrame,CitadelPlayerLevel #SoulsFill",
                &[("wash-color", c.to_string())],
            );
        }

        let souls = "#hudGoldContainer #CurrentGoldAmount";
        let mut count = Vec::new();
        if self.souls_px != d.souls_px {
            count.push(("font-size", format!("{}px", self.souls_px)));
        }
        if let Some(c) = self.souls_color {
            count.push(("color", c.to_string()));
        }
        add(GOLD_STYLE, &format!("{souls} #hudCurGoldLabel"), &count);
        if let Some(c) = self.souls_color {
            add(
                GOLD_STYLE,
                &format!("{souls} #hudCurGoldIcon"),
                &[("wash-color", c.to_string())],
            );
        }
        if self.hide_souls_icon {
            add(GOLD_STYLE, &format!("{souls} #hudCurGoldIcon"), &collapse());
        }
        if self.hide_souls_label {
            add(GOLD_STYLE, &format!("{souls} #hudGoldLabel"), &collapse());
        }

        if self.tile_gap_px != d.tile_gap_px {
            let gap = u32::from(self.tile_gap_px);
            let rows = 2 * (TILE_PX + 2 * gap);
            add(
                MOD_ICON_STYLE,
                "#ModsContainer CitadelModIcon",
                &[("padding", format!("{gap}px"))],
            );
            add(
                HUD_STYLE,
                "#StatsAndModsContainer .ModsContainer",
                &[("height", format!("{rows}px"))],
            );
            add(
                MODS_PANEL_STYLE,
                "#ModsContainer CitadelModsPurchasedPanel",
                &[("height", format!("{rows}px"))],
            );
            add(
                MODS_PANEL_STYLE,
                "#ModsContainer #ModPurchasedPanelUniversalLocked",
                &[("width", format!("{}px", rows + 2))],
            );
        }
        if self.empty_opacity_pct != d.empty_opacity_pct {
            add(
                MOD_ICON_STYLE,
                "#ModsContainer .unowned.mod_icon_single_container",
                &[("opacity", fraction(self.empty_opacity_pct))],
            );
        }
        if self.cooldown_pct != d.cooldown_pct {
            let alpha = (u32::from(self.cooldown_pct) * 255 + 50) / 100;
            add(
                MOD_ICON_STYLE,
                "#ModsContainer #CooldownMask",
                &[("background-color", format!("#10130D{alpha:02X}"))],
            );
        }
        if self.hide_tiers {
            add(MOD_ICON_STYLE, "#ModsContainer #TierContainer", &collapse());
        }
        if self.hide_upgrades {
            add(
                MOD_ICON_STYLE,
                "#ModsContainer #UpgradeLevelContainer",
                &collapse(),
            );
        }
        if self.hide_flex {
            add(
                HUD_STYLE,
                "#ModsContainer #ModPurchasedPanelUniversalLocked",
                &collapse(),
            );
        }
        Ok(STYLES
            .into_iter()
            .filter_map(|file| files.remove(file).map(|css| (file, css)))
            .collect())
    }

    /// The category colours that are set: panel id, item tile class, colour.
    fn category_colors(&self) -> Vec<(&'static str, &'static str, Color)> {
        [
            ("Weapon", "isWeapon", self.weapon_color),
            ("Spirit", "isTech", self.spirit_color),
            ("Vitality", "isArmor", self.vitality_color),
        ]
        .into_iter()
        .filter_map(|(id, tier, c)| c.map(|c| (id, tier, c)))
        .collect()
    }
}

/// A part's hide, move, size and fade, as declarations on its panel.
fn placement(spec: PartSpec, edit: PartEdit) -> Vec<(&'static str, String)> {
    let mut decls = Vec::new();
    if edit.hidden {
        decls.push(("visibility", "collapse".to_string()));
    }
    if edit.offset_x != 0 || edit.offset_y != 0 {
        decls.push((
            "transform",
            format!(
                "translateX({}px) translateY({}px)",
                edit.offset_x, edit.offset_y
            ),
        ));
    }
    let scale = edit.scale();
    if scale != 100 {
        match spec.sizing {
            Sizing::Middle => {
                decls.push(("pre-transform-scale2d", number(f64::from(scale) / 100.0)));
            }
            Sizing::UiScale { base } => {
                let pct = (base * u32::from(scale) + 50) / 100;
                decls.push(("ui-scale", format!("{pct}%")));
            }
            Sizing::Fixed => {}
        }
    }
    if let Some(o) = edit.opacity_pct.filter(|&o| o != spec.vanilla_opacity) {
        decls.push(("opacity", fraction(o)));
    }
    decls
}

fn fraction(pct: u8) -> String {
    number(f64::from(pct) / 100.0)
}

fn number(value: f64) -> String {
    let text = format!("{value:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::css;

    fn sheet<'a>(out: &'a [(&str, String)], file: &str) -> &'a str {
        out.iter()
            .find(|(f, _)| *f == file)
            .map(|(_, css)| css.as_str())
            .unwrap_or("")
    }

    #[test]
    fn vanilla_writes_nothing_and_every_preset_round_trips() {
        let s = PlayerStatsStyle::default();
        assert!(s.is_vanilla());
        assert_eq!(s.changed_count(), 0);
        assert!(s.compile().unwrap().is_empty());
        for preset in StatsPreset::ALL {
            let style = preset.style();
            assert_eq!(style.preset(), Some(preset));
            for (_, css) in style.compile().unwrap() {
                assert!(css::parse_rules(&css).is_ok(), "{css}");
            }
        }
    }

    #[test]
    fn a_part_moves_sizes_fades_and_hides_on_its_own_panel() {
        let mut s = PlayerStatsStyle::default();
        s.set_part(
            StatsPart::Souls,
            PartEdit {
                offset_x: 40,
                offset_y: -12,
                scale_pct: 125,
                opacity_pct: Some(70),
                ..PartEdit::default()
            },
        );
        s.set_part(
            StatsPart::Quickbuy,
            PartEdit {
                hidden: true,
                opacity_pct: Some(20),
                ..PartEdit::default()
            },
        );
        let out = s.compile().unwrap();
        assert_eq!(
            sheet(&out, GOLD_STYLE),
            "#gold_and_ap_container #hudGoldContainer{transform:translateX(40px) translateY(-12px);\
             pre-transform-scale2d:1.25;opacity:0.7;}"
        );
        assert_eq!(
            sheet(&out, QUICKBUY_STYLE),
            "#StatsAndModsContainer #HudMini{visibility:collapse;}",
            "the game's own 20% writes no opacity"
        );
        assert_eq!(s.changed_count(), 2);
        assert_eq!(s.part_changed_count(StatsPart::Souls), 1);
    }

    #[test]
    fn the_item_grid_scales_from_its_corner_and_gaps_keep_both_rows() {
        let mut s = PlayerStatsStyle::default();
        s.set_part(
            StatsPart::Items,
            PartEdit {
                scale_pct: 90,
                ..PartEdit::default()
            },
        );
        s.tile_gap_px = 5;
        s.hide_flex = true;
        let out = s.compile().unwrap();
        let hud = sheet(&out, HUD_STYLE);
        assert!(
            hud.starts_with("#StatsAndModsContainer .ModsContainer{ui-scale:108%;}"),
            "{hud}"
        );
        assert!(
            hud.contains("#StatsAndModsContainer .ModsContainer{height:110px;}"),
            "{hud}"
        );
        assert!(
            hud.ends_with("#ModsContainer #ModPurchasedPanelUniversalLocked{visibility:collapse;}")
        );
        assert_eq!(
            sheet(&out, MODS_PANEL_STYLE),
            "#ModsContainer CitadelModsPurchasedPanel{height:110px;}\
             #ModsContainer #ModPurchasedPanelUniversalLocked{width:112px;}"
        );
        assert_eq!(
            sheet(&out, MOD_ICON_STYLE),
            "#ModsContainer CitadelModIcon{padding:5px;}"
        );
    }

    #[test]
    fn details_reach_numbers_level_souls_and_tiles() {
        let red = Color([0xFF, 0x40, 0x40, 255]);
        let s = PlayerStatsStyle {
            number_px: 30,
            straight_numbers: true,
            weapon_color: Some(red),
            hide_deltas: true,
            level_px: 32,
            souls_color: Some(red),
            hide_souls_label: true,
            cooldown_pct: 50,
            hide_tiers: true,
            ..PlayerStatsStyle::default()
        };
        let out = s.compile().unwrap();
        let stats = sheet(&out, ACTIVE_STATS_STYLE);
        assert!(stats.contains(
            "#hudActivePlayerStats #HudStatBlock #CoreStats .statNumber{font-size:30px;transform:rotateZ(0deg);}"
        ), "{stats}");
        assert!(stats.contains("#CoreStats #Weapon .core_bg{wash-color:#FF4040;}"));
        assert!(stats.contains(".has_delta .statNumberDelta{visibility:collapse;}"));
        assert_eq!(
            sheet(&out, LEVEL_STYLE),
            "CitadelPlayerLevel #PlayerLevelNumber{font-size:32px;}"
        );
        let gold = sheet(&out, GOLD_STYLE);
        assert!(gold.contains("#hudCurGoldLabel{color:#FF4040;}"));
        assert!(gold.contains("#hudCurGoldIcon{wash-color:#FF4040;}"));
        assert!(gold.contains("#hudGoldLabel{visibility:collapse;}"));
        let tiles = sheet(&out, MOD_ICON_STYLE);
        assert!(tiles.contains("#ModsContainer .isWeapon .tier_bg{wash-color:#FF4040;}"));
        assert!(tiles.contains("#ModsContainer #CooldownMask{background-color:#10130D80;}"));
        assert!(tiles.contains("#ModsContainer #TierContainer{visibility:collapse;}"));
        for (_, css) in &out {
            assert!(css::parse_rules(css).is_ok(), "{css}");
        }
        assert_eq!(s.changed_count(), 9);
        let mut back = s.clone();
        back.reset_part(StatsPart::Numbers);
        assert_eq!(back.part_changed_count(StatsPart::Numbers), 0);
        assert_eq!(back.changed_count(), 5);
    }

    #[test]
    fn out_of_range_values_are_refused() {
        let mut s = PlayerStatsStyle::default();
        s.set_part(
            StatsPart::Level,
            PartEdit {
                scale_pct: 300,
                ..PartEdit::default()
            },
        );
        assert_eq!(
            s.compile(),
            Err(PlayerStatsError::Scale(StatsPart::Level, 300))
        );
        let s = PlayerStatsStyle {
            souls_px: 80,
            ..PlayerStatsStyle::default()
        };
        assert!(matches!(
            s.compile(),
            Err(PlayerStatsError::Px("souls size", 80, _))
        ));
    }

    #[test]
    fn toml_keeps_only_what_changed() {
        let mut s = StatsPreset::Clean.style();
        s.set_part(
            StatsPart::Numbers,
            PartEdit {
                offset_x: 12,
                ..PartEdit::default()
            },
        );
        let text = toml::to_string(&s).unwrap();
        assert!(text.contains("[parts.numbers]"), "{text}");
        assert!(!text.contains("scale_pct"), "{text}");
        assert_eq!(toml::from_str::<PlayerStatsStyle>(&text).unwrap(), s);
        let partial: PlayerStatsStyle = toml::from_str("hide_flex = true\n").unwrap();
        assert!(partial.hide_flex && partial.number_px == NUMBER_PX);
    }
}
