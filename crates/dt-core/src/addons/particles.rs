//! The screen-space particle disabler's path table. Upstream (Laund's `pak02_dir.vpk`)
//! replaces 108 particle systems with the game's empty particle; here they are grouped
//! by what the player sees on screen, so `native_particles` ships only the hidden ones.

use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParticleGroup {
    pub id: &'static str,
    pub label: &'static str,
    pub paths: &'static [&'static str],
}

macro_rules! group {
    ($id:literal, $label:literal, $dir:literal, [$($name:literal),+ $(,)?]) => {
        ParticleGroup {
            id: $id,
            label: $label,
            paths: &[$(concat!("particles/", $dir, "/", $name, ".vpcf_c")),+],
        }
    };
}

/// Hero labels use the game's hero name where the codename is settled; the rest show the
/// effect with the codename in brackets until verified in game (NOTES in docs/testing-windows.md).
pub static GROUPS: &[ParticleGroup] = &[
    group!(
        "low_health",
        "Low health vignette",
        "generic",
        [
            "player_low_health_screen",
            "player_low_health_screen_lv",
            "player_low_health_screen_pattern",
            "player_low_health_screen_lv_pattern",
        ]
    ),
    group!(
        "damage",
        "Damage and death flashes",
        "generic",
        [
            "player_damage_screen",
            "player_ground_damage_screen",
            "player_ground_damage_screen_glow",
            "player_death_screen",
        ]
    ),
    group!(
        "steam",
        "Vent steam and smoke",
        "environment",
        [
            "vent_steam_screen_effect",
            "vent_steam_screen_effect_top",
            "vent_steam_screen_effect_bot",
            "vent_steam_screen_effect_left",
            "vent_steam_screen_effect_right",
        ]
    ),
    group!(
        "obscured",
        "Obscured by steam",
        "modifiers",
        ["obscured_by_steam_screen"]
    ),
    group!(
        "items",
        "Item debuffs (armor piercing, tech resist, silence)",
        "upgrades",
        [
            "armor_piercing_debuff_screen",
            "armor_piercing_debuff_screen_border",
            "tech_resist_debuff_screen",
            "aoe_mute_screen",
        ]
    ),
    group!(
        "archer",
        "Grey Talon: Guided Owl",
        "abilities/archer",
        [
            "archer_guided_arrow_screen",
            "archer_guided_arrow_channel_screen",
            "archer_guided_arrow_screen_pattern_top",
            "archer_guided_arrow_screen_pattern_bottom",
            "archer_guided_arrow_screen_pattern_left",
            "archer_guided_arrow_screen_pattern_right",
        ]
    ),
    group!(
        "bookworm",
        "Dragonfire (bookworm)",
        "abilities/bookworm",
        [
            "bookworm_dragonfire_screen_effect",
            "bookworm_dragonfire_screen_effect_top",
            "bookworm_dragonfire_screen_effect_bot",
            "bookworm_dragonfire_screen_effect_left",
            "bookworm_dragonfire_screen_effect_right",
        ]
    ),
    group!(
        "butcher",
        "Bullet shield (butcher)",
        "abilities/butcher",
        [
            "butcher_bullet_shield_screen",
            "butcher_bullet_shield_screen_warp",
            "butcher_bullet_shield_screen_glow",
        ]
    ),
    group!(
        "chrono",
        "Paradox: Pulse Grenade sphere",
        "abilities/chrono",
        ["chrono_sphere_screen_debuff_border",]
    ),
    group!(
        "doorman",
        "Doorman: bell debuff",
        "abilities/doorman",
        ["doorman_bell_debuff_screen"]
    ),
    group!(
        "drifter",
        "Drifter: victim vision",
        "abilities/drifter",
        [
            "drifter_victim_vision_screen",
            "drifter_victim_vision_screen_miasma_top",
            "drifter_victim_vision_screen_miasma_bot",
            "drifter_victim_vision_screen_miasma_left",
            "drifter_victim_vision_screen_miasma_right",
        ]
    ),
    group!(
        "druid",
        "Invisibility (druid)",
        "abilities/druid",
        [
            "druid_invis_screen_effect",
            "druid_invis_screen_effect_top",
            "druid_invis_screen_effect_bot",
            "druid_invis_screen_effect_left",
            "druid_invis_screen_effect_right",
        ]
    ),
    group!(
        "familiar",
        "Tagalong and Naptime (familiar)",
        "abilities/familiar",
        [
            "familiar_tagalong_screen",
            "familiar_tagalong_screen_top",
            "familiar_tagalong_screen_bot",
            "familiar_tagalong_host_screen",
            "familiar_tagalong_host_screen_bot",
            "familiar_naptime_debuff_screen",
            "familiar_naptime_buildup_debuff_screen",
        ]
    ),
    group!(
        "fathom",
        "Fathom: immobilize",
        "abilities/fathom",
        [
            "fathom_immobilize_screen_effect",
            "fathom_immobilize_screen_effect_top_beam",
            "fathom_immobilize_screen_effect_bot_beam",
            "fathom_immobilize_screen_effect_left_beam",
            "fathom_immobilize_screen_effect_right_beam",
        ]
    ),
    group!(
        "fencer",
        "Ultimate screen effect (fencer)",
        "abilities/fencer",
        [
            "fencer_ult_screen_effect",
            "fencer_ult_screen_effect_center",
            "fencer_ult_screen_effect_top",
            "fencer_ult_screen_effect_left",
            "fencer_ult_screen_effect_right",
        ]
    ),
    group!(
        "frank",
        "Pain aura (frank)",
        "abilities/frank",
        [
            "frank_painaura_aura_screen",
            "frank_painaura_aura_screen_bot",
        ]
    ),
    group!(
        "ghost",
        "Lady Geist: Essence Bomb and Soul Exchange",
        "abilities/ghost",
        [
            "ghost_blood_bomb_debuff_screen",
            "ghost_blood_bomb_debuff_screen_glow",
            "ghost_blood_exchange_tgt_screen",
        ]
    ),
    group!(
        "gigawatt",
        "Seven: Lightning Ball",
        "abilities/gigawatt",
        ["gigawatt_lightning_ball_tgt_screen",]
    ),
    group!(
        "hijack",
        "Weapon jammer (hijack)",
        "abilities/hijack",
        [
            "hijack_weapon_jammer_screen",
            "hijack_weapon_jammer_screen_border",
        ]
    ),
    group!(
        "hornet",
        "Vindicta: Stake tether",
        "abilities/hornet",
        [
            "hornet_tether_screen_effect",
            "hornet_tether_screen_effect_miasma_top",
            "hornet_tether_screen_effect_miasma_bot",
            "hornet_tether_screen_effect_miasma_left",
            "hornet_tether_screen_effect_miasma_right",
        ]
    ),
    group!(
        "inferno",
        "Infernus: burn (Catalyst, Flame Dash)",
        "abilities/inferno",
        [
            "inferno_incendiary_debuff_screen",
            "inferno_incendiary_debuff_screen_glow",
            "inferno_flamedash_debuff_screen",
            "inferno_flamedash_debuff_screen_glow",
        ]
    ),
    group!(
        "lash",
        "Lash: Death Slam",
        "abilities/lash",
        ["lash_final_strike_screen"]
    ),
    group!(
        "mirage",
        "Mirage: Tornado, Djinn's Mark, Traveler",
        "abilities/mirage",
        [
            "mirage_tornado_debuff_screen",
            "mirage_tornado_debuff_screen_glow",
            "mirage_djinns_reach_debuff_screen",
            "mirage_traveler_screen",
        ]
    ),
    group!(
        "pocket",
        "Pocket: Affliction",
        "abilities/pocket",
        [
            "pocket_affliction_dot_screen_effect",
            "pocket_affliction_dot_screen_top",
            "pocket_affliction_dot_screen_bot",
            "pocket_affliction_dot_screen_left",
            "pocket_affliction_dot_screen_right",
        ]
    ),
    group!(
        "priest",
        "Flashbang (priest)",
        "abilities/priest",
        ["priest_flashbang_screen"]
    ),
    group!(
        "punkgoat",
        "Blasted (punkgoat)",
        "abilities/punkgoat",
        [
            "punkgoat_blasted_screen_effect",
            "punkgoat_blasted_screen_effect_top",
            "punkgoat_blasted_screen_effect_bottom",
            "punkgoat_blasted_screen_effect_left",
            "punkgoat_blasted_screen_effect_right",
        ]
    ),
    group!(
        "shiv",
        "Shiv: Bloodletting dash and Killing Blow",
        "abilities/shiv",
        [
            "shiv_dash_screen",
            "shiv_transform_debuff_screen",
            "shiv_transform_debuff_screen_top",
            "shiv_transform_debuff_screen_left",
            "shiv_transform_debuff_screen_right",
        ]
    ),
    group!(
        "synth",
        "Pulse channel (synth)",
        "abilities/synth",
        ["synth_pulse_channel_screen"]
    ),
    group!(
        "viper",
        "Vyper: venom",
        "abilities/viper",
        [
            "viper_venom_screen_effect",
            "viper_venom_screen_effect_top",
            "viper_venom_screen_effect_bottom",
            "viper_venom_screen_effect_left",
            "viper_venom_screen_effect_right",
        ]
    ),
    group!(
        "wrecker",
        "Wrecker: teleport",
        "abilities/wrecker",
        [
            "wrecker_teleport_player_screen",
            "wrecker_teleport_player_screen_warp",
            "wrecker_teleport_player_screen_glow",
        ]
    ),
];

