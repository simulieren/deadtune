//! The screen-space particle disabler, sourced from the game. Laund's `pak02_dir.vpk`
//! overrides 108 screen-space particle systems (vignettes, damage flashes, ability screen
//! overlays) with one 1189-byte file, CRC 0xf3db7131, and adds
//! `materials/debug/debugempty_color_tga_fd967415.vtex_c`. That file is the game's own
//! `particles/empty.vpcf_c`, byte for byte: its RED2 block names `particles/empty.vpcf` as
//! the compiled source and GameTracking-Deadlock lists `particles/empty.vpcf_c` in pak01
//! with the same CRC and size. It is a `CParticleSystemDefinition` with no emitters,
//! initializers, operators or renderers, so the system spawns nothing. The texture is also
//! stock pak01 content (CRC 0x0ab109dd, 7556 bytes) and the stub does not reference it, so
//! we leave it out. We copy `particles/empty.vpcf_c` from the player's own pak01 to every
//! path in the groups they hide ([`particles::GROUPS`]), after checking it is still an
//! empty particle definition.

use std::collections::{BTreeMap, BTreeSet};

use super::{AddonError, particles};
use crate::hud::resource::{Resource, ResourceError};
use crate::hud::vpk::VpkDir;

pub const EMPTY_PARTICLE: &str = "particles/empty.vpcf_c";

/// DATA keys that give a particle system something to spawn, move or draw.
const BEHAVIOUR_KEYS: &[&[u8]] = &[
    b"m_Emitters",
    b"m_Initializers",
    b"m_Operators",
    b"m_PreEmissionOperators",
    b"m_Renderers",
    b"m_ForceGenerators",
    b"m_Constraints",
    b"m_Children",
];

/// Paths to file bytes for our pak: the game's empty particle at every path of each group
/// not in `keep`. Empty when every group is kept (the caller plans a removal).
pub fn build(
    game: &VpkDir,
    keep: &BTreeSet<String>,
) -> Result<BTreeMap<String, Vec<u8>>, AddonError> {
    let paths = particles::hidden_paths(keep);
    if paths.is_empty() {
        return Ok(BTreeMap::new());
    }
    let empty = game.read(EMPTY_PARTICLE)?;
    check_empty(&empty)?;
    Ok(paths
        .into_iter()
        .map(|p| (p.to_string(), empty.clone()))
        .collect())
}

