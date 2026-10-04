//! Where upstream addon files come from: a per-user cache under the state dir, filled
//! by `fetch` (GitHub, with the `fetch` feature) or by importing a file the player
//! downloaded. Files are recognised by sha256 only, so a renamed download still works
//! and a tampered one never installs.

use std::path::{Path, PathBuf};

use super::{AddonError, AddonId, Source, all, info};
use crate::backup::{atomic_write, sha256_hex};

pub const CACHE_DIR: &str = "addons";

pub fn cache_dir(state_dir: &Path) -> PathBuf {
    state_dir.join(CACHE_DIR)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Available {
    pub path: PathBuf,
    pub sha256: String,
}

fn slot(cache_dir: &Path, id: AddonId) -> Option<(PathBuf, &'static str)> {
    match info(id).source {
        Source::Upstream { file, sha256, .. } => {
            Some((cache_dir.join(id.key()).join(file), sha256))
        }
        Source::Generated => None,
    }
}

/// The cached upstream file for `id`, if present and matching its pinned sha256. A
/// corrupt or outdated cache file reads as absent so a fetch or import replaces it.
pub fn cached(cache_dir: &Path, id: AddonId) -> Result<Option<Available>, AddonError> {
    let Some((path, expected)) = slot(cache_dir, id) else {
        return Ok(None);
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let sha256 = sha256_hex(&bytes);
    Ok((sha256 == expected).then_some(Available { path, sha256 }))
}

/// Copies a downloaded `.vpk`, or every known one inside a folder (two levels deep,
/// the layout of the extracted GameBanana archive), into the cache.
pub fn import(cache_dir: &Path, from: &Path) -> Result<Vec<AddonId>, AddonError> {
    if from.is_dir() {
        let mut found = Vec::new();
        for file in vpk_files(from, 2)? {
            found.extend(import_file(cache_dir, &file)?);
        }
        found.sort_unstable();
        found.dedup();
        if found.is_empty() {
            return Err(AddonError::NotRecognised(from.to_path_buf()));
        }
        return Ok(found);
    }
    match import_file(cache_dir, from)? {
        Some(id) => Ok(vec![id]),
        None => Err(AddonError::NotRecognised(from.to_path_buf())),
    }
}

fn import_file(cache_dir: &Path, file: &Path) -> Result<Option<AddonId>, AddonError> {
    let bytes = std::fs::read(file)?;
    let sha = sha256_hex(&bytes);
    let Some(addon) = all()
        .iter()
        .find(|a| matches!(a.source, Source::Upstream { sha256, .. } if sha256 == sha))
    else {
        return Ok(None);
    };
    let (path, _) = slot(cache_dir, addon.id).expect("upstream addons have a cache slot");
    std::fs::create_dir_all(path.parent().expect("cache path has a parent"))?;
    atomic_write(&path, &bytes)?;
    Ok(Some(addon.id))
}

fn vpk_files(dir: &Path, depth: usize) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            if depth > 0 {
                out.extend(vpk_files(&path, depth - 1)?);
            }
        } else if path.extension().is_some_and(|e| e == "vpk") {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

/// Downloads the pinned upstream file into the cache, refusing bytes whose sha256 differs
/// from the pin.
#[cfg(feature = "fetch")]
pub fn fetch(cache_dir: &Path, id: AddonId) -> Result<Available, AddonError> {
    let (Source::Upstream { url, file, sha256 }, Some((path, _))) =
        (info(id).source, slot(cache_dir, id))
    else {
        return Err(AddonError::NotRecognised(PathBuf::from(id.key())));
    };
    let bytes = ureq::get(url)
        .call()
        .and_then(|mut r| r.body_mut().with_config().limit(256 << 20).read_to_vec())
        .map_err(|e| AddonError::Http(e.to_string()))?;
    let got = sha256_hex(&bytes);
    if got != sha256 {
        return Err(AddonError::ShaMismatch {
            file: file.to_string(),
            expected: sha256.to_string(),
            got,
        });
    }
    std::fs::create_dir_all(path.parent().expect("cache path has a parent"))?;
    atomic_write(&path, &bytes)?;
    Ok(Available { path, sha256: got })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn research(dir: &str, file: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../research/configs/OptimizationLock/Various Addons Relating to Performance/{dir}/{file}"
        ))
    }

    #[test]
    fn imports_a_known_file_by_hash_whatever_its_name() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = cache_dir(tmp.path());
        let renamed = tmp.path().join("whatever.vpk");
        std::fs::copy(
            research("Vindicta Scope Downscale", "pak89_dir.vpk"),
            &renamed,
        )
        .unwrap();
        assert_eq!(cached(&cache, AddonId::VindictaScope).unwrap(), None);
        assert_eq!(
            import(&cache, &renamed).unwrap(),
            vec![AddonId::VindictaScope]
        );
        let got = cached(&cache, AddonId::VindictaScope).unwrap().unwrap();
        assert_eq!(got.path, cache.join("vindicta_scope/pak89_dir.vpk"));
        assert!(got.path.is_file());
        assert_eq!(cached(&cache, AddonId::BlurDisabler).unwrap(), None);
    }

    #[test]
    fn imports_every_known_file_from_a_folder_and_rejects_strangers() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = cache_dir(tmp.path());
        let folder = tmp.path().join("Various Addons");
        for (dir, file) in [
            ("Screenspace Particle Disabler", "pak02_dir.vpk"),
            ("Sinner Light Fix Mod", "pak26_dir.vpk"),
        ] {
            std::fs::create_dir_all(folder.join(dir)).unwrap();
            std::fs::copy(research(dir, file), folder.join(dir).join(file)).unwrap();
        }
        std::fs::write(folder.join("readme.md"), "hi").unwrap();
        std::fs::write(folder.join("stranger_dir.vpk"), b"not an addon").unwrap();
        assert_eq!(
            import(&cache, &folder).unwrap(),
            vec![AddonId::ParticleDisabler, AddonId::SinnerLightFix]
        );
        let stranger = folder.join("stranger_dir.vpk");
        assert!(matches!(
            import(&cache, &stranger),
            Err(AddonError::NotRecognised(p)) if p == stranger
        ));
        let empty = tmp.path().join("empty");
        std::fs::create_dir(&empty).unwrap();
        assert!(matches!(
            import(&cache, &empty),
            Err(AddonError::NotRecognised(_))
        ));
    }

    #[test]
    fn a_corrupt_cache_file_reads_as_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = cache_dir(tmp.path());
        let path = cache.join("sinner_light_fix/pak26_dir.vpk");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"garbage").unwrap();
        assert_eq!(cached(&cache, AddonId::SinnerLightFix).unwrap(), None);
    }

    #[cfg(feature = "fetch")]
    #[test]
    #[ignore = "network"]
    fn fetch_downloads_and_verifies_every_upstream_addon() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = cache_dir(tmp.path());
        for a in all() {
            if matches!(a.source, Source::Upstream { .. }) {
                let got = fetch(&cache, a.id).unwrap();
                assert_eq!(cached(&cache, a.id).unwrap(), Some(got));
            }
        }
    }
}
