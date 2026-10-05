//! The game images DeadTune's HUD previews draw, by preview element. Paths only: the
//! pictures are read at runtime from the player's pak01, an image export or a snapshot
//! (`snapshot::images::ImageSource`), never shipped.
//!
//! Which image each element uses comes from the game's layouts and stylesheets of build
//! 25712201 and from the in-game reference screenshot; the evidence is in
//! `docs/plans/hud-previews.md`. Sizes are what shows on a 1080p screen: the CSS box where
//! the game draws the panel at its CSS size, the size measured in the screenshot where the
//! game scales the panel in code (every minimap marker).

use super::minimap_colors::IconId;

/// One game image a preview draws, and its size on a 1080p screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Art {
    pub path: &'static str,
    pub size: [u16; 2],
}

const fn art(path: &'static str, w: u16, h: u16) -> Art {
    Art { path, size: [w, h] }
}

/// `#hud_minimap .backgroundImage1`: the dark map silhouette, 360 px inside the frame. The
/// lanes are not in any image; the game paints them on its `#canvas`.
pub const MINIMAP_MAP: Art = art(
    "panorama/images/minimap/base/minimap_midtown_mid_psd.vtex_c",
    360,
    360,
);
/// `#minimap_frame` (hud.css): 400 px, the second of its two background images wins, over a
/// blurred view of the world (`world-blur`).
pub const MINIMAP_FRAME: Art = art("panorama/images/minimap/compass_frame_psd.vtex_c", 400, 400);

/// A minimap objective: its dark backdrop (`.boss_health_bg`) under its fill
/// (`.boss_health`), both `.boss_image` and so washed in the team or lane colour. The fill
/// is cut from the top as the objective loses health.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Objective {
    pub back: Art,
    pub fill: Art,
}

const fn objective(back: &'static str, fill: &'static str, side: u16) -> Objective {
    Objective {
        back: art(back, side, side),
        fill: art(fill, side, side),
    }
}

/// `.boss_icon_t1` (guardians and walkers): a solid diamond. 40 px in CSS, 26 px on screen.
pub const GUARDIAN: Objective = objective(
    "panorama/images/minimap/boss_health_psd.vtex_c",
    "panorama/images/minimap/boss_health_fill_psd.vtex_c",
    26,
);
/// `.boss_buildingzip` (the two shrines beside each patron): a diamond with a dark centre.
/// 32 px in CSS, 21 px on screen.
pub const SHRINE: Objective = objective(
    "panorama/images/minimap/boss_health_t2_psd.vtex_c",
    "panorama/images/minimap/boss_health_fill_t2_psd.vtex_c",
    21,
);
/// `.boss_icon_t3` (the patron): the arch over the base, rotated 180 degrees for the
/// friendly one. 80 px in CSS, 75 px on screen.
pub const PATRON: Objective = objective(
    "panorama/images/minimap/boss_health_final_psd.vtex_c",
    "panorama/images/minimap/boss_health_final_fill_psd.vtex_c",
    75,
);
/// `.boss_icon_t3 .boss_health_s2`: the patron's second stage, a diamond in the arch.
pub const PATRON_STAGE2: Objective = objective(
    "panorama/images/minimap/boss_health_final_stage2_psd.vtex_c",
    "panorama/images/minimap/boss_health_final_stage2_fill_psd.vtex_c",
    75,
);

/// `.client_cone_fov #MainImage`, the local hero's view cone, 65% of the hero button.
pub const VIEW_CONE: Art = art("panorama/images/minimap/player_cone_psd.vtex_c", 36, 36);