/// KV3 keys are stored as plain strings in an uncompressed DATA block; a compressed or
/// reshaped block fails here rather than shipping something we cannot read.
pub(crate) fn check_empty(bytes: &[u8]) -> Result<(), AddonError> {
    let res = Resource::parse(bytes)?;
    let data = &res
        .block(b"DATA")
        .ok_or(ResourceError::MissingBlock("DATA"))?
        .data;
    let has = |key: &[u8]| data.windows(key.len()).any(|w| w == key);
    if !has(b"CParticleSystemDefinition") {
        return Err(AddonError::Invalid(format!(
            "{EMPTY_PARTICLE} in the game is not a readable particle definition"
        )));
    }
    if let Some(key) = BEHAVIOUR_KEYS.iter().find(|k| has(k)) {
        return Err(AddonError::Invalid(format!(
            "{EMPTY_PARTICLE} in the game is no longer empty (has {})",
            String::from_utf8_lossy(key)
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::verify::{Expect, verify};
    use crate::hud::crc32::crc32;
    use crate::hud::vpk;

    /// `particles/empty.vpcf_c` as GameTracking lists it in pak01: CRC 0xf3db7131, 1189 bytes.
    const EMPTY: &[u8] = include_bytes!("../../tests/fixtures/particles/empty.vpcf_c");

    fn pak(files: &[(&str, &[u8])]) -> VpkDir {
        let files: BTreeMap<String, Vec<u8>> = files
            .iter()
            .map(|(p, b)| (p.to_string(), b.to_vec()))
            .collect();
        VpkDir::in_memory(vpk::write(&files)).unwrap()
    }

    fn game() -> VpkDir {
        pak(&[
            (EMPTY_PARTICLE, EMPTY),
            ("particles/generic/player_low_health_screen.vpcf_c", b"real"),
        ])
    }

    fn all_groups() -> BTreeSet<String> {
        particles::GROUPS.iter().map(|g| g.id.to_string()).collect()
    }

    fn data_block(bytes: &[u8]) -> Vec<u8> {
        Resource::parse(bytes)
            .unwrap()
            .block(b"DATA")
            .unwrap()
            .data
            .clone()
    }

    #[test]
    fn the_upstream_stub_is_the_games_empty_particle() {
        assert_eq!(EMPTY.len(), 1189);
        assert_eq!(crc32(EMPTY), particles::STUB_CRC);
        let upstream = VpkDir::open(&particles::tests::upstream_path()).unwrap();
        for path in particles::hidden_paths(&BTreeSet::new()) {
            if !upstream.contains(path) {
                continue;
            }
            assert_eq!(upstream.read(path).unwrap(), EMPTY, "{path}");
        }
    }

    #[test]
    fn the_stub_is_an_empty_particle_definition_compiled_from_empty_vpcf() {
        let res = Resource::parse(EMPTY).unwrap();
        let names: Vec<&[u8]> = res.blocks.iter().map(|b| &b.name[..]).collect();
        assert_eq!(names, [&b"RED2"[..], b"DATA"]);
        let red2 = &res.block(b"RED2").unwrap().data;
        let has = |hay: &[u8], s: &[u8]| hay.windows(s.len()).any(|w| w == s);
        assert!(has(red2, b"particles/empty.vpcf\0"));
        assert!(!has(EMPTY, b"debugempty") && !has(EMPTY, b"materials/"));
        let data = data_block(EMPTY);
        assert_eq!(&data[..4], b"\x053VK", "KV3 version 5");
        assert_eq!(&data[20..24], &[0, 0, 0, 0], "uncompressed");
        assert!(has(&data, b"_class\0CParticleSystemDefinition\0"));
        for key in BEHAVIOUR_KEYS {
            assert!(!has(&data, key));
        }
        check_empty(EMPTY).unwrap();
    }

    #[test]
    fn build_puts_the_games_empty_particle_at_each_hidden_path() {
        let keep: BTreeSet<String> = all_groups()
            .into_iter()
            .filter(|id| id != "low_health" && id != "shiv")
            .collect();
        let files = build(&game(), &keep).unwrap();
        let want: Vec<&str> = particles::hidden_paths(&keep);
        assert_eq!(want.len(), 4 + 5);
        assert_eq!(files.keys().map(String::as_str).collect::<Vec<_>>(), {
            let mut w = want.clone();
            w.sort_unstable();
            w
        });
        assert!(files.values().all(|b| b == EMPTY));
        assert!(!files.contains_key(particles::DEBUG_TEXTURE));
    }

    #[test]
    fn hiding_everything_covers_the_upstream_list_and_passes_verify() {
        let files = build(&game(), &BTreeSet::new()).unwrap();
        assert_eq!(files.len(), 112);
        let upstream = VpkDir::open(&particles::tests::upstream_path()).unwrap();
        for (path, bytes) in files.iter().filter(|(p, _)| upstream.contains(p)) {
            assert_eq!(&upstream.read(path).unwrap(), bytes, "{path}");
        }
        let ours = VpkDir::in_memory(vpk::write(&files)).unwrap();
        let expect = Expect {
            particle_stub: Some(EMPTY.to_vec()),
            ..Expect::default()
        };
        let got = verify(&ours, &expect);
        assert!(got.is_ok(), "{got}");
        assert_eq!(got.entries, 112);
    }

    #[test]
    fn keeping_everything_builds_nothing_and_reads_nothing() {
        assert!(build(&pak(&[]), &all_groups()).unwrap().is_empty());
    }

    #[test]
    fn a_game_without_the_empty_particle_is_an_error() {
        let game = pak(&[("particles/other.vpcf_c", EMPTY)]);
        assert!(matches!(
            build(&game, &BTreeSet::new()),
            Err(AddonError::Vpk(_))
        ));
    }

    #[test]
    fn an_empty_particle_that_gained_behaviour_is_refused() {
        let mut res = Resource::parse(EMPTY).unwrap();
        let data = &mut res.blocks[1].data;
        let at = data
            .windows(14)
            .position(|w| w == b"m_previewState")
            .unwrap();
        data[at..at + 14].copy_from_slice(b"m_Renderers\0\0\0");
        let game = pak(&[(EMPTY_PARTICLE, &res.to_bytes())]);
        let err = build(&game, &BTreeSet::new()).unwrap_err().to_string();
        assert!(err.contains("no longer empty (has m_Renderers)"), "{err}");

        let game = pak(&[(EMPTY_PARTICLE, b"not a resource")]);
        assert!(matches!(
            build(&game, &BTreeSet::new()),
            Err(AddonError::Resource(_))
        ));
    }
}
