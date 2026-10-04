//! Experimental: minimap icon colours. Not yet tested in game.
//!
//! One table row per recolourable icon. Rules go into `hud_minimap.vcss_c`, the
//! stylesheet `hud_minimap.xml` includes for every minimap icon snippet. Each selector
//! starts with `#hud_minimap` so it outranks its vanilla rule regardless of order.
//! Selectors use team and type classes only, and only colour properties are emitted,
//! so a recolour shows nothing the vanilla minimap hides
//! (research/hud/minimap-icon-colors.md section 5).
//!
//! Troopers and ziplines are drawn into the minimap canvas by game code and cannot be
//! recoloured per team from CSS, so they have no rows.

use std::fmt;
use std::str::FromStr;

pub const MINIMAP_STYLE: &str = "panorama/styles/hud_minimap.vcss_c";

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum IconId {
    EnemyHero,
    AllyHero,
    LocalHero,
    EnemyHeroArrow,
    AllyHeroArrow,
    EnemyObjective,
    AllyObjective,
    EnemyUrnReturn,
    AllyUrnReturn,
    UrnSpawn,
    CarriedUrn,
    SmallCamp,
    MediumCamp,
    LargeCamp,
    Vault,
    MidBoss,
    WeaponPowerup,
    SoulsPowerup,
    HealthPowerup,
    SpiritPowerup,
    MovementPowerup,
    PowerupSpawn,
    UnsecuredSouls,
    RejuvCrystal,
    Shop,
    Broker,
    Teleporter,
    Stairs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IconSpec {
    pub id: IconId,
    pub label: &'static str,
    pub selector: &'static str,
    /// `background-color` for flat fills, `wash-color` for textured icons.
    pub property: &'static str,
    /// Vanilla colour of the rule this overrides; `None` when the texture's own colour shows.
    pub vanilla: Option<&'static str>,
    pub notes: &'static str,
}

pub static ICONS: &[IconSpec] = &[
    IconSpec {
        id: IconId::EnemyHero,
        label: "Enemy heroes",
        selector: "#hud_minimap .map_button.player.enemy #BackgroundImage",
        property: "background-color",
        vanilla: Some("#FF410D"),
        notes: "Disc behind the hero portrait. Vanilla: .enemy.player #BackgroundImage, colorEnemy. The in-game Enemy UI Color setting (citadel_custom_ui_colors) may override this.",
    },
    IconSpec {
        id: IconId::AllyHero,
        label: "Allied heroes",
        selector: "#hud_minimap .map_button.player.friend:not(.localplayer) #BackgroundImage",
        property: "background-color",
        vanilla: Some("#FFEFD7"),
        notes: "Disc behind the hero portrait, excluding yourself. Vanilla: .friend.player #BackgroundImage, offWhite.",
    },
    IconSpec {
        id: IconId::LocalHero,
        label: "Your hero",
        selector: "#hud_minimap .map_button.player.localplayer #BackgroundImage",
        property: "background-color",
        vanilla: Some("#FFEFD7"),
        notes: "Same disc on your own icon. Your 110% self marker may cover most of it.",
    },
    IconSpec {
        id: IconId::EnemyHeroArrow,
        label: "Enemy edge arrows",
        selector: "#hud_minimap .map_button.player.enemy #ArrowImage",
        property: "wash-color",
        vanilla: None,
        notes: "Arrow shown when an enemy icon is clamped to the minimap edge. Tints player_arrow_enemy_psd.",
    },
    IconSpec {
        id: IconId::AllyHeroArrow,
        label: "Allied edge arrows",
        selector: "#hud_minimap .map_button.player.friend #ArrowImage",
        property: "wash-color",
        vanilla: None,
        notes: "Arrow shown when an allied icon is clamped to the minimap edge. Tints player_arrow_friend_psd.",
    },
    IconSpec {
        id: IconId::EnemyObjective,
        label: "Enemy objectives",
        selector: "#hud_minimap .map_button.enemy.boss .boss_image",
        property: "wash-color",
        vanilla: Some("#DC4C2F"),
        notes: "Guardians, walkers, barracks and patron. Vanilla: ColorEnemyObjective.",
    },
    IconSpec {
        id: IconId::AllyObjective,
        label: "Allied objectives",
        selector: "#hud_minimap .map_button.friend.boss .boss_image",
        property: "wash-color",
        vanilla: None,
        notes: "Replaces the per-lane colours (yellow, green, blue, purple) with one colour, so the lane cue is lost.",
    },
    IconSpec {
        id: IconId::EnemyUrnReturn,
        label: "Enemy urn drop-off",
        selector: "#hud_minimap .map_button.idol_return_enemy .idolReturnTarget",
        property: "wash-color",
        vanilla: Some("#DC4C2F"),
        notes: "Pulsing target where the enemy team returns the soul urn.",
    },
    IconSpec {
        id: IconId::AllyUrnReturn,
        label: "Allied urn drop-off",
        selector: "#hud_minimap .map_button.idol_return_friendly .idolReturnTarget",
        property: "wash-color",
        vanilla: Some("#34DDFF"),
        notes: "Pulsing target where your team returns the soul urn.",
    },
    IconSpec {
        id: IconId::UrnSpawn,
        label: "Soul urn spawn",
        selector: "#hud_minimap .map_button.idol_spawn",
        property: "wash-color",
        vanilla: None,
        notes: "Urn on the ground. Vanilla adds a teal glow (img-shadow) that the tint does not change.",
    },
    IconSpec {
        id: IconId::CarriedUrn,
        label: "Carried urn",
        selector: "#hud_minimap .held_idol_image",
        property: "wash-color",
        vanilla: None,
        notes: "Urn drawn on the icon of the hero carrying it, either team.",
    },
    IconSpec {
        id: IconId::SmallCamp,
        label: "Small camps",
        selector: "#hud_minimap .map_button.neutral_weak",
        property: "wash-color",
        vanilla: None,
        notes: "Texture neutral_small_psd. A newer camp icon set exists behind citadel_show_old_neutral_camp_icons; this rule may not reach it.",
    },
    IconSpec {
        id: IconId::MediumCamp,
        label: "Medium camps",
        selector: "#hud_minimap .map_button.neutral_medium",
        property: "wash-color",
        vanilla: None,
        notes: "Texture neutral_medium_psd.",
    },
    IconSpec {
        id: IconId::LargeCamp,
        label: "Large camps",
        selector: "#hud_minimap .map_button.neutral_large",
        property: "wash-color",
        vanilla: None,
        notes: "Texture neutral_large_psd.",
    },
    IconSpec {
        id: IconId::Vault,
        label: "Vaults",
        selector: "#hud_minimap .map_button.neutral_vault",
        property: "wash-color",
        vanilla: None,
        notes: "Sinner's Sacrifice machines. Texture neutral_vault_psd.",
    },
    IconSpec {
        id: IconId::MidBoss,
        label: "Mid boss",
        selector: "#hud_minimap .map_button.mid_boss .boss_image,#hud_minimap .map_button.mid_boss #Spawned",
        property: "wash-color",
        vanilla: None,
        notes: "Vanilla greys the marker (saturation 0, brightness 0.3) until it spawns; both the grey and the spawned marker get the tint.",
    },
    IconSpec {
        id: IconId::WeaponPowerup,
        label: "Weapon powerup",
        selector: "#hud_minimap .map_button.powerup_gun",
        property: "wash-color",
        vanilla: Some("#FFCC80"),
        notes: "Golden statue buff icons.",
    },
    IconSpec {
        id: IconId::SoulsPowerup,
        label: "Souls powerup",
        selector: "#hud_minimap .map_button.powerup_souls",
        property: "wash-color",
        vanilla: Some("#99FFD6"),
        notes: "Vanilla: shardColor.",
    },
    IconSpec {
        id: IconId::HealthPowerup,
        label: "Health powerup",
        selector: "#hud_minimap .map_button.powerup_survival",
        property: "wash-color",
        vanilla: Some("#D7FF9A"),
        notes: "",
    },
    IconSpec {
        id: IconId::SpiritPowerup,
        label: "Spirit powerup",
        selector: "#hud_minimap .map_button.powerup_casting",
        property: "wash-color",
        vanilla: Some("#E1A6FF"),
        notes: "",
    },
    IconSpec {
        id: IconId::MovementPowerup,
        label: "Movement powerup",
        selector: "#hud_minimap .map_button.powerup_movement",
        property: "wash-color",
        vanilla: Some("#99FFDD"),
        notes: "",
    },
    IconSpec {
        id: IconId::PowerupSpawn,
        label: "Gold crate spawn",
        selector: "#hud_minimap .map_button.powerup_spawn",
        property: "wash-color",
        vanilla: None,
        notes: "Texture gold_crate_marker_psd.",
    },
    IconSpec {
        id: IconId::UnsecuredSouls,
        label: "Unsecured souls",
        selector: "#hud_minimap .map_button.gold_spawn",
        property: "wash-color",
        vanilla: Some("#82FFF9"),
        notes: "",
    },
    IconSpec {
        id: IconId::RejuvCrystal,
        label: "Rejuv crystal",
        selector: "#hud_minimap .map_button.rejuv_crystal",
        property: "wash-color",
        vanilla: Some("#82FFF9"),
        notes: "",
    },
    IconSpec {
        id: IconId::Shop,
        label: "Shops",
        selector: "#hud_minimap .map_button.tier1_shop",
        property: "wash-color",
        vanilla: None,
        notes: "Texture minimap_shop_psd.",
    },
    IconSpec {
        id: IconId::Broker,
        label: "Broker",
        selector: "#hud_minimap .map_button.corrupted_item_shop",
        property: "wash-color",
        vanilla: None,
        notes: "Vanilla desaturates it until you can buy; the tint shows through only as far as saturation allows.",
    },
    IconSpec {
        id: IconId::Teleporter,
        label: "Teleporters",
        selector: "#hud_minimap .map_button.teleporter_far,#hud_minimap .map_button.teleporter_nearby,#hud_minimap .map_button.teleporter_active",
        property: "wash-color",
        vanilla: None,
        notes: "All three distance states share the colour, so nothing new is shown.",
    },
    IconSpec {
        id: IconId::Stairs,
        label: "Stairs and tunnel exits",
        selector: "#hud_minimap .map_button.mid_stairs,#hud_minimap .map_button.shop_tunnel_exit",
        property: "wash-color",
        vanilla: None,
        notes: "",
    },
];

pub fn spec(id: IconId) -> &'static IconSpec {
    ICONS
        .iter()
        .find(|s| s.id == id)
        .expect("every IconId has a row in ICONS")
}

