//! The screen-space particle disabler, made configurable. Upstream (Laund's
//! `pak02_dir.vpk`) replaces 108 particle systems with one empty stub. We take the
//! stub from the upstream file and ship it at only the paths the player hides,
//! grouped by what the player sees on screen. The groups also cover the four effects
//! Laund added later in the Screenspace pack of Clutter Be Gone (`clutter/screenspace.txt`).

use std::collections::{BTreeMap, BTreeSet};

use super::AddonError;
use crate::hud::vpk::{self, VpkDir};

/// Any upstream entry carries the stub; this one is checked.
pub const STUB_PATH: &str = "particles/generic/player_low_health_screen.vpcf_c";
pub const STUB_CRC: u32 = 0xf3db_7131;
/// The empty texture the stub references; always shipped with it.
pub const DEBUG_TEXTURE: &str = "materials/debug/debugempty_color_tga_fd967415.vtex_c";

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
        "hideout",
        "Hideout: finding match",
        "ui",
        ["ui_hideout_findingmatch_screen"]
    ),
    group!(
        "snowball",
        "Snowball hit (Christmas event)",
        "event/christmas",
        ["snowball_tgt_screen"]
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
        "ratking",
        "Nibble debuff (ratking)",
        "abilities/ratking",
        [
            "ratking_nibble_debuff_screen",
            "ratking_nibble_debuff_screen_border",
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

/// The two upstream files every build needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stub {
    pub particle: Vec<u8>,
    pub texture: Vec<u8>,
}

pub fn stub_from_upstream(upstream: &VpkDir) -> Result<Stub, AddonError> {
    if upstream.entries.get(STUB_PATH).map(|e| e.crc) != Some(STUB_CRC) {
        return Err(AddonError::BadStub);
    }
    Ok(Stub {
        particle: upstream.read(STUB_PATH)?,
        texture: upstream.read(DEBUG_TEXTURE)?,
    })
}

/// Our pak: the stub at each hidden path plus the texture it references. No paths
/// means no pak at all (the caller plans a removal).
pub fn build(stub: &Stub, paths: &[&str]) -> Option<Vec<u8>> {
    if paths.is_empty() {
        return None;
    }
    let mut files: BTreeMap<String, Vec<u8>> = paths
        .iter()
        .map(|p| (p.to_string(), stub.particle.clone()))
        .collect();
    files.insert(DEBUG_TEXTURE.to_string(), stub.texture.clone());
    Some(vpk::write(&files))
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::hud::crc32::crc32;

    pub fn upstream_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../research/configs/OptimizationLock/Various Addons Relating to Performance/Screenspace Particle Disabler/pak02_dir.vpk",
        )
    }

    fn upstream() -> VpkDir {
        VpkDir::open(&upstream_path()).unwrap()
    }

    #[test]
    fn groups_cover_both_upstream_particle_lists_exactly_once() {
        let upstream = upstream();
        let mut ours: Vec<&str> = hidden_paths(&BTreeSet::new());
        ours.sort_unstable();
        let mut theirs: Vec<&str> = upstream
            .entries
            .keys()
            .map(String::as_str)
            .filter(|p| *p != DEBUG_TEXTURE)
            .collect();
        theirs.extend(include_str!("clutter/screenspace.txt").lines());
        theirs.sort_unstable();
        theirs.dedup();
        assert_eq!(ours.len(), 112);
        assert_eq!(upstream.entries.len(), 109);
        assert_eq!(ours, theirs);
        let mut ids: Vec<&str> = GROUPS.iter().map(|g| g.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), GROUPS.len());
    }

    #[test]
    fn stub_is_the_same_bytes_at_every_upstream_path() {
        let upstream = upstream();
        let stub = stub_from_upstream(&upstream).unwrap();
        assert_eq!(crc32(&stub.particle), STUB_CRC);
        for path in hidden_paths(&BTreeSet::new()) {
            if !upstream.contains(path) {
                continue;
            }
            assert_eq!(upstream.read(path).unwrap(), stub.particle, "{path}");
        }
        assert_eq!(stub.texture.len(), 7556);
    }

    #[test]
    fn build_holds_exactly_the_selected_paths_plus_texture() {
        let stub = stub_from_upstream(&upstream()).unwrap();
        let keep: BTreeSet<String> = GROUPS
            .iter()
            .map(|g| g.id.to_string())
            .filter(|id| id != "low_health" && id != "shiv")
            .collect();
        let paths = hidden_paths(&keep);
        assert_eq!(paths.len(), 4 + 5);
        let bytes = build(&stub, &paths).unwrap();
        assert_eq!(bytes, build(&stub, &paths).unwrap());
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("pak71_dir.vpk");
        std::fs::write(&p, &bytes).unwrap();
        let ours = VpkDir::open(&p).unwrap();
        assert_eq!(ours.entries.len(), paths.len() + 1);
        for path in &paths {
            assert_eq!(ours.read(path).unwrap(), stub.particle);
        }
        assert_eq!(ours.read(DEBUG_TEXTURE).unwrap(), stub.texture);
        assert!(!ours.contains("particles/abilities/lash/lash_final_strike_screen.vpcf_c"));
        let all: BTreeSet<String> = GROUPS.iter().map(|g| g.id.to_string()).collect();
        assert_eq!(build(&stub, &hidden_paths(&all)), None);
    }

    #[test]
    fn a_file_without_the_stub_is_rejected() {
        let files = BTreeMap::from([(STUB_PATH.to_string(), b"not a stub".to_vec())]);
        let fake = VpkDir::parse(Path::new("x_dir.vpk"), &vpk::write(&files)).unwrap();
        assert!(matches!(
            stub_from_upstream(&fake),
            Err(AddonError::BadStub)
        ));
    }
}
