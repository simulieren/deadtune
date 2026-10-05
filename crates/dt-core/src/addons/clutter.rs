//! World clutter remover, after Laund's Clutter Be Gone (GameBanana mod 722853). Each of
//! its packs overrides a list of particle systems with the empty particle; the lists live
//! in `clutter/*.txt`, read from the packs' directory trees. Like the screen-edge disabler
//! we copy the game's own `particles/empty.vpcf_c` to every path the player hides, so
//! nothing is downloaded. "Everything" takes its list from the installed game instead.

use std::collections::{BTreeMap, BTreeSet};

use super::{AddonError, native_particles};
use crate::hud::vpk::VpkDir;

/// The Graves pack hides her wall marker unless objects fade at this distance scale or less.
pub const FADE_CONVAR: &str = "sc_fade_distance_scale_override";
pub const FADE_MAX: i64 = 4;
/// Below this scale distant effects that fade with range, such as jump pad wind, vanish.
/// Confirmed in game for 4 and 5; 100 is the lowest value presets use and is assumed safe.
pub const FADE_SAFE_MIN: f64 = 100.0;
pub const FADE_WARNING: &str = "Low values hide jump pad wind and other effects that fade with distance. The game default is -1 (Auto), which keeps them.";

/// True when the fade distance scale is low enough to hide effects past a few metres.
/// -1 (the engine default) and anything that does not parse are left alone.
pub fn fade_hides_effects(value: &str) -> bool {
    value
        .trim()
        .parse::<f64>()
        .is_ok_and(|v| (0.0..FADE_SAFE_MIN).contains(&v))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paths {
    List(&'static str),
    /// Every particle system in the game.
    EveryParticle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClutterGroup {
    pub id: &'static str,
    pub label: &'static str,
    /// One plain sentence: what disappears.
    pub detail: &'static str,
    /// What the player gives up, shown next to the checkbox.
    pub warning: Option<&'static str>,
    pub paths: Paths,
}

pub const CITY: &str = "city_ambient";
pub const EVERYTHING: &str = "everything";

pub static GROUPS: [ClutterGroup; 4] = [
    ClutterGroup {
        id: CITY,
        label: "City ambience",
        detail: "Steam, chimney smoke, street-light glow, candle flames and the crows circling the towers.",
        warning: None,
        paths: Paths::List(include_str!("clutter/city_ambient.txt")),
    },
    ClutterGroup {
        id: "graves",
        label: "Graves effects",
        detail: "Most of Graves' ambient, weapon and ability particles; what you need to play against her stays.",
        warning: Some(
            "Her grave wall marker only shows with object fade distance (sc_fade_distance_scale_override) at 4 or lower, which also hides jump pad wind past a few metres.",
        ),
        paths: Paths::List(include_str!("clutter/graves.txt")),
    },
    ClutterGroup {
        id: "walker",
        label: "Walker effects",
        detail: "Most of the Walker boss's particles; the ones that show its attacks stay.",
        warning: Some("Experimental upstream: some attacks may be harder to see."),
        paths: Paths::List(include_str!("clutter/walker.txt")),
    },
    ClutterGroup {
        id: EVERYTHING,
        label: "Everything (offline only)",
        detail: "Every particle in the game: abilities, bullets, explosions, all of it.",
        warning: Some("Do not play online with this. You will not see attacks or most abilities."),
        paths: Paths::EveryParticle,
    },
];

pub fn group(id: &str) -> Option<&'static ClutterGroup> {
    GROUPS.iter().find(|g| g.id == id)
}

/// Sorted paths to override for the groups in `hide`. `game` lists every particle for
/// [`Paths::EveryParticle`] and is not read otherwise.
pub fn hidden_paths(hide: &BTreeSet<String>, game: &VpkDir) -> Vec<String> {
    let mut out = BTreeSet::new();
    for g in GROUPS.iter().filter(|g| hide.contains(g.id)) {
        match g.paths {
            Paths::List(text) => out.extend(text.lines().map(str::to_string)),
            Paths::EveryParticle => out.extend(
                game.entries
                    .keys()
                    .filter(|p| p.starts_with("particles/") && p.ends_with(".vpcf_c"))
                    .filter(|p| *p != native_particles::EMPTY_PARTICLE)
                    .cloned(),
            ),
        }
    }
    out.into_iter().collect()
}

/// Paths to file bytes for our pak: the game's empty particle at each path. Empty when
/// nothing is hidden (the caller plans a removal).
pub fn build(game: &VpkDir, paths: &[String]) -> Result<BTreeMap<String, Vec<u8>>, AddonError> {
    if paths.is_empty() {
        return Ok(BTreeMap::new());
    }
    let empty = game.read(native_particles::EMPTY_PARTICLE)?;
    native_particles::check_empty(&empty)?;
    Ok(paths.iter().map(|p| (p.clone(), empty.clone())).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::vpk;

    const EMPTY: &[u8] = include_bytes!("../../tests/fixtures/particles/empty.vpcf_c");

    #[test]
    fn low_fade_scales_hide_effects_and_the_default_does_not() {
        for v in ["0", "4", "5", "99.9"] {
            assert!(fade_hides_effects(v), "{v}");
        }
        for v in ["-1", "100", "180", "", "nope"] {
            assert!(!fade_hides_effects(v), "{v}");
        }
    }

    fn game() -> VpkDir {
        let files: BTreeMap<String, Vec<u8>> = [
            (native_particles::EMPTY_PARTICLE, EMPTY),
            ("particles/a/fire.vpcf_c", b"real".as_slice()),
            ("particles/b/smoke.vpcf_c", b"real"),
            ("materials/x.vmat_c", b"real"),
        ]
        .into_iter()
        .map(|(p, b)| (p.to_string(), b.to_vec()))
        .collect();
        VpkDir::in_memory(vpk::write(&files)).unwrap()
    }

    fn hide(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn lists_match_the_upstream_packs() {
        let counts: Vec<(&str, usize)> = GROUPS
            .iter()
            .filter_map(|g| match g.paths {
                Paths::List(text) => Some((g.id, text.lines().count())),
                Paths::EveryParticle => None,
            })
            .collect();
        assert_eq!(counts, [(CITY, 8), ("graves", 245), ("walker", 87)]);
        for g in &GROUPS {
            if let Paths::List(text) = g.paths {
                for line in text.lines() {
                    assert!(
                        line.starts_with("particles/") && line.ends_with(".vpcf_c"),
                        "{}: {line}",
                        g.id
                    );
                }
            }
        }
    }

    #[test]
    fn hidden_paths_join_the_chosen_lists_without_reading_the_game() {
        let empty_game = VpkDir::in_memory(vpk::write(&BTreeMap::new())).unwrap();
        let paths = hidden_paths(&hide(&[CITY, "walker"]), &empty_game);
        assert_eq!(paths.len(), 8 + 87);
        assert!(paths.windows(2).all(|w| w[0] < w[1]));
        assert!(paths.contains(&"particles/environment/crows_circling_tower.vpcf_c".into()));
        assert!(hidden_paths(&hide(&[]), &empty_game).is_empty());
    }

    #[test]
    fn everything_is_every_game_particle_but_the_empty_one() {
        let paths = hidden_paths(&hide(&[EVERYTHING]), &game());
        assert_eq!(
            paths,
            ["particles/a/fire.vpcf_c", "particles/b/smoke.vpcf_c"]
        );
        let files = build(&game(), &paths).unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.values().all(|b| b == EMPTY));
    }

    #[test]
    fn nothing_hidden_builds_nothing() {
        assert!(build(&game(), &[]).unwrap().is_empty());
    }
}