/// The minimap marker for `id`. Heroes and objectives draw more than one picture
/// ([`Hero::marker`], [`GUARDIAN`], [`SHRINE`], [`PATRON`]) and return `None`.
pub fn marker(id: IconId) -> Option<Art> {
    let m = |path, side| Some(art(path, side, side));
    match id {
        IconId::EnemyHero | IconId::AllyHero | IconId::EnemyObjective | IconId::AllyObjective => {
            None
        }
        IconId::LocalHero => m(
            "panorama/images/minimap/player_marker_self_spec_highlight_psd.vtex_c",
            30,
        ),
        IconId::EnemyHeroArrow => m("panorama/images/minimap/player_arrow_enemy_psd.vtex_c", 22),
        IconId::AllyHeroArrow => m("panorama/images/minimap/player_arrow_friend_psd.vtex_c", 22),
        IconId::EnemyUrnReturn => m(
            "panorama/images/minimap/soul_jar_marker_return_enemy_psd.vtex_c",
            30,
        ),
        IconId::AllyUrnReturn => m(
            "panorama/images/minimap/soul_jar_marker_return_psd.vtex_c",
            30,
        ),
        IconId::UrnSpawn | IconId::CarriedUrn => {
            m("panorama/images/minimap/soul_jar_marker_psd.vtex_c", 20)
        }
        IconId::SmallCamp => m("panorama/images/minimap/neutral_small_psd.vtex_c", 14),
        IconId::MediumCamp => m("panorama/images/minimap/neutral_medium_psd.vtex_c", 14),
        IconId::LargeCamp => m("panorama/images/minimap/neutral_large_psd.vtex_c", 14),
        IconId::Vault => m("panorama/images/minimap/neutral_vault_psd.vtex_c", 14),
        IconId::MidBoss => m(
            "panorama/images/minimap/super_neutral_marker_psd.vtex_c",
            42,
        ),
        IconId::WeaponPowerup => m("panorama/images/minimap/powerup_weapon.vsvg_c", 14),
        IconId::SoulsPowerup => m("panorama/images/minimap/powerup_souls_png.vtex_c", 14),
        IconId::HealthPowerup => m("panorama/images/minimap/powerup_health.vsvg_c", 14),
        IconId::SpiritPowerup => m("panorama/images/minimap/powerup_magic.vsvg_c", 14),
        IconId::MovementPowerup => m("panorama/images/minimap/powerup_movement.vsvg_c", 14),
        IconId::PowerupSpawn => m("panorama/images/minimap/gold_crate_marker_psd.vtex_c", 14),
        IconId::UnsecuredSouls => m("panorama/images/minimap/unsecured_souls.vsvg_c", 12),
        IconId::RejuvCrystal => m("panorama/images/minimap/rejuv_crystal_psd.vtex_c", 12),
        IconId::Shop => m("panorama/images/minimap/minimap_shop_psd.vtex_c", 16),
        IconId::Broker => m("panorama/images/minimap/minimap_icon_broker.vsvg_c", 12),
        IconId::Teleporter => m(
            "panorama/images/minimap/minimap_teleporter_go_right.vsvg_c",
            12,
        ),
        IconId::Stairs => m("panorama/images/minimap/icon_stairs_mid.vsvg_c", 10),
    }
}

/// A stand-in hero for the previews: one of twelve that have every image variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hero {
    /// `.player #MainImage` on the minimap: the `_mm` bust, 85% of the hero's disc, which is
    /// 25 px on screen (the local hero's 28 px, with the bust at 110%).
    pub marker: Art,
    /// The top bar badge's `#HeroImage`: the `_card` bust, the one picture whose 280x380
    /// frame the badge's mask (`hero_badges/hero_image_mask_psd`, straight top, round
    /// bottom) is cut for. 80% of the 88 px player panel
    /// wide, so 70x95, over a 70 px disc at its bottom.
    pub portrait: Art,
}

macro_rules! hero {
    ($name:literal) => {
        Hero {
            marker: art(
                concat!("panorama/images/heroes/", $name, "_mm_psd.vtex_c"),
                25,
                25,
            ),
            portrait: art(
                concat!("panorama/images/heroes/", $name, "_card_psd.vtex_c"),
                70,
                95,
            ),
        }
    };
}

pub const ALLIES: [Hero; 6] = [
    hero!("archer"),
    hero!("astro"),
    hero!("bebop"),
    hero!("bull"),
    hero!("chrono"),
    hero!("digger"),
];
pub const ENEMIES: [Hero; 6] = [
    hero!("drifter"),
    hero!("haze"),
    hero!("inferno"),
    hero!("kelvin"),
    hero!("lash"),
    hero!("warden"),
];

/// `.team1 .TeamIcon` / `.team2 .TeamIcon`, beside each team's souls.
pub const TEAM_ICONS: [Art; 2] = [
    art("panorama/images/hud/core/icon_team1_psd.vtex_c", 30, 26),
    art("panorama/images/hud/core/icon_team2_psd.vtex_c", 30, 26),
];
/// `#RejuvenatorCharges .RejuvCount_1 .rejuvIcon`.
pub const TEAM_REJUV: Art = art(
    "panorama/images/hud/top_bar/team_rejuv_1_png.vtex_c",
    70,
    40,
);
/// `.rejuvTimer .statusEffectImage`.
pub const REJUV_ICON: Art = art("panorama/images/hud/icons/rejuvenator.vsvg_c", 30, 30);
/// `#UltimateAbilityIconMini` while the ultimate charges, on a 22 px team-coloured disc.
pub const ULTIMATE_OFF: Art = art(
    "panorama/images/hud/top_bar/icon_ultimate_off.vsvg_c",
    22,
    22,
);
/// `.UltimateCooldownReady #UltimateAbilityIconMini`: the open eye.
pub const ULTIMATE: Art = art("panorama/images/hud/top_bar/icon_ultimate.vsvg_c", 22, 22);
/// `.DeathIcon`, washed `#FF5656`.
pub const SKULL: Art = art("panorama/images/hud/icons/skull.vsvg_c", 50, 50);
/// `#HealthBar_Border` beside each portrait, washed offBlack; the bar is 16x50, allies'
/// 10 px in from the panel's left edge, enemies' 4 px in from its right.
pub const PORTRAIT_HEALTH: Art = art(
    "panorama/images/hud/healthbar/healthbar_backer_vert_border.vsvg_c",
    16,
    50,
);

