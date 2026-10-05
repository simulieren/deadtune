//! The game images DeadTune's HUD previews draw, by preview element, and where a folder of
//! decoded images keeps each one. Paths only: the pictures are read at runtime from the
//! player's pak01, an image export or a snapshot, never shipped.

use std::collections::BTreeMap;

use super::minimap_colors::IconId;

pub const EXPORT_MANIFEST: &str = "manifest.json";

/// One game image a preview draws, and its size on a 1080p screen from the game's CSS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Art {
    pub path: &'static str,
    pub size: [u16; 2],
}

const fn art(path: &'static str, w: u16, h: u16) -> Art {
    Art { path, size: [w, h] }
}

/// `#hud_minimap .backgroundImage1`, the surface map, 360 px inside the frame.
pub const MINIMAP_MAP: Art = art(
    "panorama/images/minimap/base/minimap_midtown_mid_psd.vtex_c",
    360,
    360,
);
/// `#minimap_frame`: the second of its two background images wins.
pub const MINIMAP_FRAME: Art = art("panorama/images/minimap/compass_frame_psd.vtex_c", 400, 400);
/// `.boss_icon_t1`, `.boss_icon_t3`: guardians and the patron, washed in the team colour.
pub const GUARDIAN: Art = art("panorama/images/minimap/boss_health_psd.vtex_c", 40, 40);
pub const PATRON: Art = art(
    "panorama/images/minimap/boss_health_final_psd.vtex_c",
    80,
    80,
);

/// The minimap marker for `id`; heroes draw their own portrait ([`Hero::marker`]) on a disc.
pub fn marker(id: IconId) -> Option<Art> {
    let m = |path, side| Some(art(path, side, side));
    match id {
        IconId::EnemyHero | IconId::AllyHero => None,
        IconId::LocalHero => m("panorama/images/minimap/player_marker_self_psd.vtex_c", 30),
        IconId::EnemyHeroArrow => m("panorama/images/minimap/player_arrow_enemy_psd.vtex_c", 22),
        IconId::AllyHeroArrow => m("panorama/images/minimap/player_arrow_friend_psd.vtex_c", 22),
        IconId::EnemyObjective | IconId::AllyObjective => {
            m("panorama/images/minimap/boss_health_t2_psd.vtex_c", 64)
        }
        IconId::EnemyUrnReturn => m(
            "panorama/images/minimap/soul_jar_marker_return_enemy_psd.vtex_c",
            48,
        ),
        IconId::AllyUrnReturn => m(
            "panorama/images/minimap/soul_jar_marker_return_psd.vtex_c",
            48,
        ),
        IconId::UrnSpawn | IconId::CarriedUrn => {
            m("panorama/images/minimap/soul_jar_marker_psd.vtex_c", 32)
        }
        IconId::SmallCamp => m("panorama/images/minimap/neutral_small_psd.vtex_c", 24),
        IconId::MediumCamp => m("panorama/images/minimap/neutral_medium_psd.vtex_c", 24),
        IconId::LargeCamp => m("panorama/images/minimap/neutral_large_psd.vtex_c", 24),
        IconId::Vault => m("panorama/images/minimap/neutral_vault_psd.vtex_c", 24),
        IconId::MidBoss => m(
            "panorama/images/minimap/super_neutral_marker_psd.vtex_c",
            64,
        ),
        IconId::WeaponPowerup => m("panorama/images/minimap/powerup_weapon.vsvg_c", 20),
        IconId::SoulsPowerup => m("panorama/images/minimap/powerup_souls_png.vtex_c", 20),
        IconId::HealthPowerup => m("panorama/images/minimap/powerup_health.vsvg_c", 20),
        IconId::SpiritPowerup => m("panorama/images/minimap/powerup_magic.vsvg_c", 20),
        IconId::MovementPowerup => m("panorama/images/minimap/powerup_movement.vsvg_c", 20),
        IconId::PowerupSpawn => m("panorama/images/minimap/gold_crate_marker_psd.vtex_c", 20),
        IconId::UnsecuredSouls => m("panorama/images/minimap/unsecured_souls.vsvg_c", 16),
        IconId::RejuvCrystal => m("panorama/images/minimap/rejuv_crystal_psd.vtex_c", 16),
        IconId::Shop => m("panorama/images/minimap/minimap_shop_psd.vtex_c", 20),
        IconId::Broker => m("panorama/images/minimap/minimap_icon_broker.vsvg_c", 16),
        IconId::Teleporter => m(
            "panorama/images/minimap/minimap_teleporter_go_right.vsvg_c",
            16,
        ),
        IconId::Stairs => m("panorama/images/minimap/icon_stairs_mid.vsvg_c", 12),
    }
}

/// A stand-in hero for the previews: one of twelve that have every image variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hero {
    /// `.player #MainImage` on the minimap.
    pub marker: Art,
    /// The top bar portrait (`#HeroImage`, 88x94). The game picks the variant in code;
    /// `_vertical` is what its frog stand-in layout uses.
    pub portrait: Art,
}