/// A Panorama colour, written `#RRGGBB` or `#RRGGBBAA`. For `wash-color` the alpha
/// sets how strongly the colour blends over the texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Color(pub [u8; 4]);

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("colour {0:?} is not #RRGGBB or #RRGGBBAA")]
pub struct ColorError(String);

impl FromStr for Color {
    type Err = ColorError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let err = || ColorError(text.to_string());
        let hex = text.strip_prefix('#').ok_or_else(err)?;
        if !(hex.len() == 6 || hex.len() == 8) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(err());
        }
        let mut rgba = [0, 0, 0, 255];
        for (i, channel) in rgba.iter_mut().enumerate().take(hex.len() / 2) {
            *channel = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|_| err())?;
        }
        Ok(Color(rgba))
    }
}

impl TryFrom<String> for Color {
    type Error = ColorError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<Color> for String {
    fn from(color: Color) -> String {
        color.to_string()
    }
}

/// Uppercase hex; alpha only when not opaque.
impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [r, g, b, a] = self.0;
        write!(f, "#{r:02X}{g:02X}{b:02X}")?;
        if a != 255 {
            write!(f, "{a:02X}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_row_per_icon_in_id_order() {
        let ids: Vec<IconId> = ICONS.iter().map(|s| s.id).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(ids, sorted, "rows sorted by IconId, no duplicates");
        assert_eq!(ids.len(), 28);
        for s in ICONS {
            assert!(
                s.selector
                    .split(',')
                    .all(|sel| sel.starts_with("#hud_minimap ")),
                "{:?} outranks vanilla",
                s.id
            );
            assert!(
                matches!(s.property, "background-color" | "wash-color"),
                "{:?} colour only",
                s.id
            );
            if let Some(v) = s.vanilla {
                v.parse::<Color>().expect("vanilla colour parses");
            }
        }
    }

    #[test]
    fn selectors_use_no_state_classes() {
        let state = [
            "PlayerDead",
            "PlayerIsHoldingIdol",
            "lowHealth",
            "hide_button",
            "spectate",
            "arrowUp",
            "arrowDown",
            "active",
            "midboss_spawned",
            "canPurchaseItem",
            "koth_warning",
            "is_visible",
            "teleporter_off",
        ];
        for s in ICONS {
            for token in s
                .selector
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
            {
                assert!(
                    !state.contains(&token) && !token.starts_with("modifier_state"),
                    "{:?} keys on state class {token}",
                    s.id
                );
            }
        }
    }

    #[test]
    fn color_parse_and_display() {
        assert_eq!("#00d5ff".parse(), Ok(Color([0, 0xD5, 0xFF, 255])));
        assert_eq!("#00D5FF80".parse(), Ok(Color([0, 0xD5, 0xFF, 0x80])));
        assert_eq!(Color([0, 0xD5, 0xFF, 255]).to_string(), "#00D5FF");
        assert_eq!(Color([1, 2, 3, 4]).to_string(), "#01020304");
        for bad in [
            "00d5ff", "#00d5f", "#00d5ff8", "#00d5fg", "#+1d5ff", "red", "",
        ] {
            assert_eq!(
                bad.parse::<Color>(),
                Err(ColorError(bad.to_string())),
                "{bad}"
            );
        }
    }
}