/// `.healthBacker`, washed vivacious green, behind the health number (90% of 100x65).
pub const HEALTH_BACKER: Art = art("panorama/images/hud/core/health_backer.vsvg_c", 90, 65);
/// `#health_bar .ProgressBarLeft`: paper texture, offWhite, cut to the bar's slanted ruler
/// shape by `healthbar_backer_mask`.
pub const HEALTH_FILL: Art = art(
    "panorama/images/hud/healthbar/healthbar_fill_texture_png.vtex_c",
    66,
    212,
);
/// `#health_bar_frame`, washed `#142304`.
pub const HEALTH_FRAME: Art = art(
    "panorama/images/hud/healthbar/healthbar_frame_with_regen.vsvg_c",
    68,
    220,
);
/// `.regen_container .regen_image`.
pub const REGEN: Art = art(
    "panorama/images/hud/healthbar/icon_regen_arrows.vsvg_c",
    7,
    8,
);

/// Every image a preview may draw, once each.
pub fn all() -> Vec<Art> {
    let mut out = vec![
        MINIMAP_MAP,
        MINIMAP_FRAME,
        VIEW_CONE,
        TEAM_REJUV,
        REJUV_ICON,
        ULTIMATE_OFF,
        ULTIMATE,
        SKULL,
        PORTRAIT_HEALTH,
        HEALTH_BACKER,
        HEALTH_FILL,
        HEALTH_FRAME,
        REGEN,
    ];
    out.extend(TEAM_ICONS);
    out.extend(
        [GUARDIAN, SHRINE, PATRON, PATRON_STAGE2]
            .iter()
            .flat_map(|o| [o.back, o.fill]),
    );
    out.extend(
        super::minimap_colors::ICONS
            .iter()
            .filter_map(|s| marker(s.id)),
    );
    out.extend(
        ALLIES
            .iter()
            .chain(&ENEMIES)
            .flat_map(|h| [h.marker, h.portrait]),
    );
    let mut seen = std::collections::BTreeSet::new();
    out.retain(|a| seen.insert(a.path));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mapped_path_is_a_replaceable_image() {
        let all = all();
        assert!(all.len() > 50, "{}", all.len());
        for a in &all {
            assert!(crate::hud::icons::target(a.path).is_ok(), "{}", a.path);
            assert!(a.size[0] > 0 && a.size[1] > 0, "{}", a.path);
        }
    }

    #[test]
    fn heroes_draw_the_minimap_bust_and_the_badge_card() {
        for h in ALLIES.iter().chain(&ENEMIES) {
            assert!(
                h.marker.path.ends_with("_mm_psd.vtex_c"),
                "{}",
                h.marker.path
            );
            assert!(
                h.portrait.path.ends_with("_card_psd.vtex_c"),
                "{}",
                h.portrait.path
            );
        }
    }

    fn missing_in(listed: &std::collections::BTreeSet<&str>) -> Vec<&'static str> {
        all()
            .iter()
            .map(|a| a.path)
            .filter(|p| !listed.contains(p))
            .collect()
    }

    /// `DEADTUNE_GAME_SNAPSHOT=<snapshot folder>` checks the table against that build's
    /// `pak01.tsv`; skipped without it.
    #[test]
    fn every_mapped_path_is_in_the_snapshot_pak() {
        let Some(dir) = std::env::var_os("DEADTUNE_GAME_SNAPSHOT") else {
            eprintln!("DEADTUNE_GAME_SNAPSHOT is not set; skipping the preview image paths");
            return;
        };
        let tsv = std::fs::read_to_string(std::path::Path::new(&dir).join("pak01.tsv")).unwrap();
        let listed = tsv.lines().filter_map(|l| l.split('\t').next()).collect();
        let missing = missing_in(&listed);
        assert!(missing.is_empty(), "not in pak01: {missing:#?}");
    }

    /// `DEADTUNE_PREVIEW_IMAGES=<"Save all images" folder>` checks the table against that
    /// export's `manifest.json`; skipped without it.
    #[test]
    fn every_mapped_path_is_in_the_image_export() {
        let Some(dir) = std::env::var_os("DEADTUNE_PREVIEW_IMAGES") else {
            eprintln!("DEADTUNE_PREVIEW_IMAGES is not set; skipping the preview image paths");
            return;
        };
        let manifest =
            crate::snapshot::images::ImagesManifest::load(std::path::Path::new(&dir)).unwrap();
        let listed = manifest
            .images
            .iter()
            .filter(|i| !i.files.is_empty())
            .map(|i| i.path.as_str())
            .collect();
        let missing = missing_in(&listed);
        assert!(missing.is_empty(), "not in the export: {missing:#?}");
    }
}