macro_rules! hero {
    ($name:literal) => {
        Hero {
            marker: art(
                concat!("panorama/images/heroes/", $name, "_mm_psd.vtex_c"),
                24,
                24,
            ),
            portrait: art(
                concat!("panorama/images/heroes/", $name, "_vertical_psd.vtex_c"),
                88,
                94,
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
/// `#UltimateAbilityIconMini` when the ultimate is ready.
pub const ULTIMATE: Art = art("panorama/images/hud/top_bar/icon_ultimate.vsvg_c", 22, 22);
/// `.DeathIcon`, washed `#FF5656`.
pub const SKULL: Art = art("panorama/images/hud/icons/skull.vsvg_c", 50, 50);
/// `#HealthBar_Border` beside each portrait.
pub const PORTRAIT_HEALTH: Art = art(
    "panorama/images/hud/healthbar/healthbar_backer_vert_border.vsvg_c",
    16,
    50,
);

/// `.healthBacker`, washed vivacious green, behind the health number (90% of 100x65).
pub const HEALTH_BACKER: Art = art("panorama/images/hud/core/health_backer.vsvg_c", 90, 65);
/// `#health_bar .ProgressBarLeft`.
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
        GUARDIAN,
        PATRON,
        TEAM_REJUV,
        REJUV_ICON,
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

/// The files a decoded-image folder (a "Save all images" export, or a snapshot's `text/`)
/// keeps for `game_path`, best first: `x_psd.vtex_c` is `x_psd.png`; `x.vsvg_c` is `x.svg`,
/// then its rendered `x.png`.
pub fn export_names(game_path: &str) -> Vec<String> {
    if let Some(stem) = game_path.strip_suffix(".vtex_c") {
        vec![format!("{stem}.png")]
    } else if let Some(stem) = game_path.strip_suffix(".vsvg_c") {
        vec![format!("{stem}.svg"), format!("{stem}.png")]
    } else {
        Vec::new()
    }
}

/// Game path -> exported files, from an export's `manifest.json`. Lenient about the shape:
/// any object naming a compiled image path (`path`, `game_path` or `source`) with its file
/// names (`files`, or `png`/`svg`/`file`) counts, wherever it sits.
pub fn manifest_names(json: &str) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(json) {
        collect(&value, &mut out);
    }
    out
}

fn collect(value: &serde_json::Value, out: &mut BTreeMap<String, Vec<String>>) {
    use serde_json::Value;
    match value {
        Value::Array(items) => items.iter().for_each(|v| collect(v, out)),
        Value::Object(map) => {
            let game = ["path", "game_path", "source"]
                .iter()
                .filter_map(|k| map.get(*k)?.as_str())
                .find(|p| p.ends_with(".vtex_c") || p.ends_with(".vsvg_c"));
            if let Some(game) = game {
                let mut files: Vec<String> = map
                    .get("files")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|f| f.as_str().map(str::to_string))
                    .collect();
                for key in ["svg", "png", "file"] {
                    if let Some(f) = map.get(key).and_then(Value::as_str) {
                        files.push(f.to_string());
                    }
                }
                files.sort_by_key(|f| !f.ends_with(".svg"));
                if !files.is_empty() {
                    out.insert(game.to_string(), files);
                }
            }
            map.values().for_each(|v| collect(v, out));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mapped_path_is_a_replaceable_image() {
        let all = all();
        assert!(all.len() > 50, "{}", all.len());
        for a in &all {
            assert!(super::super::icons::target(a.path).is_ok(), "{}", a.path);
            assert!(a.size[0] > 0 && a.size[1] > 0, "{}", a.path);
        }
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
        let listed: std::collections::BTreeSet<&str> =
            tsv.lines().filter_map(|l| l.split('\t').next()).collect();
        let missing: Vec<&str> = all()
            .iter()
            .map(|a| a.path)
            .filter(|p| !listed.contains(p))
            .collect();
        assert!(missing.is_empty(), "not in pak01: {missing:#?}");
    }

    #[test]
    fn export_names_follow_the_snapshot_text_layout() {
        assert_eq!(
            export_names("panorama/images/minimap/gold_psd.vtex_c"),
            ["panorama/images/minimap/gold_psd.png"]
        );
        assert_eq!(
            export_names("panorama/images/hud/top_bar/icon_ultimate.vsvg_c"),
            [
                "panorama/images/hud/top_bar/icon_ultimate.svg",
                "panorama/images/hud/top_bar/icon_ultimate.png"
            ]
        );
        assert!(export_names("panorama/styles/hud.vcss_c").is_empty());
    }

    #[test]
    fn manifest_names_read_any_reasonable_shape() {
        let json = r#"{"build":"1","images":[
            {"path":"panorama/images/a_psd.vtex_c","files":["panorama/images/a_psd.png"]},
            {"game_path":"panorama/images/b.vsvg_c","png":"x/b.png","svg":"x/b.svg"},
            {"path":"panorama/images/c_psd.vtex_c","error":"undecodable"}
        ]}"#;
        let names = manifest_names(json);
        assert_eq!(
            names["panorama/images/a_psd.vtex_c"],
            ["panorama/images/a_psd.png"]
        );
        assert_eq!(names["panorama/images/b.vsvg_c"], ["x/b.svg", "x/b.png"]);
        assert!(!names.contains_key("panorama/images/c_psd.vtex_c"));
        assert!(manifest_names("not json").is_empty());
    }
}