pub fn group(id: &str) -> Option<&'static ParticleGroup> {
    GROUPS.iter().find(|g| g.id == id)
}

/// Particle paths hidden when every group not in `keep` is disabled, in table order.
pub fn hidden_paths(keep: &BTreeSet<String>) -> Vec<&'static str> {
    GROUPS
        .iter()
        .filter(|g| !keep.contains(g.id))
        .flat_map(|g| g.paths.iter().copied())
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::hud::vpk::VpkDir;

    pub fn upstream_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../research/configs/OptimizationLock/Various Addons Relating to Performance/Screenspace Particle Disabler/pak02_dir.vpk",
        )
    }

    fn upstream() -> VpkDir {
        VpkDir::open(&upstream_path()).unwrap()
    }

    #[test]
    fn groups_cover_the_upstream_particle_list_exactly_once() {
        let upstream = upstream();
        let mut ours: Vec<&str> = hidden_paths(&BTreeSet::new());
        ours.sort_unstable();
        let mut theirs: Vec<&str> = upstream
            .entries
            .keys()
            .map(String::as_str)
            .filter(|p| p.ends_with(".vpcf_c"))
            .collect();
        theirs.sort_unstable();
        assert_eq!(ours.len(), 108);
        assert_eq!(upstream.entries.len(), 109);
        assert_eq!(ours, theirs);
        let mut ids: Vec<&str> = GROUPS.iter().map(|g| g.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), GROUPS.len());
    }
}
