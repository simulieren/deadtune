//! Builds the texture addon: walks the game's VPKs, reduces every selected `.vtex_c`,
//! and streams the copies into one DeadTune-owned addon VPK. One texture is in memory
//! at a time.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use super::select::TextureDownscale;
use super::vtex::{SkipReason, Vtex};
use crate::hud::vpk::{VpkDir, VpkError, VpkWriter};

/// A reduced texture keeps at least this many pixels on its longer side. Smaller
/// textures cost little VRAM and lose too much at half size.
pub const MIN_SIDE: u16 = 128;
/// Data per `_NNN.vpk` chunk. A half-size pass over a full game is a few gigabytes.
pub const CHUNK_LIMIT: u64 = 256 << 20;

#[derive(Debug, thiserror::Error)]
pub enum AddonError {
    #[error("{0}")]
    Vpk(#[from] VpkError),
    #[error("cancelled")]
    Cancelled,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// `.vtex_c` entries seen across the game VPKs.
    pub textures: usize,
    pub reduced: usize,
    /// Original size of the reduced textures.
    pub bytes_before: u64,
    /// Size of their reduced copies, which is also the addon's payload size.
    pub bytes_after: u64,
    pub skipped: BTreeMap<SkipReason, usize>,
}

impl Stats {
    fn skip(&mut self, reason: SkipReason) {
        *self.skipped.entry(reason).or_default() += 1;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Progress<'a> {
    pub done: usize,
    pub total: usize,
    /// VPK path of the texture just processed.
    pub path: &'a str,
    pub stats: &'a Stats,
}

/// Writes `out_path` (`<stem>_dir.vpk`, plus `<stem>_NNN.vpk` chunks when the payload
/// exceeds `CHUNK_LIMIT`). The game VPKs are only read. `progress` runs after every
/// texture; `ControlFlow::Break` cancels, which removes the partial output.
/// When the same path is in several VPKs, the first in `game_vpks` wins.
pub fn build_texture_addon(
    game_vpks: &[PathBuf],
    cfg: &TextureDownscale,
    out_path: &Path,
    progress: &mut dyn FnMut(Progress) -> ControlFlow<()>,
) -> Result<Stats, AddonError> {
    let dirs = game_vpks
        .iter()
        .map(|p| VpkDir::open(p))
        .collect::<Result<Vec<_>, _>>()?;
    let mut seen = BTreeSet::new();
    let mut work: Vec<(&VpkDir, &str)> = Vec::new();
    for dir in &dirs {
        for path in dir.entries.keys() {
            if path.ends_with(".vtex_c") && seen.insert(path.as_str()) {
                work.push((dir, path));
            }
        }
    }

    let levels = cfg.factor.levels();
    let mut writer = VpkWriter::create(out_path, CHUNK_LIMIT)?;
    let mut stats = Stats::default();
    for (done, (dir, path)) in work.iter().enumerate() {
        stats.textures += 1;
        match cfg.selects(path) {
            Err(reason) => stats.skip(reason),
            Ok(()) => {
                let bytes = dir.read(path)?;
                match reduce(&bytes, levels) {
                    Err(reason) => stats.skip(reason),
                    Ok(out) => {
                        writer.add(path, &out)?;
                        stats.reduced += 1;
                        stats.bytes_before += bytes.len() as u64;
                        stats.bytes_after += out.len() as u64;
                    }
                }
            }
        }
        let report = Progress {
            done: done + 1,
            total: work.len(),
            path,
            stats: &stats,
        };
        if progress(report).is_break() {
            return Err(AddonError::Cancelled);
        }
    }
    writer.finish()?;
    Ok(stats)
}

fn reduce(bytes: &[u8], levels: u8) -> Result<Vec<u8>, SkipReason> {
    let v = Vtex::parse(bytes).map_err(|_| SkipReason::Malformed)?;
    if v.pixel_start() + v.pixel_len() != bytes.len() {
        return Err(SkipReason::Malformed);
    }
    let levels = v.reducible_levels(levels, MIN_SIDE)?;
    Ok(v.strip(bytes, levels))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::vpk;
    use crate::texture::select::{Category, Factor};
    use crate::texture::vtex::downscale;
    use crate::texture::vtex::tests::{COLOR, MASK, NOLOD, TINY};

    const WORLD_COLOR: &str = "models/props/a/color.vtex_c";
    const HERO_MASK: &str = "models/heroes/h/mask.vtex_c";

    fn game(dir: &Path) -> PathBuf {
        let files = BTreeMap::from([
            (WORLD_COLOR.to_string(), COLOR.to_vec()),
            (HERO_MASK.to_string(), MASK.to_vec()),
            ("models/props/a/color_ao.vtex_c".to_string(), COLOR.to_vec()),
            ("panorama/images/x.vtex_c".to_string(), MASK.to_vec()),
            ("models/props/a/tiny.vtex_c".to_string(), TINY.to_vec()),
            ("models/props/a/emissive.vtex_c".to_string(), NOLOD.to_vec()),
            ("models/props/a/broken.vtex_c".to_string(), b"junk".to_vec()),
            ("panorama/styles/hud.vcss_c".to_string(), b"css".to_vec()),
        ]);
        let p = dir.join("pak01_dir.vpk");
        std::fs::write(&p, vpk::write(&files)).unwrap();
        p
    }

    #[test]
    fn builds_addon_with_stats_and_progress() {
        let tmp = tempfile::tempdir().unwrap();
        let game_vpk = game(tmp.path());
        let out = tmp.path().join("addons/pak78_dir.vpk");
        let mut seen = Vec::new();
        let stats = build_texture_addon(
            &[game_vpk.clone(), game_vpk.clone()],
            &TextureDownscale::default(),
            &out,
            &mut |p| {
                seen.push((p.done, p.total, p.path.to_string()));
                ControlFlow::Continue(())
            },
        )
        .unwrap();

        assert_eq!(stats.textures, 7);
        assert_eq!(stats.reduced, 2);
        assert_eq!(stats.bytes_before, (COLOR.len() + MASK.len()) as u64);
        assert_eq!(
            stats.skipped,
            BTreeMap::from([
                (SkipReason::Lighting, 1),
                (SkipReason::NotSelected, 1),
                (SkipReason::NoMips, 1),
                (SkipReason::Unsupported, 1),
                (SkipReason::Malformed, 1),
            ])
        );
        assert_eq!(seen.len(), 7);
        assert_eq!(seen[0].0, 1);
        assert!(seen.iter().all(|s| s.1 == 7));

        let addon = VpkDir::open(&out).unwrap();
        assert_eq!(addon.entries.len(), 2);
        for (path, orig) in [(WORLD_COLOR, COLOR), (HERO_MASK, MASK)] {
            let got = addon.read(path).unwrap();
            assert_eq!(got, downscale(orig, 1).unwrap().unwrap(), "{path}");
            assert_eq!(Vtex::parse(&got).unwrap().width, 256);
        }
        assert_eq!(
            stats.bytes_after,
            addon.entries.values().map(|e| e.length as u64).sum::<u64>()
        );
    }

    #[test]
    fn quarter_respects_the_floor() {
        let tmp = tempfile::tempdir().unwrap();
        let game_vpk = game(tmp.path());
        let out = tmp.path().join("pak78_dir.vpk");
        let cfg = TextureDownscale {
            factor: Factor::Quarter,
            categories: Category::ALL.into_iter().collect(),
            exclude_lighting: false,
        };
        let stats =
            build_texture_addon(&[game_vpk], &cfg, &out, &mut |_| ControlFlow::Continue(()))
                .unwrap();
        assert_eq!(stats.reduced, 4);
        let addon = VpkDir::open(&out).unwrap();
        assert_eq!(
            Vtex::parse(&addon.read(WORLD_COLOR).unwrap())
                .unwrap()
                .width,
            128
        );
        assert_eq!(stats.skipped.get(&SkipReason::Lighting), None);
    }

    #[test]
    fn cancel_removes_output() {
        let tmp = tempfile::tempdir().unwrap();
        let game_vpk = game(tmp.path());
        let out = tmp.path().join("addons/pak78_dir.vpk");
        let err = build_texture_addon(&[game_vpk], &TextureDownscale::default(), &out, &mut |p| {
            if p.done == 2 {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })
        .unwrap_err();
        assert!(matches!(err, AddonError::Cancelled));
        assert_eq!(
            std::fs::read_dir(tmp.path().join("addons"))
                .unwrap()
                .count(),
            0
        );
    }
}
